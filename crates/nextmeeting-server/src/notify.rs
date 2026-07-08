//! Desktop notification engine for upcoming meetings.
//!
//! This module provides the notification system that alerts users about
//! upcoming meetings. It supports:
//! - Configurable notification timing (e.g., 5, 10, 15 minutes before)
//! - Snooze functionality
//! - Deduplication to avoid repeated notifications

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local, Utc};
use notify_rust::Notification;
#[cfg(target_os = "linux")]
use notify_rust::Urgency;
use sha2::{Digest, Sha256};
use tokio::sync::{RwLock, Semaphore};
use tracing::{debug, error, info};

use nextmeeting_core::MeetingView;

/// Configuration for the notification engine.
#[derive(Debug, Clone)]
pub struct NotifyConfig {
    /// Minutes before event to send notifications.
    pub notify_minutes: Vec<u32>,
    /// Application name for notifications.
    pub app_name: String,
    /// Default notification timeout in seconds.
    pub timeout_secs: u32,
    /// Whether notifications are enabled.
    pub enabled: bool,
    /// Override urgency level ("low", "normal", "critical").
    pub urgency: Option<String>,
    /// Override expiry/timeout in seconds.
    pub expiry_secs: Option<u32>,
    /// Custom notification icon path.
    pub icon_path: Option<String>,
    /// Time to send morning agenda notification (format: "HH:MM").
    pub morning_agenda_time: Option<String>,
    /// Minutes before meeting end to trigger end-warning notifications.
    ///
    /// If not set, end-warning notifications are disabled.
    pub end_warning_minutes: Option<u32>,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            notify_minutes: vec![15, 5, 1],
            app_name: "nextmeeting".to_string(),
            timeout_secs: 10,
            enabled: true,
            urgency: None,
            expiry_secs: None,
            icon_path: None,
            morning_agenda_time: None,
            end_warning_minutes: None,
        }
    }
}

impl NotifyConfig {
    /// Creates a new notification config with the given notification minutes.
    pub fn new(notify_minutes: Vec<u32>) -> Self {
        Self {
            notify_minutes,
            ..Default::default()
        }
    }

    /// Builder: set app name.
    pub fn with_app_name(mut self, name: impl Into<String>) -> Self {
        self.app_name = name.into();
        self
    }

    /// Builder: set timeout.
    pub fn with_timeout(mut self, secs: u32) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Builder: enable or disable notifications.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Builder: set urgency override.
    pub fn with_urgency(mut self, urgency: impl Into<String>) -> Self {
        self.urgency = Some(urgency.into());
        self
    }

    /// Builder: set expiry override.
    pub fn with_expiry_secs(mut self, secs: u32) -> Self {
        self.expiry_secs = Some(secs);
        self
    }

    /// Builder: set icon path.
    pub fn with_icon_path(mut self, path: impl Into<String>) -> Self {
        self.icon_path = Some(path.into());
        self
    }

    /// Builder: set morning agenda time.
    pub fn with_morning_agenda_time(mut self, time: impl Into<String>) -> Self {
        self.morning_agenda_time = Some(time.into());
        self
    }

    /// Builder: configure end-warning notifications.
    pub fn with_end_warning_minutes(mut self, minutes_before: Option<u32>) -> Self {
        self.end_warning_minutes = minutes_before;
        self
    }
}

/// Notification state for tracking sent notifications and snooze.
#[derive(Debug)]
pub struct NotifyState {
    /// SHA-256 hashes of sent notifications, mapped to when they were sent,
    /// for deduplication with time-based eviction.
    sent_notifications: HashMap<String, DateTime<Utc>>,
    /// When notifications are snoozed until.
    snoozed_until: Option<DateTime<Utc>>,
}

impl Default for NotifyState {
    fn default() -> Self {
        Self::new()
    }
}

impl NotifyState {
    /// Creates a new notification state.
    pub fn new() -> Self {
        Self {
            sent_notifications: HashMap::new(),
            snoozed_until: None,
        }
    }

    /// Returns true if notifications are currently snoozed.
    pub fn is_snoozed(&self) -> bool {
        if let Some(until) = self.snoozed_until {
            Utc::now() < until
        } else {
            false
        }
    }

    /// Snoozes notifications for the given duration.
    pub fn snooze(&mut self, minutes: u32) {
        let until = Utc::now() + chrono::Duration::minutes(minutes as i64);
        self.snoozed_until = Some(until);
        info!(until = %until, minutes = minutes, "Notifications snoozed");
    }

    /// Clears the snooze.
    pub fn clear_snooze(&mut self) {
        self.snoozed_until = None;
    }

    /// Returns when notifications are snoozed until.
    pub fn snoozed_until(&self) -> Option<DateTime<Utc>> {
        self.snoozed_until
    }

    /// Checks if a notification has already been sent (by hash).
    pub fn was_sent(&self, hash: &str) -> bool {
        self.sent_notifications.contains_key(hash)
    }

    /// Marks a notification as sent.
    pub fn mark_sent(&mut self, hash: String) {
        self.sent_notifications.insert(hash, Utc::now());
    }

    /// Removes a notification hash so it can be retried.
    pub fn remove_sent(&mut self, hash: &str) {
        self.sent_notifications.remove(hash);
    }

    /// Given a set of currently-due notification thresholds, marks the
    /// not-yet-sent ones as sent and returns them.
    ///
    /// Used to consolidate multiple simultaneously-due thresholds (which can
    /// happen after a coarse or delayed check, since notification windows
    /// nest) into a single physical notification: callers send at most one
    /// notification per batch, using the actual remaining time, while every
    /// due threshold is still marked so none of them fire again later with a
    /// stale label.
    fn claim_due_thresholds(
        &mut self,
        due_thresholds: &[u32],
        hash_fn: impl Fn(u32) -> String,
    ) -> Vec<u32> {
        let mut newly_sent = Vec::new();
        for &notify_minutes in due_thresholds {
            let hash = hash_fn(notify_minutes);
            if !self.was_sent(&hash) {
                self.mark_sent(hash);
                newly_sent.push(notify_minutes);
            }
        }
        newly_sent
    }

    /// Evicts stale notification hashes and clears expired snoozes.
    ///
    /// Entries older than `retention` are removed; if the map still exceeds
    /// `max_size`, the oldest entries are evicted first. Recent hashes are
    /// always preserved so in-window notifications are never re-sent.
    pub fn cleanup_old_hashes_with_retention(
        &mut self,
        max_size: usize,
        retention: chrono::Duration,
    ) {
        let cutoff = Utc::now() - retention;
        let before = self.sent_notifications.len();
        self.sent_notifications
            .retain(|_, sent_at| *sent_at > cutoff);

        if self.sent_notifications.len() > max_size {
            // Secondary bound: evict the oldest entries beyond the cap.
            let mut entries: Vec<(String, DateTime<Utc>)> = self
                .sent_notifications
                .iter()
                .map(|(h, t)| (h.clone(), *t))
                .collect();
            entries.sort_by_key(|(_, t)| *t);
            let excess = entries.len() - max_size;
            for (hash, _) in entries.into_iter().take(excess) {
                self.sent_notifications.remove(&hash);
            }
        }

        let evicted = before.saturating_sub(self.sent_notifications.len());
        if evicted > 0 {
            debug!(
                evicted = evicted,
                remaining = self.sent_notifications.len(),
                "Evicted stale notification hashes"
            );
        }

        // Drop an expired snooze so state reflects reality.
        if let Some(until) = self.snoozed_until
            && Utc::now() >= until
        {
            self.snoozed_until = None;
        }
    }

    /// Evicts stale notification hashes using the default 24-hour retention.
    pub fn cleanup_old_hashes(&mut self, max_size: usize) {
        self.cleanup_old_hashes_with_retention(max_size, chrono::Duration::hours(24));
    }
}

/// Shared notification state.
pub type SharedNotifyState = Arc<RwLock<NotifyState>>;

/// Creates a new shared notification state.
pub fn new_notify_state() -> SharedNotifyState {
    Arc::new(RwLock::new(NotifyState::new()))
}

/// Generates a unique hash for a notification.
///
/// The hash is based on the meeting ID, start time, and notification offset.
pub fn notification_hash(meeting: &MeetingView, notify_minutes: u32) -> String {
    notification_hash_with_kind(meeting, notify_minutes, NotificationKind::StartsSoon)
}

#[derive(Debug, Clone, Copy)]
enum NotificationKind {
    StartsSoon,
    EndingSoon,
}

fn notification_hash_with_kind(
    meeting: &MeetingView,
    notify_minutes: u32,
    kind: NotificationKind,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(meeting.id.as_bytes());
    hasher.update(meeting.start_local.timestamp().to_le_bytes());
    hasher.update(notify_minutes.to_le_bytes());
    match kind {
        NotificationKind::StartsSoon => hasher.update(b"starts-soon"),
        NotificationKind::EndingSoon => hasher.update(b"ending-soon"),
    }
    let result = hasher.finalize();
    hex::encode(result)
}

/// Maximum number of concurrent OS threads for D-Bus notification calls.
const MAX_NOTIFICATION_THREADS: usize = 3;

/// The notification engine that sends desktop notifications.
pub struct NotifyEngine {
    config: NotifyConfig,
    state: SharedNotifyState,
    notification_semaphore: Arc<Semaphore>,
}

impl NotifyEngine {
    /// Creates a new notification engine.
    pub fn new(config: NotifyConfig) -> Self {
        Self {
            config,
            state: new_notify_state(),
            notification_semaphore: Arc::new(Semaphore::new(MAX_NOTIFICATION_THREADS)),
        }
    }

    /// Creates a new notification engine with shared state.
    pub fn with_state(config: NotifyConfig, state: SharedNotifyState) -> Self {
        Self {
            config,
            state,
            notification_semaphore: Arc::new(Semaphore::new(MAX_NOTIFICATION_THREADS)),
        }
    }

    /// Returns the shared state.
    pub fn state(&self) -> SharedNotifyState {
        self.state.clone()
    }

    /// Returns the effective notification thresholds (minutes-before-start)
    /// to use for a given meeting.
    ///
    /// If the meeting carries resolved reminder minutes (from the calendar
    /// provider's own reminder settings, e.g. Google's `reminders` field),
    /// those take priority over the global configured thresholds. Otherwise
    /// the global `notify_minutes` configuration is used as a fallback.
    fn effective_notify_minutes<'a>(&'a self, meeting: &'a MeetingView) -> &'a [u32] {
        match &meeting.reminder_minutes {
            Some(minutes) if !minutes.is_empty() => minutes,
            _ => &self.config.notify_minutes,
        }
    }

    /// Returns the configured start-soon thresholds whose notification
    /// window is currently open for a meeting.
    ///
    /// Windows nest — e.g. with thresholds `[15, 5, 1]` the 15-minute window
    /// stays open right through the 5- and 1-minute windows. If a check is
    /// delayed (coarse tick, server just started, or waking from sleep),
    /// several thresholds can be due at once; this is handled in
    /// [`check_and_notify`](Self::check_and_notify) by consolidating them
    /// into a single notification using the real remaining time.
    fn due_start_soon_thresholds(&self, meeting: &MeetingView, now: DateTime<Local>) -> Vec<u32> {
        self.effective_notify_minutes(meeting)
            .iter()
            .copied()
            .filter(|&notify_minutes| {
                let notify_time =
                    meeting.start_local - chrono::Duration::minutes(notify_minutes as i64);
                now >= notify_time && now < meeting.start_local
            })
            .collect()
    }

    /// Checks meetings and sends notifications for those starting soon.
    pub async fn check_and_notify(&self, meetings: &[MeetingView]) -> usize {
        if !self.config.enabled {
            return 0;
        }

        let state = self.state.read().await;
        if state.is_snoozed() {
            debug!("Notifications snoozed, skipping");
            return 0;
        }
        drop(state);

        let now = Local::now();
        let mut sent_count = 0;

        for meeting in meetings {
            if meeting.is_all_day {
                continue; // Skip all-day events
            }

            // Fire at most one notification per meeting per check, even if
            // several thresholds are simultaneously due, using the *actual*
            // remaining time for the label rather than the static configured
            // number — this keeps the text accurate even when a threshold
            // was noticed late, and avoids the notification daemon silently
            // dropping/replacing an earlier, differently-labelled popup for
            // the same meeting.
            let due_thresholds = self.due_start_soon_thresholds(meeting, now);

            if !due_thresholds.is_empty() {
                let newly_sent = {
                    let mut state = self.state.write().await;
                    state.claim_due_thresholds(&due_thresholds, |notify_minutes| {
                        notification_hash_with_kind(
                            meeting,
                            notify_minutes,
                            NotificationKind::StartsSoon,
                        )
                    })
                };

                if !newly_sent.is_empty() {
                    let minutes_left = meeting.minutes_until_start(now).max(0) as u32;
                    if self.send_notification(meeting, minutes_left).await {
                        sent_count += 1;
                    } else {
                        // Sending failed: allow every threshold in this batch
                        // to be retried on the next check.
                        let mut state = self.state.write().await;
                        for &notify_minutes in &newly_sent {
                            let hash = notification_hash_with_kind(
                                meeting,
                                notify_minutes,
                                NotificationKind::StartsSoon,
                            );
                            state.remove_sent(&hash);
                        }
                    }
                }
            }

            if self.should_send_end_warning(meeting, now)
                && let Some(minutes_before) = self.config.end_warning_minutes
            {
                let hash = notification_hash_with_kind(
                    meeting,
                    minutes_before,
                    NotificationKind::EndingSoon,
                );

                let minutes_left = meeting.minutes_until_end(now).max(0) as u32;

                let should_send = {
                    let mut state = self.state.write().await;
                    if state.was_sent(&hash) {
                        false
                    } else {
                        state.mark_sent(hash.clone());
                        true
                    }
                };
                if should_send {
                    if self
                        .send_end_warning_notification(meeting, minutes_left)
                        .await
                    {
                        sent_count += 1;
                    } else {
                        self.state.write().await.remove_sent(&hash);
                    }
                }
            }
        }

        sent_count
    }

    /// Sends a desktop notification for a meeting.
    async fn send_notification(&self, meeting: &MeetingView, minutes_before: u32) -> bool {
        let summary = if minutes_before == 0 {
            format!("Meeting starting now: {}", meeting.title)
        } else if minutes_before == 1 {
            format!("Meeting in 1 minute: {}", meeting.title)
        } else {
            format!("Meeting in {} minutes: {}", minutes_before, meeting.title)
        };

        let body = format!("Starts at {}", meeting.start_local.format("%H:%M"));

        #[cfg(target_os = "linux")]
        let urgency = if let Some(ref urg) = self.config.urgency {
            parse_urgency(urg)
        } else if minutes_before <= 1 {
            Urgency::Critical
        } else if minutes_before <= 5 {
            Urgency::Normal
        } else {
            Urgency::Low
        };

        let timeout_secs = self.config.expiry_secs.unwrap_or(self.config.timeout_secs);

        debug!(
            title = %meeting.title,
            minutes_before = minutes_before,
            "Sending notification"
        );

        let mut notification = Notification::new();
        notification
            .appname(&self.config.app_name)
            .summary(&summary)
            .body(&body)
            .timeout(Duration::from_secs(timeout_secs as u64));

        if let Some(ref icon) = self.config.icon_path {
            notification.icon(icon);
        }

        #[cfg(target_os = "linux")]
        notification.urgency(urgency);

        match show_notification(&notification, &self.notification_semaphore).await {
            Ok(_) => {
                info!(
                    title = %meeting.title,
                    minutes_before = minutes_before,
                    "Notification sent"
                );
                true
            }
            Err(e) => {
                error!(
                    error = %e,
                    title = %meeting.title,
                    "Failed to send notification"
                );
                false
            }
        }
    }

    /// Sends a desktop notification for an ongoing meeting that is about to end.
    async fn send_end_warning_notification(
        &self,
        meeting: &MeetingView,
        minutes_left: u32,
    ) -> bool {
        let summary = if minutes_left == 0 {
            format!("Meeting ending now: {}", meeting.title)
        } else if minutes_left == 1 {
            format!("Meeting ending in 1 minute: {}", meeting.title)
        } else {
            format!(
                "Meeting ending in {} minutes: {}",
                minutes_left, meeting.title
            )
        };

        let body = format!("Ends at {}", meeting.end_local.format("%H:%M"));

        #[cfg(target_os = "linux")]
        let urgency = if let Some(ref urg) = self.config.urgency {
            parse_urgency(urg)
        } else if minutes_left <= 1 {
            Urgency::Critical
        } else if minutes_left <= 5 {
            Urgency::Normal
        } else {
            Urgency::Low
        };

        let timeout_secs = self.config.expiry_secs.unwrap_or(self.config.timeout_secs);

        debug!(
            title = %meeting.title,
            minutes_left = minutes_left,
            "Sending end-warning notification"
        );

        let mut notification = Notification::new();
        notification
            .appname(&self.config.app_name)
            .summary(&summary)
            .body(&body)
            .timeout(Duration::from_secs(timeout_secs as u64));

        if let Some(ref icon) = self.config.icon_path {
            notification.icon(icon);
        }

        #[cfg(target_os = "linux")]
        notification.urgency(urgency);

        match show_notification(&notification, &self.notification_semaphore).await {
            Ok(_) => {
                info!(
                    title = %meeting.title,
                    minutes_left = minutes_left,
                    "End-warning notification sent"
                );
                true
            }
            Err(e) => {
                error!(
                    error = %e,
                    title = %meeting.title,
                    "Failed to send end-warning notification"
                );
                false
            }
        }
    }

    /// Returns true if the meeting is currently in the configured end-warning window.
    fn should_send_end_warning(&self, meeting: &MeetingView, now: DateTime<Local>) -> bool {
        let minutes_before = match self.config.end_warning_minutes {
            Some(minutes) if minutes > 0 => minutes as i64,
            _ => return false,
        };

        if !meeting.is_active_at(now) {
            return false;
        }

        let warning_start = meeting.end_local - chrono::Duration::minutes(minutes_before);
        now >= warning_start
    }

    /// Checks if it's time to send the morning agenda notification.
    /// Sends a summary of today's meetings once per day at the configured time.
    pub async fn check_morning_agenda(&self, meetings: &[MeetingView]) {
        // Skip if notifications are snoozed
        if self.state.read().await.is_snoozed() {
            debug!("Notifications snoozed, skipping morning agenda");
            return;
        }

        let agenda_time = match &self.config.morning_agenda_time {
            Some(t) => t,
            None => return,
        };

        let now = Local::now();
        let target = match chrono::NaiveTime::parse_from_str(agenda_time, "%H:%M") {
            Ok(t) => t,
            Err(_) => return,
        };

        // Only trigger within a 2-minute window of the agenda time
        let current_time = now.time();
        let diff = (current_time - target).num_minutes().abs();
        if diff > 1 {
            return;
        }

        // Deduplicate by date — atomic check-and-mark under a single write lock
        let date_hash = format!("morning-agenda-{}", now.format("%Y-%m-%d"));
        let should_send = {
            let mut state = self.state.write().await;
            if state.was_sent(&date_hash) {
                false
            } else {
                state.mark_sent(date_hash.clone());
                true
            }
        };
        if !should_send {
            return;
        }

        // Build agenda
        let today = now.date_naive();
        let today_meetings: Vec<_> = meetings
            .iter()
            .filter(|m| m.start_local.date_naive() == today && !m.is_all_day)
            .collect();

        if today_meetings.is_empty() {
            return;
        }

        let summary = format!("Today's Agenda: {} meetings", today_meetings.len());
        let body_lines: Vec<String> = today_meetings
            .iter()
            .map(|m| format!("{} - {}", m.start_local.format("%H:%M"), m.title))
            .collect();
        let body = body_lines.join("\n");

        let mut notification = Notification::new();
        notification
            .appname(&self.config.app_name)
            .summary(&summary)
            .body(&body)
            .timeout(Duration::from_secs(
                self.config.expiry_secs.unwrap_or(self.config.timeout_secs) as u64,
            ));

        if let Some(ref icon) = self.config.icon_path {
            notification.icon(icon);
        }

        match show_notification(&notification, &self.notification_semaphore).await {
            Ok(_) => {
                info!("Morning agenda notification sent");
            }
            Err(e) => {
                error!(error = %e, "Failed to send morning agenda notification");
                self.state.write().await.remove_sent(&date_hash);
            }
        }
    }

    /// Snoozes notifications for the given duration.
    pub async fn snooze(&self, minutes: u32) {
        self.state.write().await.snooze(minutes);
    }

    /// Clears the snooze.
    pub async fn clear_snooze(&self) {
        self.state.write().await.clear_snooze();
    }

    /// Returns true if notifications are currently snoozed.
    pub async fn is_snoozed(&self) -> bool {
        self.state.read().await.is_snoozed()
    }

    /// Returns when notifications are snoozed until.
    pub async fn snoozed_until(&self) -> Option<DateTime<Utc>> {
        self.state.read().await.snoozed_until()
    }

    /// Performs periodic cleanup of the notification state.
    pub async fn cleanup(&self) {
        self.state.write().await.cleanup_old_hashes(1000);
    }
}

/// Sends a notification via D-Bus.
///
/// Spawns a plain OS thread (not a Tokio blocking task) to run the synchronous
/// `show()` method, communicating the result back via a `oneshot` channel.
/// This avoids two problems:
/// 1. Panics when `zbus/tokio` and `zbus/async-io` features are both active due
///    to Cargo feature unification — `zbus::block_on` creates a nested tokio
///    runtime which panics inside an existing async context.
/// 2. Stuck D-Bus calls (e.g. unresponsive session bus) no longer consume Tokio
///    blocking pool threads — they run on plain OS threads that are abandoned on
///    timeout without affecting the Tokio runtime.
async fn show_notification(
    notification: &Notification,
    semaphore: &Arc<Semaphore>,
) -> Result<(), String> {
    let permit = semaphore
        .clone()
        .try_acquire_owned()
        .map_err(|_| "too many in-flight notification threads".to_string())?;
    let notification = notification.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let result = notification.show().map(|_| ());
        let _ = tx.send(result);
        drop(permit);
    });
    match tokio::time::timeout(Duration::from_secs(10), rx).await {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(e))) => Err(e.to_string()),
        Ok(Err(_)) => Err("notification thread panicked".to_string()),
        Err(_) => Err("notification D-Bus call timed out after 10s".to_string()),
    }
}

/// Parses an urgency string into a notify-rust Urgency value.
#[cfg(target_os = "linux")]
fn parse_urgency(s: &str) -> Urgency {
    match s.to_lowercase().as_str() {
        "low" => Urgency::Low,
        "normal" => Urgency::Normal,
        "critical" => Urgency::Critical,
        _ => Urgency::Normal,
    }
}

// Simple hex encoding (avoid adding another dependency)
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn make_meeting(id: &str, title: &str, minutes_from_now: i64) -> MeetingView {
        make_meeting_with_reminders(id, title, minutes_from_now, None)
    }

    fn make_meeting_with_reminders(
        id: &str,
        title: &str,
        minutes_from_now: i64,
        reminder_minutes: Option<Vec<u32>>,
    ) -> MeetingView {
        let start = Local::now() + chrono::Duration::minutes(minutes_from_now);
        MeetingView {
            id: id.to_string(),
            provider_name: "unknown".to_string(),
            title: title.to_string(),
            start_local: start,
            end_local: start + chrono::Duration::hours(1),
            is_all_day: false,
            is_ongoing: false,
            primary_link: None,
            secondary_links: vec![],
            calendar_url: None,
            calendar_id: "primary".to_string(),
            user_response_status: nextmeeting_core::ResponseStatus::Unknown,
            other_attendee_count: 0,
            location: None,
            description: None,
            attendees: vec![],
            reminder_minutes,
        }
    }

    #[test]
    fn config_default() {
        let config = NotifyConfig::default();
        assert!(!config.notify_minutes.is_empty());
        assert!(config.enabled);
        assert_eq!(config.end_warning_minutes, None);
    }

    #[test]
    fn state_snooze() {
        let mut state = NotifyState::new();
        assert!(!state.is_snoozed());

        state.snooze(30);
        assert!(state.is_snoozed());

        state.clear_snooze();
        assert!(!state.is_snoozed());
    }

    #[test]
    fn state_sent_tracking() {
        let mut state = NotifyState::new();
        let hash = "abc123".to_string();

        assert!(!state.was_sent(&hash));
        state.mark_sent(hash.clone());
        assert!(state.was_sent(&hash));
    }

    #[test]
    fn cleanup_evicts_only_stale_hashes() {
        let mut state = NotifyState::new();
        state.mark_sent("fresh".to_string());
        // Backdate one entry past the retention window.
        state.sent_notifications.insert(
            "stale".to_string(),
            Utc::now() - chrono::Duration::hours(48),
        );

        state.cleanup_old_hashes_with_retention(1000, chrono::Duration::hours(24));

        assert!(state.was_sent("fresh"), "recent hashes must be preserved");
        assert!(!state.was_sent("stale"), "stale hashes must be evicted");
    }

    #[test]
    fn cleanup_size_cap_evicts_oldest_first() {
        let mut state = NotifyState::new();
        for i in 0..10 {
            state.sent_notifications.insert(
                format!("hash-{}", i),
                Utc::now() - chrono::Duration::minutes(10 - i),
            );
        }

        state.cleanup_old_hashes_with_retention(5, chrono::Duration::hours(24));

        assert_eq!(state.sent_notifications.len(), 5);
        // The newest entries survive.
        for i in 5..10 {
            assert!(state.was_sent(&format!("hash-{}", i)));
        }
        for i in 0..5 {
            assert!(!state.was_sent(&format!("hash-{}", i)));
        }
    }

    #[test]
    fn cleanup_clears_expired_snooze() {
        let mut state = NotifyState::new();
        state.snoozed_until = Some(Utc::now() - chrono::Duration::minutes(1));

        state.cleanup_old_hashes(1000);

        assert!(state.snoozed_until().is_none());

        // An active snooze must be left alone.
        state.snooze(30);
        state.cleanup_old_hashes(1000);
        assert!(state.is_snoozed());
    }

    #[test]
    fn claim_due_thresholds_consolidates_a_batch_and_tracks_state() {
        let mut state = NotifyState::new();

        // A delayed check finds both the 15- and 5-minute windows open at
        // once; both must be claimed (marked sent) in a single call.
        let newly = state.claim_due_thresholds(&[15, 5], |m| format!("hash-{m}"));
        assert_eq!(newly, vec![15, 5]);
        assert!(state.was_sent("hash-15"));
        assert!(state.was_sent("hash-5"));

        // Re-checking the same batch claims nothing new.
        let newly_again = state.claim_due_thresholds(&[15, 5], |m| format!("hash-{m}"));
        assert!(newly_again.is_empty());

        // Later, the 1-minute threshold becomes due too: only it is newly
        // claimed, since 15 and 5 were already marked sent.
        let newly_later = state.claim_due_thresholds(&[15, 5, 1], |m| format!("hash-{m}"));
        assert_eq!(newly_later, vec![1]);
    }

    #[test]
    fn due_start_soon_thresholds_nest_when_check_is_delayed() {
        // With thresholds [15, 5, 1], a meeting starting in 4 minutes has
        // both the 15- and 5-minute windows open simultaneously (they
        // nest), but not the 1-minute window yet. This is exactly the
        // scenario that used to cause a stale/wrong label (e.g. "in 5
        // minutes" shown when only 4 actually remain, or a batch of
        // differently-labelled notifications firing at once).
        let config = NotifyConfig::new(vec![15, 5, 1]).with_enabled(true);
        let engine = NotifyEngine::new(config);

        let meeting = make_meeting("1", "Test", 4);
        let due = engine.due_start_soon_thresholds(&meeting, Local::now());
        assert_eq!(due, vec![15, 5]);
    }

    #[test]
    fn due_start_soon_thresholds_empty_when_meeting_is_far_away() {
        let config = NotifyConfig::new(vec![15, 5, 1]).with_enabled(true);
        let engine = NotifyEngine::new(config);

        let meeting = make_meeting("1", "Test", 30);
        let due = engine.due_start_soon_thresholds(&meeting, Local::now());
        assert!(due.is_empty());
    }

    #[test]
    fn due_start_soon_thresholds_excludes_meetings_that_already_started() {
        let config = NotifyConfig::new(vec![15, 5, 1]).with_enabled(true);
        let engine = NotifyEngine::new(config);

        let meeting = make_meeting("1", "Test", -1); // started a minute ago
        let due = engine.due_start_soon_thresholds(&meeting, Local::now());
        assert!(due.is_empty());
    }

    #[test]
    fn due_start_soon_thresholds_uses_per_event_reminder_minutes_over_config() {
        // The global config would fire at 15/5/1, but this event carries its
        // own popup reminder minutes (e.g. resolved from Google Calendar's
        // `reminders` field), which must take priority.
        let config = NotifyConfig::new(vec![15, 5, 1]).with_enabled(true);
        let engine = NotifyEngine::new(config);

        let meeting = make_meeting_with_reminders("1", "Test", 10, Some(vec![10]));
        let due = engine.due_start_soon_thresholds(&meeting, Local::now());
        assert_eq!(due, vec![10]);
    }

    #[test]
    fn due_start_soon_thresholds_falls_back_to_config_when_reminders_disabled() {
        // `None` represents reminders that were explicitly disabled (or
        // otherwise unresolved) for the event; the global config is used.
        let config = NotifyConfig::new(vec![15, 5, 1]).with_enabled(true);
        let engine = NotifyEngine::new(config);

        let meeting = make_meeting_with_reminders("1", "Test", 12, None);
        let due = engine.due_start_soon_thresholds(&meeting, Local::now());
        assert_eq!(due, vec![15]);
    }

    #[test]
    fn due_start_soon_thresholds_falls_back_to_config_when_reminders_empty() {
        // An empty (but present) reminder list also falls back to the
        // global config, rather than silently notifying never.
        let config = NotifyConfig::new(vec![15, 5, 1]).with_enabled(true);
        let engine = NotifyEngine::new(config);

        let meeting = make_meeting_with_reminders("1", "Test", 12, Some(vec![]));
        let due = engine.due_start_soon_thresholds(&meeting, Local::now());
        assert_eq!(due, vec![15]);
    }

    #[test]
    fn notification_hash_unique() {
        let meeting1 = make_meeting("1", "Meeting 1", 10);
        let meeting2 = make_meeting("2", "Meeting 2", 10);

        let hash1 = notification_hash(&meeting1, 5);
        let hash2 = notification_hash(&meeting2, 5);
        let hash3 = notification_hash(&meeting1, 10);

        assert_ne!(hash1, hash2); // Different meetings
        assert_ne!(hash1, hash3); // Different notification times
    }

    #[test]
    fn notification_hash_consistent() {
        let meeting = make_meeting("1", "Meeting 1", 10);

        let hash1 = notification_hash(&meeting, 5);
        let hash2 = notification_hash(&meeting, 5);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn end_warning_hash_differs_from_start_hash() {
        let meeting = make_meeting("1", "Meeting 1", 10);
        let start_hash = notification_hash_with_kind(&meeting, 5, NotificationKind::StartsSoon);
        let end_hash = notification_hash_with_kind(&meeting, 5, NotificationKind::EndingSoon);
        assert_ne!(start_hash, end_hash);
    }

    #[tokio::test]
    async fn engine_snooze() {
        let config = NotifyConfig::default().with_enabled(true);
        let engine = NotifyEngine::new(config);

        assert!(!engine.is_snoozed().await);

        engine.snooze(30).await;
        assert!(engine.is_snoozed().await);

        engine.clear_snooze().await;
        assert!(!engine.is_snoozed().await);
    }

    #[tokio::test]
    async fn engine_skips_when_snoozed() {
        let config = NotifyConfig::default().with_enabled(true);
        let engine = NotifyEngine::new(config);

        engine.snooze(30).await;

        let meetings = vec![make_meeting("1", "Test", 5)];
        let sent = engine.check_and_notify(&meetings).await;

        assert_eq!(sent, 0);
    }

    #[tokio::test]
    async fn engine_skips_when_disabled() {
        let config = NotifyConfig::default().with_enabled(false);
        let engine = NotifyEngine::new(config);

        let meetings = vec![make_meeting("1", "Test", 5)];
        let sent = engine.check_and_notify(&meetings).await;

        assert_eq!(sent, 0);
    }

    #[tokio::test]
    async fn engine_skips_all_day_events() {
        let config = NotifyConfig::new(vec![5]).with_enabled(true);
        let engine = NotifyEngine::new(config);

        let mut meeting = make_meeting("1", "Test", 3);
        meeting.is_all_day = true;

        let sent = engine.check_and_notify(&[meeting]).await;
        assert_eq!(sent, 0);
    }

    #[tokio::test]
    async fn notification_delivery_path_does_not_panic_in_tokio_runtime() {
        let mut notification = Notification::new();
        notification
            .appname("nextmeeting-test")
            .summary("runtime-check")
            .body("runtime-check")
            .timeout(Duration::from_secs(1));

        let semaphore = Arc::new(Semaphore::new(3));
        let _ = show_notification(&notification, &semaphore).await;
    }

    #[test]
    fn hex_encode() {
        assert_eq!(hex::encode([0x00, 0xff, 0xab]), "00ffab");
        assert_eq!(hex::encode([]), "");
    }

    #[test]
    fn config_with_urgency() {
        let config = NotifyConfig::default().with_urgency("critical");
        assert_eq!(config.urgency, Some("critical".to_string()));
    }

    #[test]
    fn config_with_expiry() {
        let config = NotifyConfig::default().with_expiry_secs(30);
        assert_eq!(config.expiry_secs, Some(30));
    }

    #[test]
    fn config_with_icon_path() {
        let config = NotifyConfig::default().with_icon_path("/usr/share/icons/meeting.png");
        assert_eq!(
            config.icon_path,
            Some("/usr/share/icons/meeting.png".to_string())
        );
    }

    #[test]
    fn config_with_morning_agenda() {
        let config = NotifyConfig::default().with_morning_agenda_time("08:00");
        assert_eq!(config.morning_agenda_time, Some("08:00".to_string()));
    }

    #[test]
    fn config_with_end_warning_minutes() {
        let config = NotifyConfig::default().with_end_warning_minutes(Some(4));
        assert_eq!(config.end_warning_minutes, Some(4));
    }

    #[test]
    fn should_send_end_warning_false_when_disabled() {
        let meeting = make_meeting("1", "Test", -1);
        let now = Local::now();
        let engine = NotifyEngine::new(NotifyConfig::default());
        assert!(!engine.should_send_end_warning(&meeting, now));
    }

    #[test]
    fn should_send_end_warning_false_without_threshold() {
        let meeting = make_meeting("1", "Test", -1);
        let now = Local::now();
        let config = NotifyConfig::default().with_end_warning_minutes(None);
        let engine = NotifyEngine::new(config);
        assert!(!engine.should_send_end_warning(&meeting, now));
    }

    #[test]
    fn should_send_end_warning_true_when_within_window() {
        let meeting = make_meeting("1", "Test", -57); // 3 minutes left (meeting lasts 1 hour)
        let now = Local::now();
        let config = NotifyConfig::default().with_end_warning_minutes(Some(5));
        let engine = NotifyEngine::new(config);
        assert!(engine.should_send_end_warning(&meeting, now));
    }

    #[test]
    fn should_send_end_warning_false_outside_window() {
        let meeting = make_meeting("1", "Test", -10); // 50 minutes left
        let now = Local::now();
        let config = NotifyConfig::default().with_end_warning_minutes(Some(5));
        let engine = NotifyEngine::new(config);
        assert!(!engine.should_send_end_warning(&meeting, now));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_urgency_values() {
        assert_eq!(super::parse_urgency("low"), Urgency::Low);
        assert_eq!(super::parse_urgency("normal"), Urgency::Normal);
        assert_eq!(super::parse_urgency("critical"), Urgency::Critical);
        assert_eq!(super::parse_urgency("Critical"), Urgency::Critical);
        assert_eq!(super::parse_urgency("unknown"), Urgency::Normal);
    }

    #[tokio::test]
    async fn morning_agenda_dedup() {
        let config = NotifyConfig::default()
            .with_enabled(true)
            .with_morning_agenda_time("08:00");
        let engine = NotifyEngine::new(config);

        // Manually mark today's agenda as sent
        let date_hash = format!("morning-agenda-{}", Local::now().format("%Y-%m-%d"));
        engine.state.write().await.mark_sent(date_hash.clone());

        // Should not send again
        let meetings = vec![make_meeting("1", "Test", 60)];
        engine.check_morning_agenda(&meetings).await;

        // Verify it was already marked
        assert!(engine.state.read().await.was_sent(&date_hash));
    }
}
