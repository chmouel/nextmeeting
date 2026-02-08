//! Request and response types for the nextmeeting protocol.

use chrono::{DateTime, Utc};
use nextmeeting_core::MeetingView;
use serde::{Deserialize, Serialize};

use crate::PROTOCOL_VERSION;

/// Message envelope wrapping all protocol messages.
///
/// Every message exchanged between client and server is wrapped in this envelope
/// which provides versioning and request correlation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope<T> {
    /// Protocol version (always "1" for v1).
    pub protocol_version: String,
    /// Unique request ID for correlation.
    pub request_id: String,
    /// The actual payload.
    pub payload: T,
}

impl<T> Envelope<T> {
    /// Creates a new envelope with the current protocol version.
    pub fn new(request_id: impl Into<String>, payload: T) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            request_id: request_id.into(),
            payload,
        }
    }

    /// Creates a request envelope.
    pub fn request(request_id: impl Into<String>, request: T) -> Self {
        Self::new(request_id, request)
    }

    /// Creates a response envelope.
    pub fn response(request_id: impl Into<String>, response: T) -> Self {
        Self::new(request_id, response)
    }

    /// Returns the protocol version.
    pub fn version(&self) -> &str {
        &self.protocol_version
    }

    /// Checks if this envelope uses a compatible protocol version.
    pub fn is_compatible(&self) -> bool {
        self.protocol_version == PROTOCOL_VERSION
    }
}

/// Request types that can be sent from client to server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Get upcoming meetings.
    GetMeetings {
        /// Optional filter for meetings.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<MeetingsFilter>,
    },

    /// Get server status.
    Status,

    /// Force a refresh of calendar data.
    Refresh {
        /// If true, bypass cache and fetch fresh data.
        force: bool,
    },

    /// Snooze notifications for a period.
    Snooze {
        /// Minutes to snooze notifications.
        minutes: u32,
    },

    /// Mutate a calendar event in a provider (e.g. decline/delete).
    MutateEvent {
        /// Provider name (e.g. "google:work").
        provider_name: String,
        /// Calendar identifier in that provider.
        calendar_id: String,
        /// Provider event identifier.
        event_id: String,
        /// Action to execute.
        action: EventMutationAction,
    },

    /// Request server shutdown.
    Shutdown,

    /// Ping to check server liveness.
    Ping,
}

impl Request {
    /// Creates a GetMeetings request with no filter.
    pub fn get_meetings() -> Self {
        Self::GetMeetings { filter: None }
    }

    /// Creates a GetMeetings request with a filter.
    pub fn get_meetings_with_filter(filter: MeetingsFilter) -> Self {
        Self::GetMeetings {
            filter: Some(filter),
        }
    }

    /// Creates a Refresh request.
    pub fn refresh(force: bool) -> Self {
        Self::Refresh { force }
    }

    /// Creates a Snooze request.
    pub fn snooze(minutes: u32) -> Self {
        Self::Snooze { minutes }
    }

    /// Creates a MutateEvent request.
    pub fn mutate_event(
        provider_name: impl Into<String>,
        calendar_id: impl Into<String>,
        event_id: impl Into<String>,
        action: EventMutationAction,
    ) -> Self {
        Self::MutateEvent {
            provider_name: provider_name.into(),
            calendar_id: calendar_id.into(),
            event_id: event_id.into(),
            action,
        }
    }
}

/// Event mutation actions supported by the protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventMutationAction {
    /// Decline an invited event.
    Decline,
    /// Delete an event.
    Delete,
}

/// Deserializes a value that can be either a single string or a Vec<String>.
/// Used for backward compatibility with the old single-pattern fields.
fn string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct StringOrVec;

    impl<'de> de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or a sequence of strings")
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<Vec<String>, E> {
            Ok(vec![value.to_string()])
        }

        fn visit_seq<S: de::SeqAccess<'de>>(self, mut seq: S) -> Result<Vec<String>, S::Error> {
            let mut v = Vec::new();
            while let Some(s) = seq.next_element()? {
                v.push(s);
            }
            Ok(v)
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

/// Filter options for GetMeetings request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingsFilter {
    /// Only return events happening today.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub today_only: bool,

    /// Maximum number of meetings to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,

    /// Skip all-day events.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_all_day: bool,

    /// Only include events matching these title patterns (any match retains).
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        alias = "include_title",
        deserialize_with = "string_or_vec"
    )]
    pub include_titles: Vec<String>,

    /// Exclude events matching these title patterns (any match removes).
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        alias = "exclude_title",
        deserialize_with = "string_or_vec"
    )]
    pub exclude_titles: Vec<String>,

    /// Only include events from these calendars.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_calendars: Vec<String>,

    /// Exclude events from these calendars.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_calendars: Vec<String>,

    /// Only include events starting within N minutes from now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within_minutes: Option<u32>,

    /// Only include events that have a meeting link.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub only_with_link: bool,

    /// Only include events within work hours (format: "HH:MM-HH:MM").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_hours: Option<String>,

    /// Enable privacy mode (replace titles with privacy_title).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub privacy: bool,

    /// Title to use when privacy mode is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_title: Option<String>,

    /// Skip events where the user has declined.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_declined: bool,

    /// Skip events where the user has tentatively accepted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_tentative: bool,

    /// Skip events where the user hasn't responded yet.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_pending: bool,

    /// Skip events without other attendees (solo events).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_without_guests: bool,
}

impl MeetingsFilter {
    /// Creates a new empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: set today_only.
    pub fn today_only(mut self, today_only: bool) -> Self {
        self.today_only = today_only;
        self
    }

    /// Builder: set limit.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Builder: set skip_all_day.
    pub fn skip_all_day(mut self, skip: bool) -> Self {
        self.skip_all_day = skip;
        self
    }

    /// Builder: add an include title pattern.
    pub fn include_title(mut self, pattern: impl Into<String>) -> Self {
        self.include_titles.push(pattern.into());
        self
    }

    /// Builder: set all include title patterns.
    pub fn include_titles(mut self, patterns: Vec<String>) -> Self {
        self.include_titles = patterns;
        self
    }

    /// Builder: add an exclude title pattern.
    pub fn exclude_title(mut self, pattern: impl Into<String>) -> Self {
        self.exclude_titles.push(pattern.into());
        self
    }

    /// Builder: set all exclude title patterns.
    pub fn exclude_titles(mut self, patterns: Vec<String>) -> Self {
        self.exclude_titles = patterns;
        self
    }

    /// Builder: add an include calendar pattern.
    pub fn include_calendar(mut self, pattern: impl Into<String>) -> Self {
        self.include_calendars.push(pattern.into());
        self
    }

    /// Builder: set all include calendar patterns.
    pub fn include_calendars(mut self, patterns: Vec<String>) -> Self {
        self.include_calendars = patterns;
        self
    }

    /// Builder: add an exclude calendar pattern.
    pub fn exclude_calendar(mut self, pattern: impl Into<String>) -> Self {
        self.exclude_calendars.push(pattern.into());
        self
    }

    /// Builder: set all exclude calendar patterns.
    pub fn exclude_calendars(mut self, patterns: Vec<String>) -> Self {
        self.exclude_calendars = patterns;
        self
    }

    /// Builder: set within_minutes filter.
    pub fn within_minutes(mut self, minutes: u32) -> Self {
        self.within_minutes = Some(minutes);
        self
    }

    /// Builder: set only_with_link filter.
    pub fn only_with_link(mut self, only: bool) -> Self {
        self.only_with_link = only;
        self
    }

    /// Builder: set work_hours filter (format: "HH:MM-HH:MM").
    pub fn work_hours(mut self, spec: impl Into<String>) -> Self {
        self.work_hours = Some(spec.into());
        self
    }

    /// Builder: enable privacy mode.
    pub fn privacy(mut self, privacy: bool) -> Self {
        self.privacy = privacy;
        self
    }

    /// Builder: set privacy title.
    pub fn privacy_title(mut self, title: impl Into<String>) -> Self {
        self.privacy_title = Some(title.into());
        self
    }

    /// Builder: set skip_declined.
    pub fn skip_declined(mut self, skip: bool) -> Self {
        self.skip_declined = skip;
        self
    }

    /// Builder: set skip_tentative.
    pub fn skip_tentative(mut self, skip: bool) -> Self {
        self.skip_tentative = skip;
        self
    }

    /// Builder: set skip_pending.
    pub fn skip_pending(mut self, skip: bool) -> Self {
        self.skip_pending = skip;
        self
    }

    /// Builder: set skip_without_guests.
    pub fn skip_without_guests(mut self, skip: bool) -> Self {
        self.skip_without_guests = skip;
        self
    }
}

/// Response types that can be sent from server to client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// List of meetings.
    Meetings {
        /// The meetings matching the request.
        meetings: Vec<MeetingView>,
    },

    /// Server status information.
    Status {
        /// Status details.
        #[serde(flatten)]
        info: StatusInfo,
    },

    /// Generic success response.
    Ok,

    /// Error response.
    Error {
        /// Error details.
        #[serde(flatten)]
        error: ErrorResponse,
    },

    /// Pong response to Ping.
    Pong,
}

impl Response {
    /// Creates a Meetings response.
    pub fn meetings(meetings: Vec<MeetingView>) -> Self {
        Self::Meetings { meetings }
    }

    /// Creates a Status response.
    pub fn status(info: StatusInfo) -> Self {
        Self::Status { info }
    }

    /// Creates an Error response.
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Error {
            error: ErrorResponse {
                code,
                message: message.into(),
            },
        }
    }

    /// Creates an error response from an ErrorResponse.
    pub fn from_error(error: ErrorResponse) -> Self {
        Self::Error { error }
    }

    /// Returns true if this is a success response (Ok, Meetings, Status, or Pong).
    pub fn is_success(&self) -> bool {
        !matches!(self, Self::Error { .. })
    }

    /// Returns the error if this is an error response.
    pub fn as_error(&self) -> Option<&ErrorResponse> {
        match self {
            Self::Error { error } => Some(error),
            _ => None,
        }
    }
}

/// Server status information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusInfo {
    /// Server uptime in seconds.
    pub uptime_seconds: u64,

    /// Last successful sync time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<DateTime<Utc>>,

    /// Status of each configured provider.
    pub providers: Vec<ProviderStatus>,

    /// If notifications are snoozed, when they resume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snoozed_until: Option<DateTime<Utc>>,
}

impl StatusInfo {
    /// Creates a new StatusInfo.
    pub fn new(uptime_seconds: u64) -> Self {
        Self {
            uptime_seconds,
            last_sync: None,
            providers: Vec::new(),
            snoozed_until: None,
        }
    }

    /// Builder: set last_sync.
    pub fn with_last_sync(mut self, last_sync: DateTime<Utc>) -> Self {
        self.last_sync = Some(last_sync);
        self
    }

    /// Builder: add a provider status.
    pub fn with_provider(mut self, provider: ProviderStatus) -> Self {
        self.providers.push(provider);
        self
    }

    /// Builder: set snoozed_until.
    pub fn with_snoozed_until(mut self, until: DateTime<Utc>) -> Self {
        self.snoozed_until = Some(until);
        self
    }
}

/// Status of a calendar provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderStatus {
    /// Provider name (e.g., "google", "caldav").
    pub name: String,

    /// Whether the provider is currently healthy.
    pub healthy: bool,

    /// Last successful fetch time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fetch: Option<DateTime<Utc>>,

    /// Error message if unhealthy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Number of events from this provider.
    pub event_count: usize,
}

impl ProviderStatus {
    /// Creates a healthy provider status.
    pub fn healthy(name: impl Into<String>, event_count: usize) -> Self {
        Self {
            name: name.into(),
            healthy: true,
            last_fetch: None,
            error: None,
            event_count,
        }
    }

    /// Creates an unhealthy provider status.
    pub fn unhealthy(name: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            healthy: false,
            last_fetch: None,
            error: Some(error.into()),
            event_count: 0,
        }
    }

    /// Builder: set last_fetch.
    pub fn with_last_fetch(mut self, last_fetch: DateTime<Utc>) -> Self {
        self.last_fetch = Some(last_fetch);
        self
    }
}

/// Error codes for protocol errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Unknown or internal error.
    InternalError,

    /// Invalid request format.
    InvalidRequest,

    /// Request timed out.
    Timeout,

    /// Provider authentication failed.
    AuthenticationFailed,

    /// Provider returned an error.
    ProviderError,

    /// Rate limited by provider.
    RateLimited,

    /// Requested resource not found.
    NotFound,

    /// Server is shutting down.
    ShuttingDown,
}

impl ErrorCode {
    /// Returns a human-readable description of the error code.
    pub fn description(&self) -> &'static str {
        match self {
            Self::InternalError => "An internal error occurred",
            Self::InvalidRequest => "The request was invalid",
            Self::Timeout => "The request timed out",
            Self::AuthenticationFailed => "Authentication failed",
            Self::ProviderError => "Calendar provider returned an error",
            Self::RateLimited => "Rate limited by calendar provider",
            Self::NotFound => "Requested resource not found",
            Self::ShuttingDown => "Server is shutting down",
        }
    }
}

/// Error response details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error code.
    pub code: ErrorCode,
    /// Human-readable error message.
    pub message: String,
}

impl ErrorResponse {
    /// Creates a new error response.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Creates an internal error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalError, message)
    }

    /// Creates an invalid request error.
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidRequest, message)
    }
}

impl std::fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.description(), self.message)
    }
}

impl std::error::Error for ErrorResponse {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_creation() {
        let envelope = Envelope::request("req-123", Request::Ping);
        assert_eq!(envelope.protocol_version, "1");
        assert_eq!(envelope.request_id, "req-123");
        assert!(envelope.is_compatible());
    }

    #[test]
    fn envelope_incompatible_version() {
        let envelope = Envelope {
            protocol_version: "2".to_string(),
            request_id: "req-123".to_string(),
            payload: Request::Ping,
        };
        assert!(!envelope.is_compatible());
    }

    #[test]
    fn request_serde_ping() {
        let request = Request::Ping;
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"type":"ping"}"#);

        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Request::Ping);
    }

    #[test]
    fn request_serde_get_meetings() {
        let request = Request::get_meetings();
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"type":"get_meetings"}"#);

        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Request::GetMeetings { filter: None });
    }

    #[test]
    fn request_serde_get_meetings_with_filter() {
        let filter = MeetingsFilter::new().today_only(true).limit(5);
        let request = Request::get_meetings_with_filter(filter);
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("today_only"));
        assert!(json.contains("limit"));

        let parsed: Request = serde_json::from_str(&json).unwrap();
        match parsed {
            Request::GetMeetings { filter: Some(f) } => {
                assert!(f.today_only);
                assert_eq!(f.limit, Some(5));
            }
            _ => panic!("unexpected request type"),
        }
    }

    #[test]
    fn request_serde_refresh() {
        let request = Request::refresh(true);
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"type":"refresh","force":true}"#);

        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Request::Refresh { force: true });
    }

    #[test]
    fn request_serde_snooze() {
        let request = Request::snooze(30);
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"type":"snooze","minutes":30}"#);

        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Request::Snooze { minutes: 30 });
    }

    #[test]
    fn request_serde_mutate_event() {
        let request = Request::mutate_event(
            "google:work",
            "primary",
            "evt-123",
            EventMutationAction::Decline,
        );
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"type\":\"mutate_event\""));
        assert!(json.contains("\"action\":\"decline\""));

        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed,
            Request::MutateEvent {
                provider_name: "google:work".to_string(),
                calendar_id: "primary".to_string(),
                event_id: "evt-123".to_string(),
                action: EventMutationAction::Decline,
            }
        );
    }

    #[test]
    fn response_serde_ok() {
        let response = Response::Ok;
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"type":"ok"}"#);

        let parsed: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Response::Ok);
    }

    #[test]
    fn response_serde_pong() {
        let response = Response::Pong;
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"type":"pong"}"#);
    }

    #[test]
    fn response_serde_error() {
        let response = Response::error(ErrorCode::InvalidRequest, "missing field");
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("invalid_request"));
        assert!(json.contains("missing field"));

        let parsed: Response = serde_json::from_str(&json).unwrap();
        assert!(!parsed.is_success());
        let error = parsed.as_error().unwrap();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
    }

    #[test]
    fn response_serde_status() {
        let info = StatusInfo::new(3600).with_provider(ProviderStatus::healthy("google", 10));
        let response = Response::status(info);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("uptime_seconds"));
        assert!(json.contains("3600"));
        assert!(json.contains("google"));

        let parsed: Response = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_success());
    }

    #[test]
    fn meetings_filter_builder() {
        let filter = MeetingsFilter::new()
            .today_only(true)
            .limit(10)
            .skip_all_day(true)
            .include_title("standup")
            .exclude_title("optional")
            .include_calendar("primary")
            .exclude_calendar("holidays")
            .within_minutes(30)
            .only_with_link(true)
            .work_hours("09:00-18:00")
            .privacy(true)
            .privacy_title("Busy");

        assert!(filter.today_only);
        assert_eq!(filter.limit, Some(10));
        assert!(filter.skip_all_day);
        assert_eq!(filter.include_titles, vec!["standup".to_string()]);
        assert_eq!(filter.exclude_titles, vec!["optional".to_string()]);
        assert_eq!(filter.include_calendars, vec!["primary".to_string()]);
        assert_eq!(filter.exclude_calendars, vec!["holidays".to_string()]);
        assert_eq!(filter.within_minutes, Some(30));
        assert!(filter.only_with_link);
        assert_eq!(filter.work_hours, Some("09:00-18:00".to_string()));
        assert!(filter.privacy);
        assert_eq!(filter.privacy_title, Some("Busy".to_string()));
    }

    #[test]
    fn meetings_filter_serde_alias() {
        // Test backward compatibility with singular field names
        let json = r#"{"include_title":"standup","exclude_title":"optional"}"#;
        let filter: MeetingsFilter = serde_json::from_str(json).unwrap();
        assert_eq!(filter.include_titles, vec!["standup".to_string()]);
        assert_eq!(filter.exclude_titles, vec!["optional".to_string()]);
    }

    #[test]
    fn error_code_description() {
        assert!(!ErrorCode::InternalError.description().is_empty());
        assert!(!ErrorCode::InvalidRequest.description().is_empty());
        assert!(!ErrorCode::Timeout.description().is_empty());
    }

    #[test]
    fn error_response_display() {
        let error = ErrorResponse::new(ErrorCode::InvalidRequest, "bad request");
        let display = format!("{}", error);
        assert!(display.contains("invalid"));
        assert!(display.contains("bad request"));
    }

    #[test]
    fn full_envelope_roundtrip() {
        let request = Envelope::request("req-abc", Request::Ping);
        let json = serde_json::to_string(&request).unwrap();
        let parsed: Envelope<Request> = serde_json::from_str(&json).unwrap();
        assert_eq!(request, parsed);

        let response = Envelope::response("req-abc", Response::Pong);
        let json = serde_json::to_string(&response).unwrap();
        let parsed: Envelope<Response> = serde_json::from_str(&json).unwrap();
        assert_eq!(response, parsed);
    }
}
