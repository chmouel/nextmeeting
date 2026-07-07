//! Background scheduler for calendar sync.
//!
//! This module provides a scheduler that periodically syncs calendar data
//! with support for:
//! - Configurable sync intervals
//! - Jitter to avoid thundering herd
//! - Cooldown after manual refresh
//! - Exponential backoff on errors

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::{RwLock, mpsc};
use tokio::time::Instant;
use tracing::{debug, error, info, warn};

/// Threshold for detecting system sleep/wake via wall-clock time jumps.
/// If actual wake time differs from expected by more than this, we assume system slept.
const TIME_JUMP_THRESHOLD_SECS: i64 = 60;

/// Scheduler configuration.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Base interval between syncs.
    pub sync_interval: Duration,
    /// Maximum jitter to add to sync interval (as fraction 0.0-1.0).
    pub jitter_fraction: f64,
    /// Cooldown period after a manual refresh.
    pub refresh_cooldown: Duration,
    /// Initial backoff duration on error.
    pub initial_backoff: Duration,
    /// Maximum backoff duration.
    pub max_backoff: Duration,
    /// Backoff multiplier.
    pub backoff_multiplier: f64,
    /// Maximum consecutive failures before giving up.
    pub max_consecutive_failures: u32,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            sync_interval: Duration::from_secs(300),   // 5 minutes
            jitter_fraction: 0.1,                      // 10% jitter
            refresh_cooldown: Duration::from_secs(30), // 30 seconds
            initial_backoff: Duration::from_secs(5),   // 5 seconds
            max_backoff: Duration::from_secs(300),     // 5 minutes
            backoff_multiplier: 2.0,
            max_consecutive_failures: 10,
        }
    }
}

impl SchedulerConfig {
    /// Creates a new scheduler config with the given sync interval.
    pub fn new(sync_interval: Duration) -> Self {
        Self {
            sync_interval,
            ..Default::default()
        }
    }

    /// Builder: set jitter fraction.
    pub fn with_jitter(mut self, fraction: f64) -> Self {
        self.jitter_fraction = fraction.clamp(0.0, 1.0);
        self
    }

    /// Builder: set refresh cooldown.
    pub fn with_refresh_cooldown(mut self, cooldown: Duration) -> Self {
        self.refresh_cooldown = cooldown;
        self
    }

    /// Builder: set backoff parameters.
    pub fn with_backoff(mut self, initial: Duration, max: Duration, multiplier: f64) -> Self {
        self.initial_backoff = initial;
        self.max_backoff = max;
        self.backoff_multiplier = multiplier;
        self
    }

    /// Calculates the next sync delay with jitter.
    pub fn next_sync_delay(&self) -> Duration {
        let base = self.sync_interval.as_secs_f64();
        let jitter_range = base * self.jitter_fraction;
        let jitter = rand_jitter(jitter_range);
        Duration::from_secs_f64(base + jitter)
    }

    /// Calculates backoff delay based on consecutive failures.
    ///
    /// The delay grows exponentially and is capped at `max_backoff`, so a
    /// persistently failing provider is probed at a slow, steady cadence
    /// rather than being abandoned.
    pub fn backoff_delay(&self, consecutive_failures: u32) -> Duration {
        if consecutive_failures == 0 {
            return Duration::ZERO;
        }

        let base = self.initial_backoff.as_secs_f64();
        let multiplier = self
            .backoff_multiplier
            .powi(consecutive_failures.min(64) as i32 - 1);
        let delay = base * multiplier;
        let max = self.max_backoff.as_secs_f64();

        Duration::from_secs_f64(delay.min(max))
    }

    /// Calculates backoff delay with jitter applied, to avoid synchronised
    /// retries across instances (thundering herd). The result is re-clamped
    /// to `max_backoff` so jitter can never push the effective delay past
    /// the documented ceiling.
    pub fn backoff_delay_with_jitter(&self, consecutive_failures: u32) -> Duration {
        let base = self.backoff_delay(consecutive_failures);
        if base.is_zero() {
            return base;
        }
        let jitter = rand_jitter(base.as_secs_f64() * self.jitter_fraction);
        let max = self.max_backoff.as_secs_f64();
        Duration::from_secs_f64((base.as_secs_f64() + jitter).clamp(0.0, max))
    }
}

/// Simple pseudo-random jitter generator.
/// Uses the current time to generate a value in [-range, range].
fn rand_jitter(range: f64) -> f64 {
    use std::time::SystemTime;

    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();

    // Map nanos to [-range, range]
    let fraction = (nanos as f64) / (1_000_000_000.0);
    (fraction * 2.0 - 1.0) * range
}

/// Commands that can be sent to the scheduler.
#[derive(Debug, Clone)]
pub enum SchedulerCommand {
    /// Trigger an immediate sync.
    SyncNow,
    /// Trigger a sync, bypassing cooldown if force is true.
    Refresh { force: bool },
    /// Pause the scheduler.
    Pause,
    /// Resume the scheduler.
    Resume,
    /// Stop the scheduler.
    Stop,
}

/// Scheduler state.
#[derive(Debug)]
pub struct SchedulerState {
    /// Whether the scheduler is paused.
    pub paused: bool,
    /// Number of consecutive sync failures.
    pub consecutive_failures: u32,
    /// Last successful sync time.
    pub last_sync: Option<DateTime<Utc>>,
    /// Last sync attempt time.
    pub last_attempt: Option<DateTime<Utc>>,
    /// Last error message.
    pub last_error: Option<String>,
    /// Last manual refresh time (for cooldown).
    pub last_refresh: Option<Instant>,
    /// Expected wake time for time-jump detection (system sleep/wake).
    pub expected_wake: Option<DateTime<Utc>>,
}

impl Default for SchedulerState {
    fn default() -> Self {
        Self::new()
    }
}

impl SchedulerState {
    /// Creates a new scheduler state.
    pub fn new() -> Self {
        Self {
            paused: false,
            consecutive_failures: 0,
            last_sync: None,
            last_attempt: None,
            last_error: None,
            last_refresh: None,
            expected_wake: None,
        }
    }

    /// Records a successful sync.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.last_sync = Some(Utc::now());
        self.last_attempt = self.last_sync;
        self.last_error = None;
    }

    /// Records a failed sync.
    pub fn record_failure(&mut self, error: impl Into<String>) {
        self.consecutive_failures += 1;
        self.last_attempt = Some(Utc::now());
        self.last_error = Some(error.into());
    }

    /// Records a manual refresh.
    pub fn record_refresh(&mut self) {
        self.last_refresh = Some(Instant::now());
    }

    /// Returns true if we're in cooldown period.
    pub fn in_cooldown(&self, cooldown: Duration) -> bool {
        if let Some(last_refresh) = self.last_refresh {
            last_refresh.elapsed() < cooldown
        } else {
            false
        }
    }

    /// Returns the time since last sync.
    pub fn time_since_sync(&self) -> Option<Duration> {
        self.last_sync.map(|last| {
            let elapsed = Utc::now() - last;
            Duration::from_secs(elapsed.num_seconds().max(0) as u64)
        })
    }
}

/// Shared scheduler state.
pub type SharedSchedulerState = Arc<RwLock<SchedulerState>>;

/// Creates a new shared scheduler state.
pub fn new_scheduler_state() -> SharedSchedulerState {
    Arc::new(RwLock::new(SchedulerState::new()))
}

/// The scheduler manages periodic background sync tasks.
pub struct Scheduler {
    config: SchedulerConfig,
    state: SharedSchedulerState,
    command_tx: mpsc::Sender<SchedulerCommand>,
    command_rx: Option<mpsc::Receiver<SchedulerCommand>>,
}

impl Scheduler {
    /// Creates a new scheduler with the given configuration.
    pub fn new(config: SchedulerConfig) -> Self {
        let (command_tx, command_rx) = mpsc::channel(16);
        Self {
            config,
            state: new_scheduler_state(),
            command_tx,
            command_rx: Some(command_rx),
        }
    }

    /// Returns a handle for sending commands to the scheduler.
    pub fn handle(&self) -> SchedulerHandle {
        SchedulerHandle {
            command_tx: self.command_tx.clone(),
            state: self.state.clone(),
        }
    }

    /// Returns the shared state.
    pub fn state(&self) -> SharedSchedulerState {
        self.state.clone()
    }

    /// Runs the scheduler loop with the given sync function.
    ///
    /// The sync function is called periodically and should return Ok(()) on success
    /// or an error message on failure.
    pub async fn run<F, Fut>(mut self, sync_fn: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), String>> + Send,
    {
        let mut command_rx = self.command_rx.take().expect("run called twice");

        info!(
            interval_secs = self.config.sync_interval.as_secs(),
            "Scheduler started"
        );

        // Initial sync
        self.do_sync(&sync_fn).await;

        loop {
            let delay = self.calculate_next_delay().await;
            debug!(delay_secs = delay.as_secs(), "Scheduling next sync");

            // Record expected wake time for time-jump detection
            let expected_wake = Utc::now() + chrono::Duration::from_std(delay).unwrap_or_default();
            self.state.write().await.expected_wake = Some(expected_wake);

            tokio::select! {
                _ = tokio::time::sleep(delay) => {
                    // Check for time jump (system sleep/wake)
                    let now = Utc::now();
                    let state = self.state.read().await;
                    let expected = state.expected_wake;
                    let paused = state.paused;
                    drop(state);

                    if let Some(expected) = expected {
                        let time_diff = (now - expected).num_seconds().abs();
                        if time_diff > TIME_JUMP_THRESHOLD_SECS {
                            info!(
                                expected_wake = %expected,
                                actual_wake = %now,
                                diff_secs = time_diff,
                                "Detected system sleep/wake, triggering immediate sync"
                            );
                            // Clear expected_wake and proceed to sync
                            self.state.write().await.expected_wake = None;
                            self.do_sync(&sync_fn).await;
                            continue;
                        }
                    }

                    if paused {
                        debug!("Scheduler paused, skipping sync");
                        continue;
                    }

                    self.do_sync(&sync_fn).await;
                }
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(SchedulerCommand::SyncNow) => {
                            debug!("Received SyncNow command");
                            self.do_sync(&sync_fn).await;
                        }
                        Some(SchedulerCommand::Refresh { force }) => {
                            debug!(force = force, "Received Refresh command");
                            let state = self.state.read().await;
                            let in_cooldown = state.in_cooldown(self.config.refresh_cooldown);
                            drop(state);

                            if force || !in_cooldown {
                                self.state.write().await.record_refresh();
                                self.do_sync(&sync_fn).await;
                            } else {
                                debug!("Skipping refresh due to cooldown");
                            }
                        }
                        Some(SchedulerCommand::Pause) => {
                            info!("Scheduler paused");
                            self.state.write().await.paused = true;
                        }
                        Some(SchedulerCommand::Resume) => {
                            info!("Scheduler resumed");
                            self.state.write().await.paused = false;
                        }
                        Some(SchedulerCommand::Stop) | None => {
                            info!("Scheduler stopping");
                            break;
                        }
                    }
                }
            }
        }
    }

    async fn calculate_next_delay(&self) -> Duration {
        let state = self.state.read().await;

        // If we have consecutive failures, use jittered backoff
        if state.consecutive_failures > 0 {
            let backoff = self
                .config
                .backoff_delay_with_jitter(state.consecutive_failures);
            debug!(
                failures = state.consecutive_failures,
                backoff_secs = backoff.as_secs(),
                "Using backoff delay"
            );
            return backoff;
        }

        // If we just had a manual refresh, use cooldown
        if state.in_cooldown(self.config.refresh_cooldown)
            && let Some(last_refresh) = state.last_refresh
        {
            let remaining = self.config.refresh_cooldown - last_refresh.elapsed();
            let next_delay = self.config.next_sync_delay();
            return remaining.max(next_delay);
        }

        // Normal sync interval with jitter
        self.config.next_sync_delay()
    }

    #[tracing::instrument(skip(self, sync_fn), fields(sync_duration_ms))]
    async fn do_sync<F, Fut>(&self, sync_fn: &F)
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        use std::panic::AssertUnwindSafe;

        use futures_util::FutureExt;
        use tracing::Span;

        let state = self.state.read().await;
        let failures = state.consecutive_failures;
        drop(state);

        // Never give up permanently: past the failure ceiling we keep probing
        // at the (capped) backoff cadence so the daemon can recover on its
        // own, and manual refreshes always get through.
        if failures >= self.config.max_consecutive_failures {
            warn!(
                failures = failures,
                max = self.config.max_consecutive_failures,
                "In degraded mode after repeated failures, probing sync"
            );
        }

        let start = std::time::Instant::now();
        debug!("Starting calendar sync");

        let result = AssertUnwindSafe(sync_fn()).catch_unwind().await;

        match result {
            Ok(Ok(())) => {
                let duration = start.elapsed();
                info!(
                    sync_duration_ms = duration.as_millis(),
                    "Sync completed successfully"
                );
                if tracing::enabled!(tracing::Level::DEBUG) {
                    Span::current().record("sync_duration_ms", duration.as_millis());
                }
                self.state.write().await.record_success();
            }
            Ok(Err(e)) => {
                let duration = start.elapsed();
                warn!(
                    error = %e,
                    sync_duration_ms = duration.as_millis(),
                    "Sync failed"
                );
                self.state.write().await.record_failure(e);
            }
            Err(panic_info) => {
                let duration = start.elapsed();
                let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                error!(
                    error = %panic_msg,
                    sync_duration_ms = duration.as_millis(),
                    "Sync panicked — scheduler continues"
                );
                self.state
                    .write()
                    .await
                    .record_failure(format!("panic: {}", panic_msg));
            }
        }
    }
}

/// Handle for sending commands to a running scheduler.
#[derive(Clone, Debug)]
pub struct SchedulerHandle {
    command_tx: mpsc::Sender<SchedulerCommand>,
    state: SharedSchedulerState,
}

impl SchedulerHandle {
    /// Triggers an immediate sync.
    pub async fn sync_now(&self) -> Result<(), mpsc::error::SendError<SchedulerCommand>> {
        self.command_tx.send(SchedulerCommand::SyncNow).await
    }

    /// Triggers a refresh (respects cooldown unless force is true).
    pub async fn refresh(
        &self,
        force: bool,
    ) -> Result<(), mpsc::error::SendError<SchedulerCommand>> {
        self.command_tx
            .send(SchedulerCommand::Refresh { force })
            .await
    }

    /// Pauses the scheduler.
    pub async fn pause(&self) -> Result<(), mpsc::error::SendError<SchedulerCommand>> {
        self.command_tx.send(SchedulerCommand::Pause).await
    }

    /// Resumes the scheduler.
    pub async fn resume(&self) -> Result<(), mpsc::error::SendError<SchedulerCommand>> {
        self.command_tx.send(SchedulerCommand::Resume).await
    }

    /// Stops the scheduler.
    pub async fn stop(&self) -> Result<(), mpsc::error::SendError<SchedulerCommand>> {
        self.command_tx.send(SchedulerCommand::Stop).await
    }

    /// Returns the current scheduler state.
    pub async fn state(&self) -> SchedulerState {
        self.state.read().await.clone()
    }

    /// Returns true if the scheduler is paused.
    pub async fn is_paused(&self) -> bool {
        self.state.read().await.paused
    }
}

impl Clone for SchedulerState {
    fn clone(&self) -> Self {
        Self {
            paused: self.paused,
            consecutive_failures: self.consecutive_failures,
            last_sync: self.last_sync,
            last_attempt: self.last_attempt,
            last_error: self.last_error.clone(),
            last_refresh: self.last_refresh,
            expected_wake: self.expected_wake,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn config_default() {
        let config = SchedulerConfig::default();
        assert_eq!(config.sync_interval, Duration::from_secs(300));
        assert!(config.jitter_fraction > 0.0);
    }

    #[test]
    fn config_next_sync_delay() {
        let config = SchedulerConfig::new(Duration::from_secs(60)).with_jitter(0.1);

        let delay = config.next_sync_delay();
        // Should be within 10% jitter
        assert!(delay.as_secs_f64() >= 54.0);
        assert!(delay.as_secs_f64() <= 66.0);
    }

    #[test]
    fn config_backoff_delay() {
        let config = SchedulerConfig::default().with_backoff(
            Duration::from_secs(5),
            Duration::from_secs(300),
            2.0,
        );

        assert_eq!(config.backoff_delay(0), Duration::ZERO);
        assert_eq!(config.backoff_delay(1), Duration::from_secs(5));
        assert_eq!(config.backoff_delay(2), Duration::from_secs(10));
        assert_eq!(config.backoff_delay(3), Duration::from_secs(20));

        // Should be capped at max
        assert_eq!(config.backoff_delay(10), Duration::from_secs(300));
    }

    #[test]
    fn state_record_success() {
        let mut state = SchedulerState::new();
        state.consecutive_failures = 5;

        state.record_success();

        assert_eq!(state.consecutive_failures, 0);
        assert!(state.last_sync.is_some());
        assert!(state.last_error.is_none());
    }

    #[test]
    fn state_record_failure() {
        let mut state = SchedulerState::new();

        state.record_failure("test error");

        assert_eq!(state.consecutive_failures, 1);
        assert!(state.last_attempt.is_some());
        assert_eq!(state.last_error, Some("test error".to_string()));
    }

    #[test]
    fn state_cooldown() {
        let mut state = SchedulerState::new();
        let cooldown = Duration::from_millis(50);

        assert!(!state.in_cooldown(cooldown));

        state.record_refresh();
        assert!(state.in_cooldown(cooldown));

        std::thread::sleep(Duration::from_millis(60));
        assert!(!state.in_cooldown(cooldown));
    }

    #[test]
    fn config_backoff_delay_with_jitter_bounds() {
        let config = SchedulerConfig::default().with_jitter(0.1).with_backoff(
            Duration::from_secs(10),
            Duration::from_secs(300),
            2.0,
        );

        for failures in 1..=5 {
            let base = config.backoff_delay(failures).as_secs_f64();
            let jittered = config.backoff_delay_with_jitter(failures).as_secs_f64();
            assert!(jittered >= base * 0.9 - f64::EPSILON);
            assert!(jittered <= base * 1.1 + f64::EPSILON);
        }

        // No jitter when there is no backoff.
        assert_eq!(config.backoff_delay_with_jitter(0), Duration::ZERO);
    }

    #[test]
    fn config_backoff_delay_with_jitter_never_exceeds_max_backoff() {
        // Once consecutive failures push the base delay up to the cap,
        // jitter must not be able to push the result past it.
        let config = SchedulerConfig::default().with_jitter(0.5).with_backoff(
            Duration::from_secs(10),
            Duration::from_secs(20),
            2.0,
        );

        // failures=2 -> base already saturates at max_backoff (20s).
        for _ in 0..50 {
            let jittered = config.backoff_delay_with_jitter(2);
            assert!(jittered <= Duration::from_secs(20));
        }
    }

    #[test]
    fn config_backoff_delay_large_failure_count_stays_capped() {
        let config = SchedulerConfig::default().with_backoff(
            Duration::from_secs(5),
            Duration::from_secs(300),
            2.0,
        );

        // Must not overflow or produce nonsense for very large counts.
        assert_eq!(config.backoff_delay(1000), Duration::from_secs(300));
    }

    #[tokio::test]
    async fn scheduler_probes_past_failure_ceiling_without_giving_up() {
        // With max_consecutive_failures = 2, the old behaviour wedged the
        // scheduler permanently after the 2nd failure. Now it keeps probing
        // at the (capped) backoff cadence indefinitely.
        let mut config = SchedulerConfig::new(Duration::from_secs(60)).with_backoff(
            Duration::from_millis(5),
            Duration::from_millis(10),
            2.0,
        );
        config.max_consecutive_failures = 2;

        let scheduler = Scheduler::new(config);
        let state = scheduler.state();
        let handle = scheduler.handle();

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        // Always fails: proves the scheduler keeps calling sync_fn well past
        // the failure ceiling instead of returning early forever.
        let scheduler_task = tokio::spawn(async move {
            scheduler
                .run(move || {
                    let count = call_count_clone.clone();
                    async move {
                        count.fetch_add(1, Ordering::SeqCst);
                        Err("always fails".to_string())
                    }
                })
                .await;
        });

        // Give it time to accumulate well past max_consecutive_failures (2)
        // purely through periodic probing.
        tokio::time::sleep(Duration::from_millis(150)).await;

        let failures = state.read().await.consecutive_failures;
        assert!(
            failures > 2,
            "scheduler must keep probing past the failure ceiling, got {failures} failures"
        );
        let calls_after_probing = call_count.load(Ordering::SeqCst);
        assert!(
            calls_after_probing > 2,
            "sync_fn must keep being invoked in degraded mode, got {calls_after_probing} calls"
        );

        handle.stop().await.unwrap();
        scheduler_task.await.unwrap();
    }

    #[tokio::test]
    async fn scheduler_forced_refresh_recovers_from_degraded_mode() {
        // Pause the scheduler so periodic probing cannot itself cause a
        // success — this isolates the forced-refresh path from ordinary
        // recovery and eliminates any timing race between the two.
        let mut config = SchedulerConfig::new(Duration::from_secs(60)).with_backoff(
            Duration::from_millis(5),
            Duration::from_millis(10),
            2.0,
        );
        config.max_consecutive_failures = 2;

        let scheduler = Scheduler::new(config);
        let state = scheduler.state();
        let handle = scheduler.handle();

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();
        let allow_success = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let allow_success_clone = allow_success.clone();

        let scheduler_task = tokio::spawn(async move {
            scheduler
                .run(move || {
                    let count = call_count_clone.clone();
                    let allow_success = allow_success_clone.clone();
                    async move {
                        count.fetch_add(1, Ordering::SeqCst);
                        if allow_success.load(Ordering::SeqCst) {
                            Ok(())
                        } else {
                            Err("still failing".to_string())
                        }
                    }
                })
                .await;
        });

        // Drive it well past the failure ceiling via periodic probing alone.
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(state.read().await.consecutive_failures > 2);

        // Pause: periodic syncs must now stop entirely.
        handle.pause().await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await; // let in-flight sleep settle
        let calls_while_pausing = call_count.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            calls_while_pausing,
            "no periodic sync should occur while paused"
        );

        // Only now allow success, and require a forced refresh — sent while
        // still paused — to be the one that triggers it.
        allow_success.store(true, std::sync::atomic::Ordering::SeqCst);
        handle.refresh(true).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            calls_while_pausing + 1,
            "forced refresh must be the sole cause of the next sync attempt"
        );
        assert_eq!(
            state.read().await.consecutive_failures,
            0,
            "the forced refresh's success must reset the failure count"
        );

        handle.stop().await.unwrap();
        scheduler_task.await.unwrap();
    }

    #[tokio::test]
    async fn scheduler_commands() {
        let config = SchedulerConfig::new(Duration::from_secs(60));
        let scheduler = Scheduler::new(config);
        let handle = scheduler.handle();

        let sync_count = Arc::new(AtomicU32::new(0));
        let sync_count_clone = sync_count.clone();

        // Run scheduler in background
        let scheduler_task = tokio::spawn(async move {
            scheduler
                .run(move || {
                    let count = sync_count_clone.clone();
                    async move {
                        count.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }
                })
                .await;
        });

        // Wait for initial sync
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(sync_count.load(Ordering::SeqCst) >= 1);

        // Trigger manual sync
        handle.sync_now().await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(sync_count.load(Ordering::SeqCst) >= 2);

        // Pause and verify
        handle.pause().await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(handle.is_paused().await);

        // Resume
        handle.resume().await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!handle.is_paused().await);

        // Stop
        handle.stop().await.unwrap();
        scheduler_task.await.unwrap();
    }

    #[tokio::test]
    async fn scheduler_backoff_on_failure() {
        let config = SchedulerConfig::new(Duration::from_secs(1)).with_backoff(
            Duration::from_millis(10),
            Duration::from_millis(100),
            2.0,
        );

        let scheduler = Scheduler::new(config);
        let state = scheduler.state();
        let handle = scheduler.handle();

        let fail_count = Arc::new(AtomicU32::new(0));
        let fail_count_clone = fail_count.clone();

        let scheduler_task = tokio::spawn(async move {
            scheduler
                .run(move || {
                    let count = fail_count_clone.clone();
                    async move {
                        let n = count.fetch_add(1, Ordering::SeqCst);
                        if n < 3 {
                            Err(format!("Failure {}", n))
                        } else {
                            Ok(())
                        }
                    }
                })
                .await;
        });

        // Wait for initial failures and recovery
        tokio::time::sleep(Duration::from_millis(200)).await;

        let current_state = state.read().await;
        // Should have recovered after 3 failures
        assert!(fail_count.load(Ordering::SeqCst) >= 3);
        drop(current_state);

        handle.stop().await.unwrap();
        scheduler_task.await.unwrap();
    }

    #[tokio::test]
    async fn scheduler_detects_time_jump() {
        // This test verifies that expected_wake is set and can be used for time-jump detection
        let config = SchedulerConfig::new(Duration::from_millis(100));
        let scheduler = Scheduler::new(config);
        let state = scheduler.state();
        let handle = scheduler.handle();

        let sync_count = Arc::new(AtomicU32::new(0));
        let sync_count_clone = sync_count.clone();

        let scheduler_task = tokio::spawn(async move {
            scheduler
                .run(move || {
                    let count = sync_count_clone.clone();
                    async move {
                        count.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }
                })
                .await;
        });

        // Wait for initial sync
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Verify expected_wake is set during normal operation
        {
            let current_state = state.read().await;
            assert!(
                current_state.expected_wake.is_some(),
                "expected_wake should be set"
            );
        }

        // Wait for next sync cycle
        tokio::time::sleep(Duration::from_millis(120)).await;

        // Verify expected_wake is updated for next cycle
        {
            let current_state = state.read().await;
            assert!(
                current_state.expected_wake.is_some(),
                "expected_wake should still be set"
            );
        }

        handle.stop().await.unwrap();
        scheduler_task.await.unwrap();
    }

    #[tokio::test]
    async fn scheduler_survives_sync_panic() {
        let config = SchedulerConfig::new(Duration::from_secs(60));
        let scheduler = Scheduler::new(config);
        let state = scheduler.state();
        let handle = scheduler.handle();

        let sync_count = Arc::new(AtomicU32::new(0));
        let sync_count_clone = sync_count.clone();

        let scheduler_task = tokio::spawn(async move {
            scheduler
                .run(move || {
                    let count = sync_count_clone.clone();
                    async move {
                        let n = count.fetch_add(1, Ordering::SeqCst);
                        if n == 0 {
                            panic!("simulated sync panic");
                        }
                        Ok(())
                    }
                })
                .await;
        });

        // Wait for initial sync (which panics)
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(sync_count.load(Ordering::SeqCst) >= 1);

        // Verify the panic was recorded as a failure
        {
            let s = state.read().await;
            assert!(
                s.consecutive_failures > 0,
                "panic should be recorded as a failure"
            );
            assert!(s.last_error.as_ref().unwrap().contains("panic:"));
        }

        // Trigger a manual sync — scheduler should still be alive
        handle
            .sync_now()
            .await
            .expect("scheduler channel should still be open");
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Second sync should succeed
        assert!(
            sync_count.load(Ordering::SeqCst) >= 2,
            "scheduler should continue after panic"
        );

        // Verify recovery
        {
            let s = state.read().await;
            assert_eq!(s.consecutive_failures, 0, "should have recovered");
        }

        handle.stop().await.unwrap();
        scheduler_task.await.unwrap();
    }
}
