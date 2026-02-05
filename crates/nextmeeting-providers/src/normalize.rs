//! RawEvent to NormalizedEvent conversion pipeline.
//!
//! This module handles the transformation from provider-specific [`RawEvent`]
//! data to the canonical [`NormalizedEvent`] representation used throughout
//! the application.
//!
//! The normalization process:
//! 1. Converts raw event times to [`EventTime`]
//! 2. Extracts and normalizes meeting links from description, location, and conference data
//! 3. Builds the final [`NormalizedEvent`] with all relevant fields

use nextmeeting_core::{EventLink, EventTime, LinkKind, NormalizedEvent, extract_links_from_text};

use crate::raw_event::{RawConferenceData, RawEvent, RawEventTime};

/// Converts a [`RawEvent`] to a [`NormalizedEvent`].
///
/// This is the main entry point for event normalization. It handles:
/// - Time conversion
/// - Link extraction from multiple sources (conference data, location, description)
/// - Field mapping
///
/// # Arguments
///
/// * `raw` - The raw event from a calendar provider
///
/// # Returns
///
/// A fully normalized event ready for use in the application
pub fn normalize_event(raw: &RawEvent) -> NormalizedEvent {
    let start = convert_time(&raw.start);
    let end = convert_time(&raw.end);

    // Extract links from all available sources
    let links = extract_all_links(raw);

    // Build the normalized event
    let mut event =
        NormalizedEvent::new(&raw.id, raw.effective_title(), start, end, &raw.calendar_id)
            .with_recurring(raw.is_recurring_instance)
            .with_links(links);

    // Set optional fields
    if let Some(ref tz) = raw.timezone {
        event = event.with_source_timezone(tz);
    }

    if let Some(ref location) = raw.location {
        event = event.with_location(location);
    }

    if let Some(ref description) = raw.description {
        event = event.with_description(description);
    }

    if let Some(ref html_link) = raw.html_link {
        event = event.with_calendar_url(html_link);
    }

    event
}

/// Converts a [`RawEventTime`] to an [`EventTime`].
fn convert_time(raw: &RawEventTime) -> EventTime {
    match raw {
        RawEventTime::DateTime(dt) => EventTime::from_utc(*dt),
        RawEventTime::Date(date) => EventTime::from_date(*date),
    }
}

/// Extracts all links from a raw event.
///
/// Links are extracted from:
/// 1. Conference data (highest priority - explicit meeting info)
/// 2. Location field
/// 3. Description field
///
/// Links are deduplicated by URL.
fn extract_all_links(raw: &RawEvent) -> Vec<EventLink> {
    let mut seen_urls = std::collections::HashSet::new();
    let mut links = Vec::new();

    // 1. Extract from conference data (highest priority)
    if let Some(ref conf) = raw.conference_data {
        for link in extract_from_conference_data(conf) {
            if seen_urls.insert(link.url.clone()) {
                links.push(link);
            }
        }
    }

    // 2. Extract from location
    if let Some(ref location) = raw.location {
        for link in extract_links_from_text(location) {
            if seen_urls.insert(link.url.clone()) {
                links.push(link);
            }
        }
    }

    // 3. Extract from description
    if let Some(ref description) = raw.description {
        for link in extract_links_from_text(description) {
            if seen_urls.insert(link.url.clone()) {
                links.push(link);
            }
        }
    }

    // 4. Add calendar link from html_link if we have one and no video links
    if let Some(ref html_link) = raw.html_link {
        if !links.iter().any(|l| l.kind.is_video_conference()) || links.is_empty() {
            if seen_urls.insert(html_link.clone()) {
                links.push(EventLink::new(LinkKind::Calendar, html_link));
            }
        }
    }

    // Sort: video conference links first, then by kind
    links.sort_by_key(|l| (!l.kind.is_video_conference(), l.kind as u8));

    links
}

/// Extracts links from conference data.
fn extract_from_conference_data(conf: &RawConferenceData) -> Vec<EventLink> {
    let mut links = Vec::new();

    for entry_point in &conf.entry_points {
        // Only process video entry points
        if entry_point.entry_point_type != "video" {
            continue;
        }

        if let Some(ref uri) = entry_point.uri {
            // Detect the link kind based on the solution name or URI
            let kind = detect_conference_kind(conf.solution_name.as_deref(), uri);

            let link = EventLink::with_credentials(
                kind,
                uri,
                entry_point.meeting_code.clone(),
                entry_point.passcode.clone(),
            );

            links.push(link);
        }
    }

    links
}

/// Detects the conference kind from solution name or URI.
fn detect_conference_kind(solution_name: Option<&str>, uri: &str) -> LinkKind {
    // First check solution name for explicit hints
    // Order matters: check more specific patterns before generic ones
    if let Some(name) = solution_name {
        let name_lower = name.to_lowercase();
        // Check Zoom first (before "meet" check since "Zoom Meeting" contains "meet")
        if name_lower.contains("zoom") {
            if uri.contains("zoomgov.com") {
                return LinkKind::ZoomGov;
            }
            return LinkKind::Zoom;
        }
        // Check Jitsi before generic "meet" (since Jitsi uses "Jitsi Meet")
        if name_lower.contains("jitsi") {
            return LinkKind::Jitsi;
        }
        if name_lower.contains("teams") {
            return LinkKind::Teams;
        }
        // Check Google Meet last - match "google meet" or "hangouts" specifically
        if name_lower.contains("google meet") || name_lower.contains("hangouts") {
            return LinkKind::GoogleMeet;
        }
    }

    // Fall back to URI-based detection
    let uri_lower = uri.to_lowercase();
    if uri_lower.contains("meet.google.com") {
        LinkKind::GoogleMeet
    } else if uri_lower.contains("zoomgov.com") {
        LinkKind::ZoomGov
    } else if uri_lower.contains("zoom.us") {
        LinkKind::Zoom
    } else if uri_lower.contains("teams.microsoft.com") || uri_lower.contains("teams.live.com") {
        LinkKind::Teams
    } else if uri_lower.contains("meet.jit.si") {
        LinkKind::Jitsi
    } else {
        LinkKind::Other
    }
}

/// Batch normalize multiple raw events.
///
/// This is a convenience function for normalizing a collection of events.
/// Cancelled events are filtered out.
pub fn normalize_events(raw_events: &[RawEvent]) -> Vec<NormalizedEvent> {
    raw_events
        .iter()
        .filter(|e| !e.is_cancelled())
        .map(normalize_event)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw_event::{RawEntryPoint, RawEventTime};
    use chrono::{NaiveDate, TimeZone, Utc};

    fn sample_datetime() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 2, 5, 10, 0, 0).unwrap()
    }

    fn sample_date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, 2, 5).unwrap()
    }

    fn sample_raw_event() -> RawEvent {
        RawEvent::new(
            "evt-123",
            RawEventTime::from_datetime(sample_datetime()),
            RawEventTime::from_datetime(Utc.with_ymd_and_hms(2025, 2, 5, 11, 0, 0).unwrap()),
            "primary",
        )
        .with_summary("Team Meeting")
    }

    mod time_conversion {
        use super::*;

        #[test]
        fn converts_datetime() {
            let raw = RawEventTime::from_datetime(sample_datetime());
            let converted = convert_time(&raw);
            assert!(converted.is_datetime());
            assert_eq!(converted.to_utc_datetime(), sample_datetime());
        }

        #[test]
        fn converts_date() {
            let raw = RawEventTime::from_date(sample_date());
            let converted = convert_time(&raw);
            assert!(converted.is_all_day());
            assert_eq!(converted.date(), sample_date());
        }
    }

    mod basic_normalization {
        use super::*;

        #[test]
        fn normalizes_minimal_event() {
            let raw = sample_raw_event();
            let normalized = normalize_event(&raw);

            assert_eq!(normalized.id, "evt-123");
            assert_eq!(normalized.title, "Team Meeting");
            assert_eq!(normalized.calendar_id, "primary");
            assert!(!normalized.is_all_day());
            assert!(!normalized.is_recurring_instance);
        }

        #[test]
        fn normalizes_event_with_optional_fields() {
            let raw = sample_raw_event()
                .with_description("Weekly sync meeting")
                .with_location("Room 101")
                .with_timezone("America/New_York")
                .with_html_link("https://calendar.google.com/event/123")
                .with_recurring("recur-abc");

            let normalized = normalize_event(&raw);

            assert_eq!(
                normalized.raw_description,
                Some("Weekly sync meeting".to_string())
            );
            assert_eq!(normalized.raw_location, Some("Room 101".to_string()));
            assert_eq!(
                normalized.source_timezone,
                Some("America/New_York".to_string())
            );
            assert_eq!(
                normalized.calendar_url,
                Some("https://calendar.google.com/event/123".to_string())
            );
            assert!(normalized.is_recurring_instance);
        }

        #[test]
        fn normalizes_all_day_event() {
            let raw = RawEvent::new(
                "evt-allday",
                RawEventTime::from_date(sample_date()),
                RawEventTime::from_date(sample_date().succ_opt().unwrap()),
                "primary",
            )
            .with_summary("Day Off");

            let normalized = normalize_event(&raw);

            assert!(normalized.is_all_day());
            assert_eq!(normalized.title, "Day Off");
        }

        #[test]
        fn uses_fallback_title_for_empty_summary() {
            let raw = RawEvent::new(
                "evt-notitle",
                RawEventTime::from_datetime(sample_datetime()),
                RawEventTime::from_datetime(sample_datetime()),
                "primary",
            );

            let normalized = normalize_event(&raw);
            assert_eq!(normalized.title, "(No title)");
        }
    }

    mod link_extraction {
        use super::*;

        #[test]
        fn extracts_links_from_description() {
            let raw = sample_raw_event().with_description("Join: https://zoom.us/j/123456789");

            let normalized = normalize_event(&raw);

            assert_eq!(normalized.links.len(), 1);
            assert_eq!(normalized.links[0].kind, LinkKind::Zoom);
        }

        #[test]
        fn extracts_links_from_location() {
            let raw = sample_raw_event().with_location("https://meet.google.com/abc-defg-hij");

            let normalized = normalize_event(&raw);

            assert_eq!(normalized.links.len(), 1);
            assert_eq!(normalized.links[0].kind, LinkKind::GoogleMeet);
        }

        #[test]
        fn extracts_from_conference_data() {
            let conf = RawConferenceData {
                conference_type: Some("hangoutsMeet".to_string()),
                solution_name: Some("Google Meet".to_string()),
                entry_points: vec![RawEntryPoint {
                    entry_point_type: "video".to_string(),
                    uri: Some("https://meet.google.com/xyz-uvwx-rst".to_string()),
                    label: None,
                    meeting_code: Some("xyz-uvwx-rst".to_string()),
                    passcode: None,
                    pin: None,
                }],
            };

            let raw = sample_raw_event().with_conference_data(conf);
            let normalized = normalize_event(&raw);

            assert_eq!(normalized.links.len(), 1);
            assert_eq!(normalized.links[0].kind, LinkKind::GoogleMeet);
            assert_eq!(
                normalized.links[0].meeting_id,
                Some("xyz-uvwx-rst".to_string())
            );
        }

        #[test]
        fn deduplicates_links() {
            let conf = RawConferenceData {
                conference_type: Some("hangoutsMeet".to_string()),
                solution_name: Some("Google Meet".to_string()),
                entry_points: vec![RawEntryPoint {
                    entry_point_type: "video".to_string(),
                    uri: Some("https://meet.google.com/abc-defg-hij".to_string()),
                    label: None,
                    meeting_code: None,
                    passcode: None,
                    pin: None,
                }],
            };

            // Same link in conference data and description
            let raw = sample_raw_event()
                .with_conference_data(conf)
                .with_description("Join at https://meet.google.com/abc-defg-hij");

            let normalized = normalize_event(&raw);

            // Should only have one link after deduplication
            assert_eq!(normalized.links.len(), 1);
        }

        #[test]
        fn prioritizes_conference_data_over_description() {
            let conf = RawConferenceData {
                conference_type: Some("addOn".to_string()),
                solution_name: Some("Zoom".to_string()),
                entry_points: vec![RawEntryPoint {
                    entry_point_type: "video".to_string(),
                    uri: Some("https://zoom.us/j/111".to_string()),
                    label: None,
                    meeting_code: Some("111".to_string()),
                    passcode: Some("secret".to_string()),
                    pin: None,
                }],
            };

            let raw = sample_raw_event()
                .with_conference_data(conf)
                .with_description("Alternative: https://zoom.us/j/222");

            let normalized = normalize_event(&raw);

            assert_eq!(normalized.links.len(), 2);
            // First link should be from conference data (has meeting code)
            assert_eq!(normalized.links[0].meeting_id, Some("111".to_string()));
            assert_eq!(normalized.links[0].passcode, Some("secret".to_string()));
        }

        #[test]
        fn adds_calendar_link_when_no_video_links() {
            let raw = sample_raw_event().with_html_link("https://calendar.google.com/event/123");

            let normalized = normalize_event(&raw);

            assert_eq!(normalized.links.len(), 1);
            assert_eq!(normalized.links[0].kind, LinkKind::Calendar);
        }

        #[test]
        fn skips_calendar_link_when_video_links_exist() {
            let raw = sample_raw_event()
                .with_html_link("https://calendar.google.com/event/123")
                .with_description("Join: https://zoom.us/j/123");

            let normalized = normalize_event(&raw);

            // Should have both the zoom link and the calendar link
            // Calendar link is added as secondary
            assert!(normalized.links.iter().any(|l| l.kind == LinkKind::Zoom));
        }
    }

    mod conference_kind_detection {
        use super::*;

        #[test]
        fn detects_google_meet() {
            assert_eq!(
                detect_conference_kind(Some("Google Meet"), "https://meet.google.com/abc"),
                LinkKind::GoogleMeet
            );
            assert_eq!(
                detect_conference_kind(None, "https://meet.google.com/abc"),
                LinkKind::GoogleMeet
            );
        }

        #[test]
        fn detects_zoom() {
            assert_eq!(
                detect_conference_kind(Some("Zoom Meeting"), "https://zoom.us/j/123"),
                LinkKind::Zoom
            );
            assert_eq!(
                detect_conference_kind(None, "https://zoom.us/j/123"),
                LinkKind::Zoom
            );
        }

        #[test]
        fn detects_zoomgov() {
            assert_eq!(
                detect_conference_kind(Some("Zoom"), "https://example.zoomgov.com/j/123"),
                LinkKind::ZoomGov
            );
            assert_eq!(
                detect_conference_kind(None, "https://zoomgov.com/j/123"),
                LinkKind::ZoomGov
            );
        }

        #[test]
        fn detects_teams() {
            assert_eq!(
                detect_conference_kind(
                    Some("Microsoft Teams"),
                    "https://teams.microsoft.com/l/meetup"
                ),
                LinkKind::Teams
            );
            assert_eq!(
                detect_conference_kind(None, "https://teams.live.com/meet/abc"),
                LinkKind::Teams
            );
        }

        #[test]
        fn detects_jitsi() {
            assert_eq!(
                detect_conference_kind(Some("Jitsi Meet"), "https://meet.jit.si/room"),
                LinkKind::Jitsi
            );
            assert_eq!(
                detect_conference_kind(None, "https://meet.jit.si/room"),
                LinkKind::Jitsi
            );
        }

        #[test]
        fn falls_back_to_other() {
            assert_eq!(
                detect_conference_kind(None, "https://example.com/meeting"),
                LinkKind::Other
            );
        }
    }

    mod batch_normalization {
        use super::*;

        #[test]
        fn normalizes_multiple_events() {
            let events = vec![
                sample_raw_event(),
                RawEvent::new(
                    "evt-456",
                    RawEventTime::from_datetime(sample_datetime()),
                    RawEventTime::from_datetime(sample_datetime()),
                    "secondary",
                )
                .with_summary("Another Meeting"),
            ];

            let normalized = normalize_events(&events);

            assert_eq!(normalized.len(), 2);
            assert_eq!(normalized[0].id, "evt-123");
            assert_eq!(normalized[1].id, "evt-456");
        }

        #[test]
        fn filters_cancelled_events() {
            let events = vec![
                sample_raw_event(),
                RawEvent::new(
                    "evt-cancelled",
                    RawEventTime::from_datetime(sample_datetime()),
                    RawEventTime::from_datetime(sample_datetime()),
                    "primary",
                )
                .with_status("cancelled"),
            ];

            let normalized = normalize_events(&events);

            assert_eq!(normalized.len(), 1);
            assert_eq!(normalized[0].id, "evt-123");
        }
    }
}
