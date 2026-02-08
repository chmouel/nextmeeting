//! Event types for calendar events.
//!
//! This module provides core types for representing calendar events:
//! - [`NormalizedEvent`]: A provider-agnostic event representation
//! - [`EventLink`]: A meeting link with metadata (URL, meeting ID, passcode)
//! - [`LinkKind`]: The type of meeting link (Zoom, Meet, Teams, etc.)
//! - [`MeetingView`]: A display-ready view of a meeting for output formatting

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

use crate::time::EventTime;

/// The response status for an event attendee.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    #[default]
    Unknown,
}

/// The kind of meeting link.
///
/// This enum represents the various video conferencing services that can be
/// detected from event descriptions, locations, or dedicated conference fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    GoogleMeet,
    Zoom,
    ZoomGov,
    ZoomNative,
    Teams,
    Jitsi,
    Webex,
    Chime,
    RingCentral,
    GoToMeeting,
    GoToWebinar,
    BlueJeans,
    EightByEight,
    Demio,
    JoinMe,
    Whereby,
    UberConference,
    Blizz,
    TeamViewerMeeting,
    VSee,
    StarLeaf,
    Duo,
    Voov,
    FacebookWorkplace,
    Skype,
    Skype4Biz,
    Skype4BizSelfHosted,
    Lifesize,
    YouTube,
    VonageMeetings,
    MeetStream,
    Around,
    Jam,
    Discord,
    BlackboardCollab,
    CoScreen,
    Vowel,
    Zhumu,
    Lark,
    Feishu,
    Vimeo,
    Ovice,
    FaceTime,
    Chorus,
    Pop,
    Gong,
    Livestorm,
    Luma,
    Preply,
    UserZoom,
    Venue,
    Teemyco,
    Demodesk,
    ZohoCliq,
    Hangouts,
    Slack,
    Reclaim,
    Tuple,
    Gather,
    Pumble,
    SuitConference,
    DoxyMe,
    CalCom,
    ZmPage,
    LiveKit,
    Meetecho,
    StreamYard,
    /// Google Calendar event URL
    Calendar,
    /// Any other URL that might be a meeting link
    Other,
}

impl LinkKind {
    /// Returns a human-readable name for this link kind.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::GoogleMeet => "Google Meet",
            Self::Zoom => "Zoom",
            Self::ZoomGov => "Zoom (Gov)",
            Self::ZoomNative => "Zoom",
            Self::Teams => "Microsoft Teams",
            Self::Jitsi => "Jitsi",
            Self::Webex => "Cisco Webex",
            Self::Chime => "Amazon Chime",
            Self::RingCentral => "RingCentral",
            Self::GoToMeeting => "GoToMeeting",
            Self::GoToWebinar => "GoToWebinar",
            Self::BlueJeans => "BlueJeans",
            Self::EightByEight => "8x8",
            Self::Demio => "Demio",
            Self::JoinMe => "Join.me",
            Self::Whereby => "Whereby",
            Self::UberConference => "UberConference",
            Self::Blizz => "Blizz",
            Self::TeamViewerMeeting => "TeamViewer Meeting",
            Self::VSee => "VSee",
            Self::StarLeaf => "StarLeaf",
            Self::Duo => "Google Duo",
            Self::Voov => "Tencent VooV",
            Self::FacebookWorkplace => "Facebook Workplace",
            Self::Skype => "Skype",
            Self::Skype4Biz => "Skype for Business",
            Self::Skype4BizSelfHosted => "Skype for Business",
            Self::Lifesize => "Lifesize",
            Self::YouTube => "YouTube",
            Self::VonageMeetings => "Vonage Meetings",
            Self::MeetStream => "Google Meet Stream",
            Self::Around => "Around",
            Self::Jam => "Jam",
            Self::Discord => "Discord",
            Self::BlackboardCollab => "Blackboard Collaborate",
            Self::CoScreen => "CoScreen",
            Self::Vowel => "Vowel",
            Self::Zhumu => "Zhumu",
            Self::Lark => "Lark",
            Self::Feishu => "Feishu",
            Self::Vimeo => "Vimeo",
            Self::Ovice => "oVice",
            Self::FaceTime => "FaceTime",
            Self::Chorus => "Chorus",
            Self::Pop => "Pop",
            Self::Gong => "Gong",
            Self::Livestorm => "Livestorm",
            Self::Luma => "Luma",
            Self::Preply => "Preply",
            Self::UserZoom => "UserZoom",
            Self::Venue => "Venue",
            Self::Teemyco => "Teemyco",
            Self::Demodesk => "Demodesk",
            Self::ZohoCliq => "Zoho Cliq",
            Self::Hangouts => "Google Hangouts",
            Self::Slack => "Slack",
            Self::Reclaim => "Reclaim",
            Self::Tuple => "Tuple",
            Self::Gather => "Gather",
            Self::Pumble => "Pumble",
            Self::SuitConference => "Suit Conference",
            Self::DoxyMe => "Doxy.me",
            Self::CalCom => "Cal.com",
            Self::ZmPage => "zm.page",
            Self::LiveKit => "LiveKit Meet",
            Self::Meetecho => "Meetecho",
            Self::StreamYard => "StreamYard",
            Self::Calendar => "Calendar",
            Self::Other => "Link",
        }
    }

    /// Returns true if this is a video conferencing link (not just a calendar link).
    pub fn is_video_conference(&self) -> bool {
        !matches!(self, Self::Calendar | Self::Other | Self::YouTube)
    }
}

/// A link extracted from a calendar event.
///
/// Contains the URL, the detected service type, and optionally
/// meeting ID and passcode for services that support them (e.g., Zoom).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventLink {
    /// The kind of meeting link.
    pub kind: LinkKind,
    /// The normalized URL.
    pub url: String,
    /// The meeting ID, if extractable (e.g., Zoom meeting ID).
    pub meeting_id: Option<String>,
    /// The passcode, if extractable (e.g., Zoom passcode).
    pub passcode: Option<String>,
}

impl EventLink {
    /// Creates a new EventLink with the given kind and URL.
    pub fn new(kind: LinkKind, url: impl Into<String>) -> Self {
        Self {
            kind,
            url: url.into(),
            meeting_id: None,
            passcode: None,
        }
    }

    /// Creates a new EventLink with meeting ID and passcode.
    pub fn with_credentials(
        kind: LinkKind,
        url: impl Into<String>,
        meeting_id: Option<String>,
        passcode: Option<String>,
    ) -> Self {
        Self {
            kind,
            url: url.into(),
            meeting_id,
            passcode,
        }
    }

    /// Returns true if this link has a meeting ID.
    pub fn has_meeting_id(&self) -> bool {
        self.meeting_id.is_some()
    }

    /// Returns true if this link has a passcode.
    pub fn has_passcode(&self) -> bool {
        self.passcode.is_some()
    }
}

/// A normalized calendar event from any provider.
///
/// This is the canonical representation of an event after fetching from
/// a calendar provider (Google, CalDAV, etc.). It contains all the information
/// needed for filtering, display, and notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedEvent {
    /// Unique identifier for the event (provider-specific).
    pub id: String,
    /// The event title/summary.
    pub title: String,
    /// When the event starts.
    pub start: EventTime,
    /// When the event ends.
    pub end: EventTime,
    /// The original timezone of the event source, if known.
    /// Stored as IANA timezone identifier (e.g., "America/New_York").
    pub source_timezone: Option<String>,
    /// Extracted meeting links from the event.
    pub links: Vec<EventLink>,
    /// The raw location field from the event.
    pub raw_location: Option<String>,
    /// The raw description field from the event.
    pub raw_description: Option<String>,
    /// The calendar this event belongs to (calendar ID or URL).
    pub calendar_id: String,
    /// URL to view this event in the calendar (e.g., Google Calendar link).
    pub calendar_url: Option<String>,
    /// Whether this is an instance of a recurring event.
    pub is_recurring_instance: bool,
    /// User's response status for this event.
    pub user_response_status: ResponseStatus,
    /// Number of non-self attendees (for filtering solo events).
    pub other_attendee_count: usize,
}

impl NormalizedEvent {
    /// Creates a new NormalizedEvent with required fields.
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        start: EventTime,
        end: EventTime,
        calendar_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            start,
            end,
            source_timezone: None,
            links: Vec::new(),
            raw_location: None,
            raw_description: None,
            calendar_id: calendar_id.into(),
            calendar_url: None,
            is_recurring_instance: false,
            user_response_status: ResponseStatus::Unknown,
            other_attendee_count: 0,
        }
    }

    /// Returns true if this is an all-day event.
    pub fn is_all_day(&self) -> bool {
        self.start.is_all_day()
    }

    /// Returns the primary meeting link (first video conference link, or first link).
    pub fn primary_link(&self) -> Option<&EventLink> {
        // Prefer video conference links over calendar/other links
        self.links
            .iter()
            .find(|l| l.kind.is_video_conference())
            .or_else(|| self.links.first())
    }

    /// Returns secondary links (all links except the primary).
    pub fn secondary_links(&self) -> Vec<&EventLink> {
        let primary = self.primary_link();
        self.links
            .iter()
            .filter(|l| primary.is_none_or(|p| l.url != p.url))
            .collect()
    }

    /// Returns true if the event has any video conference link.
    pub fn has_video_link(&self) -> bool {
        self.links.iter().any(|l| l.kind.is_video_conference())
    }

    /// Checks if the event is currently ongoing at the given time.
    pub fn is_ongoing_at(&self, now: DateTime<Utc>) -> bool {
        let start = self.start.to_utc_datetime();
        let end = self.end.to_utc_datetime();
        start <= now && now < end
    }

    /// Returns the duration of the event in minutes.
    pub fn duration_minutes(&self) -> i64 {
        let duration = self.end.to_utc_datetime() - self.start.to_utc_datetime();
        duration.num_minutes()
    }

    /// Builder method to set source timezone.
    pub fn with_source_timezone(mut self, tz: impl Into<String>) -> Self {
        self.source_timezone = Some(tz.into());
        self
    }

    /// Builder method to add a link.
    pub fn with_link(mut self, link: EventLink) -> Self {
        self.links.push(link);
        self
    }

    /// Builder method to set links.
    pub fn with_links(mut self, links: Vec<EventLink>) -> Self {
        self.links = links;
        self
    }

    /// Builder method to set raw location.
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.raw_location = Some(location.into());
        self
    }

    /// Builder method to set raw description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.raw_description = Some(description.into());
        self
    }

    /// Builder method to set calendar URL.
    pub fn with_calendar_url(mut self, url: impl Into<String>) -> Self {
        self.calendar_url = Some(url.into());
        self
    }

    /// Builder method to mark as recurring instance.
    pub fn with_recurring(mut self, is_recurring: bool) -> Self {
        self.is_recurring_instance = is_recurring;
        self
    }

    /// Builder method to set user response status.
    pub fn with_user_response_status(mut self, status: ResponseStatus) -> Self {
        self.user_response_status = status;
        self
    }

    /// Builder method to set other attendee count.
    pub fn with_other_attendee_count(mut self, count: usize) -> Self {
        self.other_attendee_count = count;
        self
    }
}

/// A display-ready view of a meeting.
///
/// This struct is designed for output formatting and contains pre-computed
/// values for display, such as local times and status flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingView {
    /// The event ID (for deduplication and tracking).
    pub id: String,
    /// The meeting title (may be truncated or privacy-redacted).
    pub title: String,
    /// Start time in local timezone.
    pub start_local: DateTime<Local>,
    /// End time in local timezone.
    pub end_local: DateTime<Local>,
    /// Whether this is an all-day event.
    pub is_all_day: bool,
    /// Whether the meeting is currently ongoing.
    pub is_ongoing: bool,
    /// The primary meeting link for joining.
    pub primary_link: Option<EventLink>,
    /// Additional meeting links.
    pub secondary_links: Vec<EventLink>,
    /// URL to the calendar event.
    pub calendar_url: Option<String>,
    /// Calendar ID this event belongs to.
    pub calendar_id: String,
    /// User's response status for this event.
    pub user_response_status: ResponseStatus,
    /// Number of non-self attendees.
    pub other_attendee_count: usize,
    /// The event location, if available.
    pub location: Option<String>,
    /// The event description, if available.
    pub description: Option<String>,
}

impl MeetingView {
    /// Creates a MeetingView from a NormalizedEvent.
    ///
    /// Converts times to local timezone and computes status flags.
    pub fn from_event(event: &NormalizedEvent, now: DateTime<Utc>) -> Self {
        let start_utc = event.start.to_utc_datetime();
        let end_utc = event.end.to_utc_datetime();

        Self {
            id: event.id.clone(),
            title: event.title.clone(),
            start_local: start_utc.with_timezone(&Local),
            end_local: end_utc.with_timezone(&Local),
            is_all_day: event.is_all_day(),
            is_ongoing: event.is_ongoing_at(now),
            primary_link: event.primary_link().cloned(),
            secondary_links: event.secondary_links().into_iter().cloned().collect(),
            calendar_url: event.calendar_url.clone(),
            calendar_id: event.calendar_id.clone(),
            user_response_status: event.user_response_status,
            other_attendee_count: event.other_attendee_count,
            location: event.raw_location.clone(),
            description: event.raw_description.clone(),
        }
    }

    /// Returns minutes until the meeting starts from the given time.
    ///
    /// Returns negative values if the meeting has already started.
    pub fn minutes_until_start(&self, now: DateTime<Local>) -> i64 {
        (self.start_local - now).num_minutes()
    }

    /// Returns minutes until the meeting ends from the given time.
    ///
    /// Returns negative values if the meeting has already ended.
    pub fn minutes_until_end(&self, now: DateTime<Local>) -> i64 {
        (self.end_local - now).num_minutes()
    }

    /// Returns the duration of the meeting in minutes.
    pub fn duration_minutes(&self) -> i64 {
        (self.end_local - self.start_local).num_minutes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone};

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, s).unwrap()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    mod link_kind {
        use super::*;

        #[test]
        fn display_names() {
            assert_eq!(LinkKind::GoogleMeet.display_name(), "Google Meet");
            assert_eq!(LinkKind::Zoom.display_name(), "Zoom");
            assert_eq!(LinkKind::ZoomGov.display_name(), "Zoom (Gov)");
            assert_eq!(LinkKind::ZoomNative.display_name(), "Zoom");
            assert_eq!(LinkKind::Teams.display_name(), "Microsoft Teams");
            assert_eq!(LinkKind::Jitsi.display_name(), "Jitsi");
            assert_eq!(LinkKind::Webex.display_name(), "Cisco Webex");
            assert_eq!(LinkKind::Slack.display_name(), "Slack");
            assert_eq!(LinkKind::Calendar.display_name(), "Calendar");
            assert_eq!(LinkKind::Other.display_name(), "Link");
        }

        #[test]
        fn video_conference_check() {
            assert!(LinkKind::GoogleMeet.is_video_conference());
            assert!(LinkKind::Zoom.is_video_conference());
            assert!(LinkKind::ZoomGov.is_video_conference());
            assert!(LinkKind::ZoomNative.is_video_conference());
            assert!(LinkKind::Teams.is_video_conference());
            assert!(LinkKind::Jitsi.is_video_conference());
            assert!(LinkKind::Webex.is_video_conference());
            assert!(LinkKind::Slack.is_video_conference());
            assert!(!LinkKind::YouTube.is_video_conference());
            assert!(!LinkKind::Calendar.is_video_conference());
            assert!(!LinkKind::Other.is_video_conference());
        }

        #[test]
        fn serde_roundtrip() {
            let kind = LinkKind::GoogleMeet;
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, "\"google_meet\"");
            let parsed: LinkKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, parsed);
        }
    }

    mod event_link {
        use super::*;

        #[test]
        fn basic_creation() {
            let link = EventLink::new(LinkKind::Zoom, "https://zoom.us/j/123456");
            assert_eq!(link.kind, LinkKind::Zoom);
            assert_eq!(link.url, "https://zoom.us/j/123456");
            assert!(!link.has_meeting_id());
            assert!(!link.has_passcode());
        }

        #[test]
        fn with_credentials() {
            let link = EventLink::with_credentials(
                LinkKind::Zoom,
                "https://zoom.us/j/123456",
                Some("123456".to_string()),
                Some("secret".to_string()),
            );
            assert!(link.has_meeting_id());
            assert!(link.has_passcode());
            assert_eq!(link.meeting_id, Some("123456".to_string()));
            assert_eq!(link.passcode, Some("secret".to_string()));
        }

        #[test]
        fn serde_roundtrip() {
            let link = EventLink::with_credentials(
                LinkKind::Zoom,
                "https://zoom.us/j/123",
                Some("123".to_string()),
                None,
            );
            let json = serde_json::to_string(&link).unwrap();
            let parsed: EventLink = serde_json::from_str(&json).unwrap();
            assert_eq!(link, parsed);
        }
    }

    mod normalized_event {
        use super::*;

        fn sample_event() -> NormalizedEvent {
            NormalizedEvent::new(
                "evt-123",
                "Team Standup",
                EventTime::from_utc(utc(2025, 2, 5, 10, 0, 0)),
                EventTime::from_utc(utc(2025, 2, 5, 10, 30, 0)),
                "primary",
            )
        }

        #[test]
        fn basic_creation() {
            let event = sample_event();
            assert_eq!(event.id, "evt-123");
            assert_eq!(event.title, "Team Standup");
            assert!(!event.is_all_day());
            assert!(event.links.is_empty());
            assert_eq!(event.duration_minutes(), 30);
        }

        #[test]
        fn all_day_event() {
            let event = NormalizedEvent::new(
                "evt-456",
                "Conference",
                EventTime::from_date(date(2025, 2, 5)),
                EventTime::from_date(date(2025, 2, 6)),
                "primary",
            );
            assert!(event.is_all_day());
        }

        #[test]
        fn builder_pattern() {
            let event = sample_event()
                .with_source_timezone("America/New_York")
                .with_location("Room 101")
                .with_description("Weekly sync")
                .with_calendar_url("https://calendar.google.com/event/abc")
                .with_recurring(true)
                .with_link(EventLink::new(
                    LinkKind::GoogleMeet,
                    "https://meet.google.com/abc-defg-hij",
                ));

            assert_eq!(event.source_timezone, Some("America/New_York".to_string()));
            assert_eq!(event.raw_location, Some("Room 101".to_string()));
            assert_eq!(event.raw_description, Some("Weekly sync".to_string()));
            assert!(event.calendar_url.is_some());
            assert!(event.is_recurring_instance);
            assert_eq!(event.links.len(), 1);
        }

        #[test]
        fn primary_link_selection() {
            // No links
            let event = sample_event();
            assert!(event.primary_link().is_none());

            // Only calendar link
            let event = sample_event().with_link(EventLink::new(
                LinkKind::Calendar,
                "https://calendar.google.com",
            ));
            assert_eq!(event.primary_link().unwrap().kind, LinkKind::Calendar);

            // Video link preferred over calendar
            let event = sample_event()
                .with_link(EventLink::new(
                    LinkKind::Calendar,
                    "https://calendar.google.com",
                ))
                .with_link(EventLink::new(LinkKind::Zoom, "https://zoom.us/j/123"));
            assert_eq!(event.primary_link().unwrap().kind, LinkKind::Zoom);
        }

        #[test]
        fn secondary_links() {
            let event = sample_event()
                .with_link(EventLink::new(LinkKind::Zoom, "https://zoom.us/j/123"))
                .with_link(EventLink::new(
                    LinkKind::Calendar,
                    "https://calendar.google.com",
                ))
                .with_link(EventLink::new(
                    LinkKind::Other,
                    "https://docs.google.com/doc",
                ));

            let secondary = event.secondary_links();
            assert_eq!(secondary.len(), 2);
            // Primary is Zoom, so Calendar and Other should be secondary
            assert!(secondary.iter().all(|l| l.kind != LinkKind::Zoom));
        }

        #[test]
        fn ongoing_detection() {
            let event = sample_event(); // 10:00-10:30 UTC

            // Before event
            assert!(!event.is_ongoing_at(utc(2025, 2, 5, 9, 59, 59)));

            // At start
            assert!(event.is_ongoing_at(utc(2025, 2, 5, 10, 0, 0)));

            // During event
            assert!(event.is_ongoing_at(utc(2025, 2, 5, 10, 15, 0)));

            // At end (exclusive)
            assert!(!event.is_ongoing_at(utc(2025, 2, 5, 10, 30, 0)));

            // After event
            assert!(!event.is_ongoing_at(utc(2025, 2, 5, 11, 0, 0)));
        }

        #[test]
        fn serde_roundtrip() {
            let event = sample_event()
                .with_source_timezone("UTC")
                .with_link(EventLink::new(
                    LinkKind::GoogleMeet,
                    "https://meet.google.com/abc",
                ));

            let json = serde_json::to_string(&event).unwrap();
            let parsed: NormalizedEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, parsed);
        }
    }

    mod meeting_view {
        use super::*;

        fn sample_event() -> NormalizedEvent {
            NormalizedEvent::new(
                "evt-123",
                "Team Standup",
                EventTime::from_utc(utc(2025, 2, 5, 10, 0, 0)),
                EventTime::from_utc(utc(2025, 2, 5, 10, 30, 0)),
                "primary",
            )
            .with_link(EventLink::new(
                LinkKind::GoogleMeet,
                "https://meet.google.com/abc",
            ))
            .with_calendar_url("https://calendar.google.com/event/123")
        }

        #[test]
        fn from_event_before_start() {
            let event = sample_event();
            let now = utc(2025, 2, 5, 9, 45, 0);
            let view = MeetingView::from_event(&event, now);

            assert_eq!(view.id, "evt-123");
            assert_eq!(view.title, "Team Standup");
            assert!(!view.is_all_day);
            assert!(!view.is_ongoing);
            assert!(view.primary_link.is_some());
            assert_eq!(view.duration_minutes(), 30);
        }

        #[test]
        fn from_event_during_meeting() {
            let event = sample_event();
            let now = utc(2025, 2, 5, 10, 15, 0);
            let view = MeetingView::from_event(&event, now);

            assert!(view.is_ongoing);
        }

        #[test]
        fn minutes_until() {
            let event = sample_event();
            let now = utc(2025, 2, 5, 9, 45, 0);
            let view = MeetingView::from_event(&event, now);
            let now_local = now.with_timezone(&Local);

            assert_eq!(view.minutes_until_start(now_local), 15);
            assert_eq!(view.minutes_until_end(now_local), 45);
        }

        #[test]
        fn all_day_view() {
            let event = NormalizedEvent::new(
                "evt-456",
                "Day Off",
                EventTime::from_date(date(2025, 2, 5)),
                EventTime::from_date(date(2025, 2, 6)),
                "primary",
            );
            let now = utc(2025, 2, 5, 12, 0, 0);
            let view = MeetingView::from_event(&event, now);

            assert!(view.is_all_day);
            assert!(view.is_ongoing); // During the all-day event
        }

        #[test]
        fn serde_roundtrip() {
            let event = sample_event();
            let now = utc(2025, 2, 5, 9, 45, 0);
            let view = MeetingView::from_event(&event, now);

            let json = serde_json::to_string(&view).unwrap();
            let parsed: MeetingView = serde_json::from_str(&json).unwrap();
            assert_eq!(view, parsed);
        }
    }
}
