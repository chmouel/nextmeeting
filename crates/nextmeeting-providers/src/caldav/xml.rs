//! XML utilities for CalDAV WebDAV operations.
//!
//! This module handles parsing and generating XML for WebDAV operations
//! like PROPFIND and REPORT.

use quick_xml::Writer;
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};

use chrono::{DateTime, Utc};
use std::io::Cursor;

/// DAV namespace
pub const DAV_NS: &str = "DAV:";
/// CalDAV namespace
pub const CALDAV_NS: &str = "urn:ietf:params:xml:ns:caldav";
/// CalendarServer namespace (for Apple servers)
pub const CS_NS: &str = "http://calendarserver.org/ns/";

/// A discovered calendar from PROPFIND.
#[derive(Debug, Clone)]
pub struct DiscoveredCalendar {
    /// The calendar's href (path).
    pub href: String,
    /// The display name.
    pub display_name: Option<String>,
    /// Calendar color (if available).
    pub color: Option<String>,
    /// The calendar description.
    pub description: Option<String>,
    /// The ctag (for change detection).
    pub ctag: Option<String>,
}

/// Generates a PROPFIND request body for calendar discovery.
///
/// This requests the properties needed to identify calendars:
/// - displayname
/// - resourcetype
/// - calendar-color (Apple extension)
/// - calendar-description
/// - getctag (Apple extension)
pub fn propfind_calendars_body() -> String {
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    // XML declaration is handled by quick-xml

    // <d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/">
    let mut propfind = BytesStart::new("d:propfind");
    propfind.push_attribute(("xmlns:d", DAV_NS));
    propfind.push_attribute(("xmlns:c", CALDAV_NS));
    propfind.push_attribute(("xmlns:cs", CS_NS));
    writer.write_event(Event::Start(propfind)).unwrap();

    // <d:prop>
    writer
        .write_event(Event::Start(BytesStart::new("d:prop")))
        .unwrap();

    // Properties we want
    write_empty_element(&mut writer, "d:displayname");
    write_empty_element(&mut writer, "d:resourcetype");
    write_empty_element(&mut writer, "c:calendar-description");
    write_empty_element(&mut writer, "cs:getctag");

    // </d:prop>
    writer
        .write_event(Event::End(BytesEnd::new("d:prop")))
        .unwrap();

    // </d:propfind>
    writer
        .write_event(Event::End(BytesEnd::new("d:propfind")))
        .unwrap();

    let result = writer.into_inner().into_inner();
    String::from_utf8(result).unwrap()
}

/// Generates a REPORT request body for fetching calendar events.
///
/// Uses calendar-query with time-range filter to expand recurring events.
pub fn calendar_query_body(start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    // <c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
    let mut query = BytesStart::new("c:calendar-query");
    query.push_attribute(("xmlns:d", DAV_NS));
    query.push_attribute(("xmlns:c", CALDAV_NS));
    writer.write_event(Event::Start(query)).unwrap();

    // <d:prop>
    writer
        .write_event(Event::Start(BytesStart::new("d:prop")))
        .unwrap();
    write_empty_element(&mut writer, "d:getetag");
    write_empty_element(&mut writer, "c:calendar-data");
    writer
        .write_event(Event::End(BytesEnd::new("d:prop")))
        .unwrap();

    // <c:filter>
    writer
        .write_event(Event::Start(BytesStart::new("c:filter")))
        .unwrap();

    // <c:comp-filter name="VCALENDAR">
    let mut vcal_filter = BytesStart::new("c:comp-filter");
    vcal_filter.push_attribute(("name", "VCALENDAR"));
    writer.write_event(Event::Start(vcal_filter)).unwrap();

    // <c:comp-filter name="VEVENT">
    let mut vevent_filter = BytesStart::new("c:comp-filter");
    vevent_filter.push_attribute(("name", "VEVENT"));
    writer.write_event(Event::Start(vevent_filter)).unwrap();

    // <c:time-range start="..." end="..."/>
    let mut time_range = BytesStart::new("c:time-range");
    time_range.push_attribute(("start", format_icalendar_datetime(start).as_str()));
    time_range.push_attribute(("end", format_icalendar_datetime(end).as_str()));
    writer.write_event(Event::Empty(time_range)).unwrap();

    // </c:comp-filter> (VEVENT)
    writer
        .write_event(Event::End(BytesEnd::new("c:comp-filter")))
        .unwrap();

    // </c:comp-filter> (VCALENDAR)
    writer
        .write_event(Event::End(BytesEnd::new("c:comp-filter")))
        .unwrap();

    // </c:filter>
    writer
        .write_event(Event::End(BytesEnd::new("c:filter")))
        .unwrap();

    // </c:calendar-query>
    writer
        .write_event(Event::End(BytesEnd::new("c:calendar-query")))
        .unwrap();

    let result = writer.into_inner().into_inner();
    String::from_utf8(result).unwrap()
}

/// Generates a calendar-multiget REPORT body to fetch specific events.
///
/// This is more efficient when fetching a subset of events by their hrefs.
pub fn calendar_multiget_body(hrefs: &[&str]) -> String {
    let mut writer = Writer::new(Cursor::new(Vec::new()));

    // <c:calendar-multiget xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
    let mut multiget = BytesStart::new("c:calendar-multiget");
    multiget.push_attribute(("xmlns:d", DAV_NS));
    multiget.push_attribute(("xmlns:c", CALDAV_NS));
    writer.write_event(Event::Start(multiget)).unwrap();

    // <d:prop>
    writer
        .write_event(Event::Start(BytesStart::new("d:prop")))
        .unwrap();
    write_empty_element(&mut writer, "d:getetag");
    write_empty_element(&mut writer, "c:calendar-data");
    writer
        .write_event(Event::End(BytesEnd::new("d:prop")))
        .unwrap();

    // <d:href>...</d:href> for each event
    for href in hrefs {
        writer
            .write_event(Event::Start(BytesStart::new("d:href")))
            .unwrap();
        writer
            .write_event(Event::Text(BytesText::new(href)))
            .unwrap();
        writer
            .write_event(Event::End(BytesEnd::new("d:href")))
            .unwrap();
    }

    // </c:calendar-multiget>
    writer
        .write_event(Event::End(BytesEnd::new("c:calendar-multiget")))
        .unwrap();

    let result = writer.into_inner().into_inner();
    String::from_utf8(result).unwrap()
}

/// Parses a PROPFIND response to extract calendar information.
pub fn parse_propfind_response(xml: &str) -> Vec<DiscoveredCalendar> {
    let mut calendars = Vec::new();

    // Use quick-xml reader to parse the response
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut current_href: Option<String> = None;
    let mut current_displayname: Option<String> = None;
    let mut current_description: Option<String> = None;
    let mut current_ctag: Option<String> = None;
    let mut is_calendar = false;
    let mut in_response = false;
    let mut current_element: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = local_name(&name);

                match local {
                    "response" => {
                        in_response = true;
                        current_href = None;
                        current_displayname = None;
                        current_description = None;
                        current_ctag = None;
                        is_calendar = false;
                    }
                    "href" | "displayname" | "calendar-description" | "getctag" => {
                        current_element = Some(local.to_string());
                    }
                    "calendar" => {
                        is_calendar = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = local_name(&name);

                if local == "response" && in_response {
                    // End of a response element - save if it's a calendar
                    if is_calendar {
                        if let Some(href) = current_href.take() {
                            calendars.push(DiscoveredCalendar {
                                href,
                                display_name: current_displayname.take(),
                                color: None,
                                description: current_description.take(),
                                ctag: current_ctag.take(),
                            });
                        }
                    }
                    in_response = false;
                }
                current_element = None;
            }
            Ok(Event::Text(e)) => {
                if let Some(ref elem) = current_element {
                    let text = e.unescape().unwrap_or_default().to_string();
                    match elem.as_str() {
                        "href" => current_href = Some(text),
                        "displayname" => current_displayname = Some(text),
                        "calendar-description" => current_description = Some(text),
                        "getctag" => current_ctag = Some(text),
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    calendars
}

/// Parses a REPORT response to extract calendar data (ICS content).
///
/// Returns a list of (href, etag, ics_data) tuples.
pub fn parse_report_response(xml: &str) -> Vec<(String, Option<String>, String)> {
    let mut results = Vec::new();

    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut current_href: Option<String> = None;
    let mut current_etag: Option<String> = None;
    let mut current_data: Option<String> = None;
    let mut in_response = false;
    let mut current_element: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = local_name(&name);

                match local {
                    "response" => {
                        in_response = true;
                        current_href = None;
                        current_etag = None;
                        current_data = None;
                    }
                    "href" | "getetag" | "calendar-data" => {
                        current_element = Some(local.to_string());
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let local = local_name(&name);

                if local == "response" && in_response {
                    if let (Some(href), Some(data)) = (current_href.take(), current_data.take()) {
                        results.push((href, current_etag.take(), data));
                    }
                    in_response = false;
                }
                current_element = None;
            }
            Ok(Event::Text(e)) => {
                if let Some(ref elem) = current_element {
                    let text = e.unescape().unwrap_or_default().to_string();
                    match elem.as_str() {
                        "href" => current_href = Some(text),
                        "getetag" => current_etag = Some(text.trim_matches('"').to_string()),
                        "calendar-data" => current_data = Some(text),
                        _ => {}
                    }
                }
            }
            Ok(Event::CData(e)) => {
                if let Some(ref elem) = current_element {
                    let text = String::from_utf8_lossy(&e).to_string();
                    match elem.as_str() {
                        "href" => current_href = Some(text),
                        "getetag" => current_etag = Some(text.trim_matches('"').to_string()),
                        "calendar-data" => current_data = Some(text),
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    results
}

/// Helper to write an empty XML element.
fn write_empty_element(writer: &mut Writer<Cursor<Vec<u8>>>, name: &str) {
    writer
        .write_event(Event::Empty(BytesStart::new(name)))
        .unwrap();
}

/// Extracts the local name from a potentially namespaced element name.
fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// Formats a datetime for iCalendar time-range filters (UTC format).
fn format_icalendar_datetime(dt: DateTime<Utc>) -> String {
    dt.format("%Y%m%dT%H%M%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn propfind_body_generation() {
        let body = propfind_calendars_body();
        assert!(body.contains("propfind"));
        assert!(body.contains("displayname"));
        assert!(body.contains("resourcetype"));
        assert!(body.contains("calendar-description"));
        assert!(body.contains("getctag"));
    }

    #[test]
    fn calendar_query_body_generation() {
        let start = Utc.with_ymd_and_hms(2025, 2, 1, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2025, 2, 28, 23, 59, 59).unwrap();

        let body = calendar_query_body(start, end);

        assert!(body.contains("calendar-query"));
        assert!(body.contains("time-range"));
        assert!(body.contains("20250201T000000Z"));
        assert!(body.contains("20250228T235959Z"));
        assert!(body.contains("VCALENDAR"));
        assert!(body.contains("VEVENT"));
    }

    #[test]
    fn calendar_multiget_body_generation() {
        let hrefs = vec!["/cal/event1.ics", "/cal/event2.ics"];
        let body = calendar_multiget_body(&hrefs);

        assert!(body.contains("calendar-multiget"));
        assert!(body.contains("/cal/event1.ics"));
        assert!(body.contains("/cal/event2.ics"));
    }

    #[test]
    fn parse_propfind_calendars() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<multistatus xmlns="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <response>
    <href>/calendars/user/work/</href>
    <propstat>
      <prop>
        <displayname>Work Calendar</displayname>
        <resourcetype>
          <collection/>
          <C:calendar/>
        </resourcetype>
      </prop>
      <status>HTTP/1.1 200 OK</status>
    </propstat>
  </response>
  <response>
    <href>/calendars/user/personal/</href>
    <propstat>
      <prop>
        <displayname>Personal</displayname>
        <resourcetype>
          <collection/>
        </resourcetype>
      </prop>
      <status>HTTP/1.1 200 OK</status>
    </propstat>
  </response>
</multistatus>"#;

        let calendars = parse_propfind_response(xml);

        // Should only find the "Work Calendar" since "Personal" is not a calendar
        assert_eq!(calendars.len(), 1);
        assert_eq!(calendars[0].href, "/calendars/user/work/");
        assert_eq!(calendars[0].display_name, Some("Work Calendar".to_string()));
    }

    #[test]
    fn parse_report_events() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<multistatus xmlns="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <response>
    <href>/calendars/user/work/event1.ics</href>
    <propstat>
      <prop>
        <getetag>"abc123"</getetag>
        <C:calendar-data>BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:event1@example.com
DTSTART:20250205T100000Z
DTEND:20250205T110000Z
SUMMARY:Team Meeting
END:VEVENT
END:VCALENDAR</C:calendar-data>
      </prop>
      <status>HTTP/1.1 200 OK</status>
    </propstat>
  </response>
</multistatus>"#;

        let results = parse_report_response(xml);

        assert_eq!(results.len(), 1);
        let (href, etag, data) = &results[0];
        assert_eq!(href, "/calendars/user/work/event1.ics");
        assert_eq!(etag.as_deref(), Some("abc123"));
        assert!(data.contains("Team Meeting"));
    }

    #[test]
    fn format_datetime_for_icalendar() {
        let dt = Utc.with_ymd_and_hms(2025, 2, 5, 14, 30, 0).unwrap();
        assert_eq!(format_icalendar_datetime(dt), "20250205T143000Z");
    }
}
