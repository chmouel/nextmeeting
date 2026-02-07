//! Event cache with TTL (Time-To-Live) support.
//!
//! This module provides a cache for storing calendar events with automatic
//! expiration based on TTL.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use nextmeeting_core::MeetingView;
use tracing::{debug, trace};

/// Cache entry containing events and metadata.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Cached meetings.
    pub meetings: Vec<MeetingView>,
    /// When the entry was last updated.
    pub updated_at: DateTime<Utc>,
    /// When the entry expires (monotonic clock).
    expires_at: Instant,
    /// ETag for conditional fetching (if available).
    pub etag: Option<String>,
}

impl CacheEntry {
    /// Creates a new cache entry with the given TTL.
    pub fn new(meetings: Vec<MeetingView>, ttl: Duration) -> Self {
        Self {
            meetings,
            updated_at: Utc::now(),
            expires_at: Instant::now() + ttl,
            etag: None,
        }
    }

    /// Creates a new cache entry with ETag.
    pub fn with_etag(meetings: Vec<MeetingView>, ttl: Duration, etag: impl Into<String>) -> Self {
        Self {
            meetings,
            updated_at: Utc::now(),
            expires_at: Instant::now() + ttl,
            etag: Some(etag.into()),
        }
    }

    /// Returns true if the entry has expired.
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    /// Returns the time until expiration.
    pub fn time_until_expiry(&self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }

    /// Updates the entry with new meetings and resets the TTL.
    pub fn update(&mut self, meetings: Vec<MeetingView>, ttl: Duration) {
        self.meetings = meetings;
        self.updated_at = Utc::now();
        self.expires_at = Instant::now() + ttl;
    }

    /// Updates the entry with new meetings, ETag, and resets the TTL.
    pub fn update_with_etag(
        &mut self,
        meetings: Vec<MeetingView>,
        ttl: Duration,
        etag: impl Into<String>,
    ) {
        self.update(meetings, ttl);
        self.etag = Some(etag.into());
    }

    /// Extends the TTL without changing the data.
    pub fn extend_ttl(&mut self, ttl: Duration) {
        self.expires_at = Instant::now() + ttl;
    }
}

/// Event cache with TTL support.
///
/// The cache stores events per provider/calendar, allowing for independent
/// TTL management and conditional fetching using ETags.
#[derive(Debug)]
pub struct EventCache {
    /// Default TTL for new entries.
    default_ttl: Duration,
    /// Cache entries keyed by provider/calendar ID.
    entries: HashMap<String, CacheEntry>,
}

impl Default for EventCache {
    fn default() -> Self {
        Self::new(Duration::from_secs(300)) // 5 minutes default
    }
}

impl EventCache {
    /// Creates a new cache with the given default TTL.
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            default_ttl,
            entries: HashMap::new(),
        }
    }

    /// Returns the default TTL.
    pub fn default_ttl(&self) -> Duration {
        self.default_ttl
    }

    /// Sets the default TTL.
    pub fn set_default_ttl(&mut self, ttl: Duration) {
        self.default_ttl = ttl;
    }

    /// Gets a cache entry by key.
    pub fn get(&self, key: &str) -> Option<&CacheEntry> {
        self.entries.get(key)
    }

    /// Gets a cache entry by key, only if not expired.
    pub fn get_valid(&self, key: &str) -> Option<&CacheEntry> {
        self.entries.get(key).filter(|entry| !entry.is_expired())
    }

    /// Checks if a key exists and is not expired.
    pub fn is_valid(&self, key: &str) -> bool {
        self.get_valid(key).is_some()
    }

    /// Inserts or updates a cache entry.
    pub fn insert(&mut self, key: impl Into<String>, meetings: Vec<MeetingView>) {
        let key = key.into();
        let ttl = self.default_ttl;

        if let Some(entry) = self.entries.get_mut(&key) {
            entry.update(meetings, ttl);
            debug!(key = %key, "Updated cache entry");
        } else {
            self.entries
                .insert(key.clone(), CacheEntry::new(meetings, ttl));
            debug!(key = %key, "Inserted new cache entry");
        }
    }

    /// Inserts or updates a cache entry with ETag.
    pub fn insert_with_etag(
        &mut self,
        key: impl Into<String>,
        meetings: Vec<MeetingView>,
        etag: impl Into<String>,
    ) {
        let key = key.into();
        let ttl = self.default_ttl;
        let etag = etag.into();

        if let Some(entry) = self.entries.get_mut(&key) {
            entry.update_with_etag(meetings, ttl, etag);
            debug!(key = %key, "Updated cache entry with ETag");
        } else {
            self.entries
                .insert(key.clone(), CacheEntry::with_etag(meetings, ttl, etag));
            debug!(key = %key, "Inserted new cache entry with ETag");
        }
    }

    /// Inserts or updates a cache entry with a custom TTL.
    pub fn insert_with_ttl(
        &mut self,
        key: impl Into<String>,
        meetings: Vec<MeetingView>,
        ttl: Duration,
    ) {
        let key = key.into();

        if let Some(entry) = self.entries.get_mut(&key) {
            entry.update(meetings, ttl);
        } else {
            self.entries
                .insert(key.clone(), CacheEntry::new(meetings, ttl));
        }
        debug!(key = %key, ttl_secs = ttl.as_secs(), "Inserted cache entry with custom TTL");
    }

    /// Removes a cache entry.
    pub fn remove(&mut self, key: &str) -> Option<CacheEntry> {
        let entry = self.entries.remove(key);
        if entry.is_some() {
            debug!(key = %key, "Removed cache entry");
        }
        entry
    }

    /// Clears all cache entries.
    pub fn clear(&mut self) {
        let count = self.entries.len();
        self.entries.clear();
        debug!(count = count, "Cleared all cache entries");
    }

    /// Removes all expired entries.
    pub fn evict_expired(&mut self) -> usize {
        let before = self.entries.len();
        self.entries.retain(|key, entry| {
            let keep = !entry.is_expired();
            if !keep {
                trace!(key = %key, "Evicting expired cache entry");
            }
            keep
        });
        let evicted = before - self.entries.len();
        if evicted > 0 {
            debug!(evicted = evicted, "Evicted expired cache entries");
        }
        evicted
    }

    /// Returns the number of cache entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns all valid (non-expired) meetings merged from all entries.
    pub fn all_meetings(&self) -> Vec<MeetingView> {
        let mut meetings: Vec<_> = self
            .entries
            .values()
            .filter(|entry| !entry.is_expired())
            .flat_map(|entry| entry.meetings.iter().cloned())
            .collect();

        // Sort by start time
        meetings.sort_by(|a, b| a.start_local.cmp(&b.start_local));
        meetings
    }

    /// Returns the ETag for a given key (if available and not expired).
    pub fn get_etag(&self, key: &str) -> Option<&str> {
        self.get_valid(key).and_then(|entry| entry.etag.as_deref())
    }

    /// Extends the TTL for a given key.
    pub fn extend_ttl(&mut self, key: &str, ttl: Duration) {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.extend_ttl(ttl);
            debug!(key = %key, ttl_secs = ttl.as_secs(), "Extended cache TTL");
        }
    }

    /// Returns the time until the next entry expires.
    pub fn next_expiry(&self) -> Option<Duration> {
        self.entries
            .values()
            .filter(|entry| !entry.is_expired())
            .map(|entry| entry.time_until_expiry())
            .min()
    }

    /// Returns an iterator over all keys.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use std::thread;

    fn make_meeting(id: &str, title: &str) -> MeetingView {
        let now = Local::now();
        MeetingView {
            id: id.to_string(),
            title: title.to_string(),
            start_local: now,
            end_local: now + chrono::Duration::hours(1),
            is_all_day: false,
            is_ongoing: false,
            primary_link: None,
            secondary_links: vec![],
            calendar_url: None,
            calendar_id: "primary".to_string(),
            user_response_status: nextmeeting_core::ResponseStatus::Unknown,
            other_attendee_count: 0,
        }
    }

    #[test]
    fn cache_entry_expiration() {
        let meetings = vec![make_meeting("1", "Test")];
        let entry = CacheEntry::new(meetings, Duration::from_millis(50));

        assert!(!entry.is_expired());
        thread::sleep(Duration::from_millis(60));
        assert!(entry.is_expired());
    }

    #[test]
    fn cache_insert_and_get() {
        let mut cache = EventCache::new(Duration::from_secs(60));

        let meetings = vec![make_meeting("1", "Meeting 1")];
        cache.insert("provider1", meetings);

        assert!(cache.get("provider1").is_some());
        assert!(cache.get_valid("provider1").is_some());
        assert!(cache.is_valid("provider1"));

        assert!(cache.get("nonexistent").is_none());
        assert!(!cache.is_valid("nonexistent"));
    }

    #[test]
    fn cache_expiration() {
        let mut cache = EventCache::new(Duration::from_millis(50));

        let meetings = vec![make_meeting("1", "Meeting 1")];
        cache.insert("provider1", meetings);

        assert!(cache.is_valid("provider1"));
        thread::sleep(Duration::from_millis(60));
        assert!(!cache.is_valid("provider1"));
    }

    #[test]
    fn cache_evict_expired() {
        let mut cache = EventCache::new(Duration::from_millis(50));

        cache.insert("provider1", vec![make_meeting("1", "Meeting 1")]);
        cache.insert_with_ttl(
            "provider2",
            vec![make_meeting("2", "Meeting 2")],
            Duration::from_secs(60),
        );

        thread::sleep(Duration::from_millis(60));

        let evicted = cache.evict_expired();
        assert_eq!(evicted, 1);
        assert_eq!(cache.len(), 1);
        assert!(cache.is_valid("provider2"));
    }

    #[test]
    fn cache_etag() {
        let mut cache = EventCache::new(Duration::from_secs(60));

        cache.insert_with_etag("provider1", vec![make_meeting("1", "Meeting 1")], "etag123");

        assert_eq!(cache.get_etag("provider1"), Some("etag123"));
        assert_eq!(cache.get_etag("nonexistent"), None);
    }

    #[test]
    fn cache_all_meetings() {
        let mut cache = EventCache::new(Duration::from_secs(60));

        cache.insert("provider1", vec![make_meeting("1", "Meeting 1")]);
        cache.insert("provider2", vec![make_meeting("2", "Meeting 2")]);

        let all = cache.all_meetings();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn cache_clear() {
        let mut cache = EventCache::new(Duration::from_secs(60));

        cache.insert("provider1", vec![make_meeting("1", "Meeting 1")]);
        cache.insert("provider2", vec![make_meeting("2", "Meeting 2")]);

        assert_eq!(cache.len(), 2);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_remove() {
        let mut cache = EventCache::new(Duration::from_secs(60));

        cache.insert("provider1", vec![make_meeting("1", "Meeting 1")]);
        assert!(cache.is_valid("provider1"));

        let removed = cache.remove("provider1");
        assert!(removed.is_some());
        assert!(!cache.is_valid("provider1"));
    }

    #[test]
    fn cache_extend_ttl() {
        let mut cache = EventCache::new(Duration::from_millis(50));

        cache.insert("provider1", vec![make_meeting("1", "Meeting 1")]);

        // Extend TTL before expiration
        thread::sleep(Duration::from_millis(30));
        cache.extend_ttl("provider1", Duration::from_secs(60));

        // Should still be valid after original TTL would have expired
        thread::sleep(Duration::from_millis(30));
        assert!(cache.is_valid("provider1"));
    }

    #[test]
    fn cache_next_expiry() {
        let mut cache = EventCache::new(Duration::from_secs(60));

        assert!(cache.next_expiry().is_none());

        cache.insert("provider1", vec![make_meeting("1", "Meeting 1")]);

        let next = cache.next_expiry();
        assert!(next.is_some());
        assert!(next.unwrap() <= Duration::from_secs(60));
    }
}
