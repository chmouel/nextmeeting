//! Desktop notification engine for upcoming meetings.
//!
//! This module provides the notification system that alerts users about
//! upcoming meetings. It supports:
//! - Configurable notification timing (e.g., 5, 10, 15 minutes before)
//! - Snooze functionality
//! - Deduplication to avoid repeated notifications

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local, Utc};
use notify_rust::Notification;
#[cfg(target_os = "linux")]
use notify_rust::Urgency;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
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
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            notify_minutes: vec![15, 5, 1],
            app_name: "nextmeeting".to_string(),
            timeout_secs: 10,
            enabled: true,
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
}

/// Notification state for tracking sent notifications and snooze.
#[derive(Debug)]
pub struct NotifyState {
    /// SHA-256 hashes of sent notifications for deduplication.
    sent_notifications: HashSet<String>,
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
            sent_notifications: HashSet::new(),
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
        self.sent_notifications.contains(hash)
    }

    /// Marks a notification as sent.
    pub fn mark_sent(&mut self, hash: String) {
        self.sent_notifications.insert(hash);
    }

    /// Clears old notification hashes to prevent unbounded growth.
    /// Called periodically to remove hashes older than the retention period.
    pub fn cleanup_old_hashes(&mut self, max_size: usize) {
        if self.sent_notifications.len() > max_size {
            // Simple strategy: clear all and let them be re-added
            // In a production system, we'd use an LRU cache or time-based eviction
            debug!(
                size = self.sent_notifications.len(),
                "Clearing notification hash cache"
            );
            self.sent_notifications.clear();
        }
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
    let mut hasher = Sha256::new();
    hasher.update(meeting.id.as_bytes());
    hasher.update(meeting.start_local.timestamp().to_le_bytes());
    hasher.update(notify_minutes.to_le_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// The notification engine that sends desktop notifications.
pub struct NotifyEngine {
    config: NotifyConfig,
    state: SharedNotifyState,
}

impl NotifyEngine {
    /// Creates a new notification engine.
    pub fn new(config: NotifyConfig) -> Self {
        Self {
            config,
            state: new_notify_state(),
        }
    }

    /// Creates a new notification engine with shared state.
    pub fn with_state(config: NotifyConfig, state: SharedNotifyState) -> Self {
        Self { config, state }
    }

    /// Returns the shared state.
    pub fn state(&self) -> SharedNotifyState {
        self.state.clone()
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

            for &notify_minutes in &self.config.notify_minutes {
                let notify_time =
                    meeting.start_local - chrono::Duration::minutes(notify_minutes as i64);

                // Check if we're within the notification window
                // (notify_time <= now < meeting.start_local)
                if now >= notify_time && now < meeting.start_local {
                    let hash = notification_hash(meeting, notify_minutes);

                    let mut state = self.state.write().await;
                    if !state.was_sent(&hash)
                        && self.send_notification(meeting, notify_minutes).await
                    {
                        state.mark_sent(hash);
                        sent_count += 1;
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
        let urgency = if minutes_before <= 1 {
            Urgency::Critical
        } else if minutes_before <= 5 {
            Urgency::Normal
        } else {
            Urgency::Low
        };

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
            .timeout(Duration::from_secs(self.config.timeout_secs as u64));

        #[cfg(target_os = "linux")]
        notification.urgency(urgency);

        match notification.show() {
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
        let start = Local::now() + chrono::Duration::minutes(minutes_from_now);
        MeetingView {
            id: id.to_string(),
            title: title.to_string(),
            start_local: start,
            end_local: start + chrono::Duration::hours(1),
            is_all_day: false,
            is_ongoing: false,
            primary_link: None,
            secondary_links: vec![],
            calendar_url: None,
            user_response_status: nextmeeting_core::ResponseStatus::Unknown,
            other_attendee_count: 0,
        }
    }

    #[test]
    fn config_default() {
        let config = NotifyConfig::default();
        assert!(!config.notify_minutes.is_empty());
        assert!(config.enabled);
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

    #[test]
    fn hex_encode() {
        assert_eq!(hex::encode([0x00, 0xff, 0xab]), "00ffab");
        assert_eq!(hex::encode([]), "");
    }
}
