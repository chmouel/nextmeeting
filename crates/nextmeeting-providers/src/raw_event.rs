//! Raw event type from calendar providers.
//!
//! This module defines [`RawEvent`], a provider-agnostic representation of
//! calendar event data as it comes from a provider (Google, CalDAV, etc.)
//! before normalization.
//!
//! The raw event preserves all available fields from the provider and is
//! then converted to a [`NormalizedEvent`] for use in the rest of the system.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// The time specification for a raw event.
///
/// Calendar providers return times in different formats:
/// - RFC3339 datetime with timezone
/// - Date-only for all-day events
/// - Sometimes with explicit timezone identifiers
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum RawEventTime {
    /// A specific datetime in UTC.
    DateTime(DateTime<Utc>),
    /// An all-day event date (no specific time).
    Date(NaiveDate),
}

impl RawEventTime {
    /// Creates a RawEventTime from a UTC datetime.
    pub fn from_datetime(dt: DateTime<Utc>) -> Self {
        Self::DateTime(dt)
    }

    /// Creates a RawEventTime from a date (all-day event).
    pub fn from_date(date: NaiveDate) -> Self {
        Self::Date(date)
    }

    /// Returns true if this is an all-day event time.
    pub fn is_all_day(&self) -> bool {
        matches!(self, Self::Date(_))
    }
}

/// The response status for an event attendee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    /// The attendee has accepted the invitation.
    Accepted,
    /// The attendee has declined the invitation.
    Declined,
    /// The attendee has tentatively accepted.
    Tentative,
    /// The attendee has not responded.
    NeedsAction,
    /// Unknown response status.
    Unknown,
}

impl Default for ResponseStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// An attendee of a calendar event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawAttendee {
    /// The attendee's email address.
    pub email: String,
    /// The attendee's display name, if available.
    pub display_name: Option<String>,
    /// Whether this attendee is the organizer.
    pub organizer: bool,
    /// Whether this attendee represents a resource (room, equipment).
    pub resource: bool,
    /// Whether this attendee is optional.
    pub optional: bool,
    /// The attendee's response status.
    pub response_status: ResponseStatus,
    /// Whether this attendee entry represents "self" (the authenticated user).
    pub is_self: bool,
}

impl RawAttendee {
    /// Creates a new attendee with the given email.
    pub fn new(email: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            display_name: None,
            organizer: false,
            resource: false,
            optional: false,
            response_status: ResponseStatus::Unknown,
            is_self: false,
        }
    }
}

/// Conference data associated with an event (e.g., Google Meet, Zoom).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawConferenceData {
    /// The type of conference (e.g., "hangoutsMeet", "addOn").
    pub conference_type: Option<String>,
    /// Entry points for joining the conference.
    pub entry_points: Vec<RawEntryPoint>,
    /// The conference solution name (e.g., "Google Meet", "Zoom").
    pub solution_name: Option<String>,
}

/// An entry point for joining a conference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEntryPoint {
    /// The type of entry point (e.g., "video", "phone", "sip").
    pub entry_point_type: String,
    /// The URI for this entry point.
    pub uri: Option<String>,
    /// A label for this entry point (e.g., phone number).
    pub label: Option<String>,
    /// The meeting code/ID.
    pub meeting_code: Option<String>,
    /// The passcode for the meeting.
    pub passcode: Option<String>,
    /// The PIN for phone dial-in.
    pub pin: Option<String>,
}

/// A raw calendar event from a provider.
///
/// This struct contains all the fields that might be available from calendar
/// providers. Not all fields will be populated by all providers. The event
/// is converted to a [`NormalizedEvent`] for further processing.
///
/// # Provider-specific notes
///
/// - **Google Calendar**: Most fields are directly mapped from the Events API.
/// - **CalDAV/ICS**: Fields are parsed from iCalendar components.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEvent {
    // === Required fields ===
    /// Unique identifier for the event within the provider.
    pub id: String,

    /// When the event starts.
    pub start: RawEventTime,

    /// When the event ends.
    pub end: RawEventTime,

    // === Common fields ===
    /// The event title/summary.
    pub summary: Option<String>,

    /// The event description (may contain HTML).
    pub description: Option<String>,

    /// The event location.
    pub location: Option<String>,

    /// The calendar this event belongs to.
    pub calendar_id: String,

    // === Timezone information ===
    /// The timezone of the event as an IANA identifier (e.g., "America/New_York").
    /// This is typically the timezone in which the event was created.
    pub timezone: Option<String>,

    // === Status and visibility ===
    /// The event status (e.g., "confirmed", "tentative", "cancelled").
    pub status: Option<String>,

    /// The visibility of the event (e.g., "default", "public", "private").
    pub visibility: Option<String>,

    // === Recurrence ===
    /// Whether this event is an instance of a recurring series.
    pub is_recurring_instance: bool,

    /// The ID of the recurring event this instance belongs to.
    pub recurring_event_id: Option<String>,

    /// The original start time of this recurring instance (for exceptions).
    pub original_start: Option<RawEventTime>,

    // === Attendees ===
    /// List of event attendees.
    pub attendees: Vec<RawAttendee>,

    /// The organizer's email address.
    pub organizer_email: Option<String>,

    /// The creator's email address.
    pub creator_email: Option<String>,

    // === Links and conference ===
    /// A direct link to view this event in the calendar UI.
    pub html_link: Option<String>,

    /// Conference data (video meeting info).
    pub conference_data: Option<RawConferenceData>,

    // === Timestamps ===
    /// When the event was created.
    pub created: Option<DateTime<Utc>>,

    /// When the event was last updated.
    pub updated: Option<DateTime<Utc>>,

    // === Provider-specific ===
    /// The ETag for conditional fetching (Google Calendar).
    pub etag: Option<String>,

    /// Additional provider-specific data stored as key-value pairs.
    /// This allows preserving data that doesn't map to standard fields.
    #[serde(default)]
    pub extra: std::collections::HashMap<String, String>,
}

impl RawEvent {
    /// Creates a new raw event with the minimum required fields.
    pub fn new(
        id: impl Into<String>,
        start: RawEventTime,
        end: RawEventTime,
        calendar_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            start,
            end,
            calendar_id: calendar_id.into(),
            summary: None,
            description: None,
            location: None,
            timezone: None,
            status: None,
            visibility: None,
            is_recurring_instance: false,
            recurring_event_id: None,
            original_start: None,
            attendees: Vec::new(),
            organizer_email: None,
            creator_email: None,
            html_link: None,
            conference_data: None,
            created: None,
            updated: None,
            etag: None,
            extra: std::collections::HashMap::new(),
        }
    }

    /// Returns the effective title, falling back to "(No title)" if empty.
    pub fn effective_title(&self) -> &str {
        self.summary
            .as_ref()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.as_str())
            .unwrap_or("(No title)")
    }

    /// Returns true if the event is cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(|s| s.eq_ignore_ascii_case("cancelled"))
    }

    /// Returns true if this is an all-day event.
    pub fn is_all_day(&self) -> bool {
        self.start.is_all_day()
    }

    /// Builder method to set the summary.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Builder method to set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Builder method to set the location.
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Builder method to set the timezone.
    pub fn with_timezone(mut self, timezone: impl Into<String>) -> Self {
        self.timezone = Some(timezone.into());
        self
    }

    /// Builder method to set the HTML link.
    pub fn with_html_link(mut self, html_link: impl Into<String>) -> Self {
        self.html_link = Some(html_link.into());
        self
    }

    /// Builder method to set recurrence info.
    pub fn with_recurring(mut self, recurring_event_id: impl Into<String>) -> Self {
        self.is_recurring_instance = true;
        self.recurring_event_id = Some(recurring_event_id.into());
        self
    }

    /// Builder method to set conference data.
    pub fn with_conference_data(mut self, conference_data: RawConferenceData) -> Self {
        self.conference_data = Some(conference_data);
        self
    }

    /// Builder method to add an attendee.
    pub fn with_attendee(mut self, attendee: RawAttendee) -> Self {
        self.attendees.push(attendee);
        self
    }

    /// Builder method to set the status.
    pub fn with_status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_datetime() -> DateTime<Utc> {
        "2025-02-05T10:00:00Z".parse().unwrap()
    }

    fn sample_date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, 2, 5).unwrap()
    }

    #[test]
    fn raw_event_time_variants() {
        let dt = RawEventTime::from_datetime(sample_datetime());
        assert!(!dt.is_all_day());

        let date = RawEventTime::from_date(sample_date());
        assert!(date.is_all_day());
    }

    #[test]
    fn raw_event_creation() {
        let start = RawEventTime::from_datetime(sample_datetime());
        let end = RawEventTime::from_datetime(sample_datetime());
        let event = RawEvent::new("evt-123", start, end, "primary");

        assert_eq!(event.id, "evt-123");
        assert_eq!(event.calendar_id, "primary");
        assert_eq!(event.effective_title(), "(No title)");
        assert!(!event.is_cancelled());
        assert!(!event.is_all_day());
    }

    #[test]
    fn raw_event_builder() {
        let start = RawEventTime::from_datetime(sample_datetime());
        let end = RawEventTime::from_datetime(sample_datetime());
        let event = RawEvent::new("evt-123", start, end, "primary")
            .with_summary("Team Meeting")
            .with_description("Weekly sync")
            .with_location("Room 101")
            .with_timezone("America/New_York")
            .with_html_link("https://calendar.google.com/event/123")
            .with_recurring("recur-abc");

        assert_eq!(event.effective_title(), "Team Meeting");
        assert_eq!(event.description, Some("Weekly sync".to_string()));
        assert_eq!(event.location, Some("Room 101".to_string()));
        assert_eq!(event.timezone, Some("America/New_York".to_string()));
        assert!(event.is_recurring_instance);
        assert_eq!(event.recurring_event_id, Some("recur-abc".to_string()));
    }

    #[test]
    fn raw_event_cancelled() {
        let start = RawEventTime::from_datetime(sample_datetime());
        let end = RawEventTime::from_datetime(sample_datetime());
        let event = RawEvent::new("evt-123", start, end, "primary").with_status("cancelled");

        assert!(event.is_cancelled());
    }

    #[test]
    fn raw_event_all_day() {
        let start = RawEventTime::from_date(sample_date());
        let end = RawEventTime::from_date(sample_date());
        let event = RawEvent::new("evt-123", start, end, "primary");

        assert!(event.is_all_day());
    }

    #[test]
    fn attendee_creation() {
        let attendee = RawAttendee::new("user@example.com");
        assert_eq!(attendee.email, "user@example.com");
        assert!(!attendee.organizer);
        assert!(!attendee.is_self);
        assert_eq!(attendee.response_status, ResponseStatus::Unknown);
    }

    #[test]
    fn serde_roundtrip() {
        let start = RawEventTime::from_datetime(sample_datetime());
        let end = RawEventTime::from_datetime(sample_datetime());
        let event = RawEvent::new("evt-123", start, end, "primary")
            .with_summary("Test Event")
            .with_timezone("UTC");

        let json = serde_json::to_string(&event).unwrap();
        let parsed: RawEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }
}
