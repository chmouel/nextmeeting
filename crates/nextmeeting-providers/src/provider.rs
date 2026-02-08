//! CalendarProvider trait definition.
//!
//! This module defines the [`CalendarProvider`] trait, which is the core
//! abstraction for calendar backends (Google Calendar, CalDAV, etc.).
//!
//! Providers are responsible for:
//! - Fetching events from calendar servers
//! - Handling authentication and authorization
//! - Managing caching hints (ETags, sync tokens)

use std::future::Future;
use std::pin::Pin;

use nextmeeting_core::TimeWindow;

use crate::error::{ProviderError, ProviderResult};
use crate::raw_event::RawEvent;

/// Information about a calendar.
#[derive(Debug, Clone)]
pub struct CalendarInfo {
    /// Unique identifier for the calendar.
    pub id: String,
    /// Human-readable name of the calendar.
    pub name: String,
    /// Description of the calendar, if available.
    pub description: Option<String>,
    /// Whether this is the primary calendar.
    pub is_primary: bool,
    /// The timezone of the calendar (IANA identifier).
    pub timezone: Option<String>,
    /// Background color for UI display.
    pub background_color: Option<String>,
    /// Foreground color for UI display.
    pub foreground_color: Option<String>,
}

impl CalendarInfo {
    /// Creates a new CalendarInfo with the given ID and name.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            is_primary: false,
            timezone: None,
            background_color: None,
            foreground_color: None,
        }
    }

    /// Builder method to mark as primary.
    pub fn with_primary(mut self, is_primary: bool) -> Self {
        self.is_primary = is_primary;
        self
    }

    /// Builder method to set timezone.
    pub fn with_timezone(mut self, timezone: impl Into<String>) -> Self {
        self.timezone = Some(timezone.into());
        self
    }
}

/// Result from fetching events, including caching metadata.
#[derive(Debug)]
pub struct FetchResult {
    /// The fetched events.
    pub events: Vec<RawEvent>,
    /// The sync token for incremental fetches (provider-specific).
    pub sync_token: Option<String>,
    /// Whether the data was unchanged from the cached version.
    pub not_modified: bool,
}

/// Action to mutate an existing provider event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventMutationAction {
    /// Decline an invited event.
    Decline,
    /// Delete the event.
    Delete,
}

impl FetchResult {
    /// Creates a new fetch result with events.
    pub fn with_events(events: Vec<RawEvent>) -> Self {
        Self {
            events,
            sync_token: None,
            not_modified: false,
        }
    }

    /// Creates a not-modified response (for conditional fetches).
    pub fn not_modified() -> Self {
        Self {
            events: Vec::new(),
            sync_token: None,
            not_modified: true,
        }
    }

    /// Builder method to set sync token.
    pub fn with_sync_token(mut self, token: impl Into<String>) -> Self {
        self.sync_token = Some(token.into());
        self
    }
}

/// Options for fetching events.
#[derive(Debug, Clone, Default)]
pub struct FetchOptions {
    /// Time window to fetch events for.
    pub time_window: Option<TimeWindow>,
    /// Maximum number of events to return.
    pub max_results: Option<usize>,
    /// ETag or sync token for conditional fetch.
    pub if_none_match: Option<String>,
    /// Whether to expand recurring events into instances.
    pub expand_recurring: bool,
    /// Only fetch events from specific calendars.
    pub calendar_ids: Option<Vec<String>>,
}

impl FetchOptions {
    /// Creates new fetch options with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder method to set time window.
    pub fn with_time_window(mut self, window: TimeWindow) -> Self {
        self.time_window = Some(window);
        self
    }

    /// Builder method to set max results.
    pub fn with_max_results(mut self, max: usize) -> Self {
        self.max_results = Some(max);
        self
    }

    /// Builder method to set conditional fetch token.
    pub fn with_if_none_match(mut self, etag: impl Into<String>) -> Self {
        self.if_none_match = Some(etag.into());
        self
    }

    /// Builder method to enable recurring event expansion.
    pub fn with_expand_recurring(mut self, expand: bool) -> Self {
        self.expand_recurring = expand;
        self
    }

    /// Builder method to filter by calendar IDs.
    pub fn with_calendar_ids(mut self, ids: Vec<String>) -> Self {
        self.calendar_ids = Some(ids);
        self
    }
}

/// Status information about a provider.
#[derive(Debug, Clone)]
pub struct ProviderStatus {
    /// The provider name/type.
    pub provider_type: String,
    /// Whether the provider is currently authenticated.
    pub is_authenticated: bool,
    /// The last successful sync time, if any.
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
    /// Any current error state.
    pub error: Option<String>,
    /// Number of calendars available.
    pub calendar_count: usize,
}

impl ProviderStatus {
    /// Creates a new provider status.
    pub fn new(provider_type: impl Into<String>) -> Self {
        Self {
            provider_type: provider_type.into(),
            is_authenticated: false,
            last_sync: None,
            error: None,
            calendar_count: 0,
        }
    }
}

/// A boxed future for async trait methods.
///
/// This is used because async functions in traits are not yet stable in a way
/// that works well with dynamic dispatch. Using boxed futures allows the trait
/// to be object-safe.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The core abstraction for calendar providers.
///
/// This trait defines the interface that all calendar backends must implement.
/// Providers handle fetching events from calendar servers, managing authentication,
/// and providing caching hints.
///
/// # Implementation Notes
///
/// - Implementations should be `Send + Sync` for use in async contexts
/// - The `fetch_events` method should expand recurring events if `expand_recurring` is set
/// - Providers should respect rate limits and implement backoff
/// - Authentication state should be managed internally
///
/// # Example Implementation
///
/// ```ignore
/// struct GoogleProvider {
///     client: reqwest::Client,
///     tokens: TokenStorage,
/// }
///
/// impl CalendarProvider for GoogleProvider {
///     fn name(&self) -> &str { "google" }
///
///     fn fetch_events(&self, options: FetchOptions) -> BoxFuture<'_, ProviderResult<FetchResult>> {
///         Box::pin(async move {
///             // Fetch from Google Calendar API
///             Ok(FetchResult::with_events(events))
///         })
///     }
///     // ... other methods
/// }
/// ```
pub trait CalendarProvider: Send + Sync {
    /// Returns the name/type of this provider (e.g., "google", "caldav").
    fn name(&self) -> &str;

    /// Fetches events from the calendar(s).
    ///
    /// This is the primary method for retrieving calendar data. It should:
    /// - Respect the time window in options
    /// - Expand recurring events if requested
    /// - Support conditional fetches via ETag/sync token
    /// - Handle pagination internally
    ///
    /// # Errors
    ///
    /// Returns `ProviderError` on network errors, authentication failures, etc.
    fn fetch_events(&self, options: FetchOptions) -> BoxFuture<'_, ProviderResult<FetchResult>>;

    /// Lists available calendars.
    ///
    /// Returns information about all calendars the user has access to.
    fn list_calendars(&self) -> BoxFuture<'_, ProviderResult<Vec<CalendarInfo>>>;

    /// Returns the current status of the provider.
    ///
    /// This includes authentication state, last sync time, and any errors.
    fn status(&self) -> BoxFuture<'_, ProviderStatus>;

    /// Refreshes the authentication tokens.
    ///
    /// This should be called when tokens are expired or about to expire.
    /// Returns an error if refresh fails (e.g., refresh token is invalid).
    fn refresh_auth(&self) -> BoxFuture<'_, ProviderResult<()>>;

    /// Checks if the provider is currently authenticated.
    ///
    /// A provider is authenticated if it has valid tokens that haven't expired.
    fn is_authenticated(&self) -> bool;

    /// Mutates a provider event.
    ///
    /// The default implementation reports unsupported operation.
    fn mutate_event(
        &self,
        _calendar_id: &str,
        _event_id: &str,
        _action: EventMutationAction,
    ) -> BoxFuture<'_, ProviderResult<()>> {
        Box::pin(async {
            Err(ProviderError::calendar(
                "event mutation is not supported by this provider",
            ))
        })
    }

    /// Returns the provider's configuration hint for the scheduler.
    ///
    /// This allows providers to suggest polling intervals based on their
    /// rate limits and typical update patterns.
    fn suggested_poll_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(60) // Default: 1 minute
    }
}

/// A provider that always returns an error.
///
/// This is useful for testing or as a placeholder when a provider
/// fails to initialize.
#[derive(Debug)]
pub struct ErrorProvider {
    name: String,
    error: ProviderError,
}

impl ErrorProvider {
    /// Creates a new error provider.
    pub fn new(name: impl Into<String>, error: ProviderError) -> Self {
        Self {
            name: name.into(),
            error,
        }
    }
}

impl CalendarProvider for ErrorProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn fetch_events(&self, _options: FetchOptions) -> BoxFuture<'_, ProviderResult<FetchResult>> {
        // Clone the error details since we can't clone ProviderError directly
        let error =
            ProviderError::new(self.error.code(), self.error.message()).with_provider(&self.name);
        Box::pin(async move { Err(error) })
    }

    fn list_calendars(&self) -> BoxFuture<'_, ProviderResult<Vec<CalendarInfo>>> {
        let error =
            ProviderError::new(self.error.code(), self.error.message()).with_provider(&self.name);
        Box::pin(async move { Err(error) })
    }

    fn status(&self) -> BoxFuture<'_, ProviderStatus> {
        let mut status = ProviderStatus::new(&self.name);
        status.error = Some(self.error.message().to_string());
        Box::pin(async move { status })
    }

    fn refresh_auth(&self) -> BoxFuture<'_, ProviderResult<()>> {
        let error =
            ProviderError::new(self.error.code(), self.error.message()).with_provider(&self.name);
        Box::pin(async move { Err(error) })
    }

    fn is_authenticated(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ProviderErrorCode;

    #[test]
    fn calendar_info_builder() {
        let info = CalendarInfo::new("cal-123", "Work Calendar")
            .with_primary(true)
            .with_timezone("America/New_York");

        assert_eq!(info.id, "cal-123");
        assert_eq!(info.name, "Work Calendar");
        assert!(info.is_primary);
        assert_eq!(info.timezone, Some("America/New_York".to_string()));
    }

    #[test]
    fn fetch_result_creation() {
        let result = FetchResult::with_events(vec![]).with_sync_token("token-abc");

        assert!(result.events.is_empty());
        assert_eq!(result.sync_token, Some("token-abc".to_string()));
        assert!(!result.not_modified);
    }

    #[test]
    fn fetch_result_not_modified() {
        let result = FetchResult::not_modified();

        assert!(result.events.is_empty());
        assert!(result.not_modified);
    }

    #[test]
    fn fetch_options_builder() {
        let window = TimeWindow::new(
            chrono::Utc::now(),
            chrono::Utc::now() + chrono::Duration::hours(24),
        );

        let options = FetchOptions::new()
            .with_time_window(window)
            .with_max_results(100)
            .with_if_none_match("etag-123")
            .with_expand_recurring(true)
            .with_calendar_ids(vec!["cal1".to_string(), "cal2".to_string()]);

        assert!(options.time_window.is_some());
        assert_eq!(options.max_results, Some(100));
        assert_eq!(options.if_none_match, Some("etag-123".to_string()));
        assert!(options.expand_recurring);
        assert_eq!(options.calendar_ids.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn provider_status_creation() {
        let status = ProviderStatus::new("google");

        assert_eq!(status.provider_type, "google");
        assert!(!status.is_authenticated);
        assert!(status.last_sync.is_none());
        assert!(status.error.is_none());
    }

    #[tokio::test]
    async fn error_provider_returns_error() {
        let provider = ErrorProvider::new("test", ProviderError::configuration("not configured"));

        assert_eq!(provider.name(), "test");
        assert!(!provider.is_authenticated());

        let result = provider.fetch_events(FetchOptions::new()).await;
        assert!(result.is_err());

        let status = provider.status().await;
        assert!(status.error.is_some());
    }

    #[test]
    fn error_provider_suggested_interval() {
        let provider = ErrorProvider::new(
            "test",
            ProviderError::new(ProviderErrorCode::ConfigurationError, "test"),
        );

        // Should use default interval
        assert_eq!(
            provider.suggested_poll_interval(),
            std::time::Duration::from_secs(60)
        );
    }
}
