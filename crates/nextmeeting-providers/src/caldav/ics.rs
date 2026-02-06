//! ICS/iCalendar parsing utilities.
//!
//! This module parses iCalendar (RFC 5545) data and converts it to [`RawEvent`].

use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
use icalendar::{Calendar, CalendarComponent, Component, DatePerhapsTime, Event, EventLike};
use regex::Regex;
use tracing::{debug, warn};

use crate::raw_event::{RawEvent, RawEventTime};

/// Parses ICS content and extracts events.
///
/// Returns a list of raw events. Recurring events should already be expanded
/// by the server when using time-range queries.
pub fn parse_ics_content(ics: &str, calendar_id: &str) -> Vec<RawEvent> {
    let calendar = match ics.parse::<Calendar>() {
        Ok(cal) => cal,
        Err(e) => {
            warn!(error = %e, "Failed to parse ICS content");
            return Vec::new();
        }
    };

    calendar
        .iter()
        .filter_map(|component| match component {
            CalendarComponent::Event(event) => parse_event(event, calendar_id),
            _ => None,
        })
        .collect()
}

/// Parses a single VEVENT component into a RawEvent.
fn parse_event(event: &Event, calendar_id: &str) -> Option<RawEvent> {
    // Extract required fields
    let uid = event.get_uid()?;
    let start_dt = event.get_start()?;
    let end_dt = event.get_end().or_else(|| {
        // If no end, use start + default duration based on whether all-day
        event.get_start()
    })?;

    let start = convert_date_time(start_dt);
    let end = convert_date_time(end_dt);

    let mut raw = RawEvent::new(uid, start, end, calendar_id);

    // Summary (title)
    if let Some(summary) = event.get_summary() {
        raw = raw.with_summary(summary);
    }

    // Description
    if let Some(description) = event.get_description() {
        raw = raw.with_description(description);
    }

    // Location
    if let Some(location) = event.get_location() {
        raw = raw.with_location(location);
    }

    // URL - check for explicit URL property
    if let Some(url) = event.property_value("URL") {
        raw.extra.insert("url".to_string(), url.to_string());
    }

    // Status
    if let Some(status) = event.get_status() {
        raw = raw.with_status(format!("{:?}", status));
    }

    // Recurrence ID indicates this is an instance of a recurring event
    if event.property_value("RECURRENCE-ID").is_some()
        && let Some(uid) = event.get_uid()
    {
        raw = raw.with_recurring(uid);
    }

    // Check for RRULE to detect recurring events
    if event.property_value("RRULE").is_some()
        && let Some(uid) = event.get_uid()
    {
        raw = raw.with_recurring(uid);
    }

    // Timestamps
    if let Some(created) = event.get_timestamp() {
        raw.created = Some(created);
    }
    if let Some(modified) = event.get_last_modified() {
        raw.updated = Some(modified);
    }

    debug!(
        uid = %raw.id,
        summary = ?raw.summary,
        start = ?raw.start,
        "Parsed event from ICS"
    );

    Some(raw)
}

/// Converts icalendar DatePerhapsTime to RawEventTime.
fn convert_date_time(dt: DatePerhapsTime) -> RawEventTime {
    match dt {
        DatePerhapsTime::Date(date) => RawEventTime::from_date(date),
        DatePerhapsTime::DateTime(cdt) => {
            // CalendarDateTime can be with or without timezone
            // Convert to UTC using chrono
            use icalendar::CalendarDateTime;
            let utc_dt = match cdt {
                CalendarDateTime::Utc(dt) => dt,
                CalendarDateTime::Floating(naive) => Utc.from_utc_datetime(&naive),
                CalendarDateTime::WithTimezone { date_time, tzid: _ } => {
                    // For simplicity, assume UTC if we can't resolve the timezone
                    Utc.from_utc_datetime(&date_time)
                }
            };
            RawEventTime::from_datetime(utc_dt)
        }
    }
}

/// Extracts a URL from the event's description or location fields.
///
/// This is a fallback for when there's no explicit URL property.
#[allow(dead_code)]
pub fn extract_url_from_text(text: &str) -> Option<String> {
    // Simple URL regex pattern - avoid problematic escapes
    let url_pattern = Regex::new(r"https?://[^\s<>]+").expect("URL regex should be valid");

    url_pattern.find(text).map(|m| {
        // Clean up trailing punctuation that might have been included
        let url = m.as_str();
        url.trim_end_matches([')', '"', '\'', '>', ']']).to_string()
    })
}

/// Parses an iCalendar datetime string.
///
/// Handles formats like:
/// - 20250205T100000Z (UTC)
/// - 20250205T100000 (local/naive)
/// - 20250205 (date only)
#[allow(dead_code)]
pub fn parse_icalendar_datetime(s: &str) -> Option<RawEventTime> {
    let s = s.trim();

    // Date only (YYYYMMDD)
    if s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
        let date = NaiveDate::parse_from_str(s, "%Y%m%d").ok()?;
        return Some(RawEventTime::from_date(date));
    }

    // DateTime with Z suffix (UTC)
    if s.ends_with('Z') {
        let dt = NaiveDateTime::parse_from_str(s.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()?;
        let utc = Utc.from_utc_datetime(&dt);
        return Some(RawEventTime::from_datetime(utc));
    }

    // Naive datetime (assume UTC)
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S") {
        let utc = Utc.from_utc_datetime(&dt);
        return Some(RawEventTime::from_datetime(utc));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ics() -> &'static str {
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         PRODID:-//Test//Test//EN\r\n\
         BEGIN:VEVENT\r\n\
         UID:test-event-1@example.com\r\n\
         DTSTART:20250205T100000Z\r\n\
         DTEND:20250205T110000Z\r\n\
         SUMMARY:Team Meeting\r\n\
         DESCRIPTION:Weekly sync meeting. Join: https://zoom.us/j/123456789\r\n\
         LOCATION:Conference Room A\r\n\
         STATUS:CONFIRMED\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR"
    }

    fn all_day_ics() -> &'static str {
        "BEGIN:VCALENDAR\r\n\
         VERSION:2.0\r\n\
         BEGIN:VEVENT\r\n\
         UID:all-day-1@example.com\r\n\
         DTSTART;VALUE=DATE:20250210\r\n\
         DTEND;VALUE=DATE:20250211\r\n\
         SUMMARY:Company Holiday\r\n\
         END:VEVENT\r\n\
         END:VCALENDAR"
    }

    #[test]
    fn parse_basic_event() {
        let events = parse_ics_content(sample_ics(), "test-cal");

        assert_eq!(events.len(), 1);
        let event = &events[0];

        assert_eq!(event.id, "test-event-1@example.com");
        assert_eq!(event.summary, Some("Team Meeting".to_string()));
        assert!(event.description.as_ref().unwrap().contains("Weekly sync"));
        assert_eq!(event.location, Some("Conference Room A".to_string()));
        assert_eq!(event.calendar_id, "test-cal");
        assert!(!event.is_all_day());
    }

    #[test]
    fn parse_all_day_event() {
        let events = parse_ics_content(all_day_ics(), "test-cal");

        assert_eq!(events.len(), 1);
        let event = &events[0];

        assert_eq!(event.id, "all-day-1@example.com");
        assert_eq!(event.summary, Some("Company Holiday".to_string()));
        assert!(event.is_all_day());
    }

    #[test]
    fn parse_icalendar_datetime_utc() {
        let result = parse_icalendar_datetime("20250205T143000Z");
        assert!(result.is_some());
        if let RawEventTime::DateTime(dt) = result.unwrap() {
            assert_eq!(dt.format("%Y-%m-%d %H:%M").to_string(), "2025-02-05 14:30");
        } else {
            panic!("Expected DateTime");
        }
    }

    #[test]
    fn parse_icalendar_datetime_date_only() {
        let result = parse_icalendar_datetime("20250210");
        assert!(result.is_some());
        if let RawEventTime::Date(date) = result.unwrap() {
            assert_eq!(date.format("%Y-%m-%d").to_string(), "2025-02-10");
        } else {
            panic!("Expected Date");
        }
    }

    #[test]
    fn extract_url_from_description() {
        let text = "Join the meeting at https://zoom.us/j/123456789 for the call";
        let url = extract_url_from_text(text);
        assert_eq!(url, Some("https://zoom.us/j/123456789".to_string()));
    }

    #[test]
    fn extract_url_handles_no_url() {
        let text = "No URL in this text";
        let url = extract_url_from_text(text);
        assert!(url.is_none());
    }
}
