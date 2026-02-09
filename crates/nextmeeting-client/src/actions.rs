//! Meeting actions: open URLs, copy to clipboard, snooze.

use nextmeeting_core::MeetingView;
use nextmeeting_protocol::{Request, Response};
use tracing::{debug, info};

use crate::error::{ClientError, ClientResult};
use crate::socket::SocketClient;

/// Opens the meeting URL in the default browser.
pub fn open_meeting_url(meetings: &[MeetingView]) -> ClientResult<()> {
    let meeting = first_meeting_with_link(meetings)?;
    let link = meeting
        .primary_link
        .as_ref()
        .ok_or_else(|| ClientError::Action("next meeting has no meeting URL".into()))?;

    info!(url = %link.url, "opening meeting URL");
    open::that(&link.url).map_err(|e| ClientError::Action(format!("failed to open URL: {}", e)))?;

    Ok(())
}

/// Copies the meeting URL to the clipboard.
pub fn copy_meeting_url(meetings: &[MeetingView]) -> ClientResult<()> {
    let meeting = first_meeting_with_link(meetings)?;
    let link = meeting
        .primary_link
        .as_ref()
        .ok_or_else(|| ClientError::Action("next meeting has no meeting URL".into()))?;

    info!(url = %link.url, "copying meeting URL to clipboard");

    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| ClientError::Action(format!("failed to access clipboard: {}", e)))?;

    clipboard
        .set_text(&link.url)
        .map_err(|e| ClientError::Action(format!("failed to copy to clipboard: {}", e)))?;

    println!("{}", link.url);
    Ok(())
}

/// Opens the calendar day view in the default browser.
pub fn open_calendar_day(
    meetings: &[MeetingView],
    google_domain: Option<&str>,
) -> ClientResult<()> {
    // Try to find a calendar URL from meetings, or construct Google Calendar URL
    if let Some(meeting) = meetings.first()
        && let Some(ref url) = meeting.calendar_url
    {
        debug!(url = %url, "opening calendar URL from meeting");
        open::that(url).map_err(|e| ClientError::Action(format!("failed to open URL: {}", e)))?;
        return Ok(());
    }

    // Fallback: open Google Calendar
    let base = if let Some(domain) = google_domain {
        format!("https://calendar.google.com/a/{}", domain)
    } else {
        "https://calendar.google.com".to_string()
    };

    info!(url = %base, "opening Google Calendar");
    open::that(&base).map_err(|e| ClientError::Action(format!("failed to open URL: {}", e)))?;

    Ok(())
}

/// Opens a specific calendar event URL in edit mode in the default browser.
pub fn edit_calendar_event_url(
    url: &str,
    event_id: &str,
    google_domain: Option<&str>,
) -> ClientResult<()> {
    let url = url.trim();
    if url.is_empty() {
        return Err(ClientError::Action("calendar event URL is empty".into()));
    }

    let edit_url = calendar_edit_url(url, event_id, google_domain);
    info!(url = %edit_url, "opening calendar event editor URL");
    open::that(&edit_url).map_err(|e| ClientError::Action(format!("failed to open URL: {}", e)))?;
    Ok(())
}

fn calendar_edit_url(url: &str, event_id: &str, google_domain: Option<&str>) -> String {
    if url.contains("calendar.google.") || url.contains("google.com/calendar") {
        let base = if let Some(domain) = google_domain {
            format!("https://calendar.google.com/a/{domain}/calendar/r/eventedit/{event_id}")
        } else {
            format!("https://calendar.google.com/calendar/r/eventedit/{event_id}")
        };
        return base;
    }
    url.to_string()
}

/// Sends a snooze request to the server.
pub async fn snooze(client: &SocketClient, minutes: u32) -> ClientResult<()> {
    info!(minutes = minutes, "snoozing notifications");

    let response = client.send(Request::snooze(minutes)).await?;
    match response {
        Response::Ok => {
            println!("Notifications snoozed for {} minutes.", minutes);
            Ok(())
        }
        Response::Error { error } => Err(ClientError::Protocol(format!(
            "snooze failed: {}",
            error.message
        ))),
        _ => Err(ClientError::Protocol(
            "unexpected response to snooze request".into(),
        )),
    }
}

/// Sends a refresh request to the server.
pub async fn refresh(client: &SocketClient) -> ClientResult<()> {
    info!("requesting calendar refresh");

    let response = client.send(Request::refresh(true)).await?;
    match response {
        Response::Ok => {
            println!("Calendar refresh triggered.");
            Ok(())
        }
        Response::Error { error } => Err(ClientError::Protocol(format!(
            "refresh failed: {}",
            error.message
        ))),
        _ => Err(ClientError::Protocol(
            "unexpected response to refresh request".into(),
        )),
    }
}

/// Opens the meeting URL using a custom command.
pub fn open_meeting_url_with(meetings: &[MeetingView], command: &str) -> ClientResult<()> {
    let meeting = first_meeting_with_link(meetings)?;
    let link = meeting
        .primary_link
        .as_ref()
        .ok_or_else(|| ClientError::Action("next meeting has no meeting URL".into()))?;

    info!(url = %link.url, command = %command, "opening meeting URL with custom command");

    std::process::Command::new(command)
        .arg(&link.url)
        .spawn()
        .map_err(|e| ClientError::Action(format!("failed to run '{}': {}", command, e)))?;

    Ok(())
}

/// Copies the meeting ID to the clipboard.
///
/// Extracts the meeting ID from the meeting URL. For example:
/// - Google Meet: `https://meet.google.com/abc-def-ghi` → `abc-def-ghi`
/// - Zoom: `https://zoom.us/j/12345678` → `12345678`
pub fn copy_meeting_id(meetings: &[MeetingView]) -> ClientResult<()> {
    let meeting = first_meeting_with_link(meetings)?;
    let link = meeting
        .primary_link
        .as_ref()
        .ok_or_else(|| ClientError::Action("next meeting has no meeting URL".into()))?;

    let meeting_id = extract_meeting_id(&link.url);
    info!(id = %meeting_id, "copying meeting ID to clipboard");

    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| ClientError::Action(format!("failed to access clipboard: {}", e)))?;

    clipboard
        .set_text(&meeting_id)
        .map_err(|e| ClientError::Action(format!("failed to copy to clipboard: {}", e)))?;

    println!("{}", meeting_id);
    Ok(())
}

/// Copies the meeting passcode to the clipboard if found in the URL.
pub fn copy_meeting_passcode(meetings: &[MeetingView]) -> ClientResult<()> {
    let meeting = first_meeting_with_link(meetings)?;
    let link = meeting
        .primary_link
        .as_ref()
        .ok_or_else(|| ClientError::Action("next meeting has no meeting URL".into()))?;

    let passcode = extract_passcode(&link.url)
        .ok_or_else(|| ClientError::Action("no passcode found in meeting URL".into()))?;

    info!("copying meeting passcode to clipboard");

    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| ClientError::Action(format!("failed to access clipboard: {}", e)))?;

    clipboard
        .set_text(&passcode)
        .map_err(|e| ClientError::Action(format!("failed to copy to clipboard: {}", e)))?;

    println!("{}", passcode);
    Ok(())
}

/// Opens a meeting link found on the clipboard.
pub fn open_link_from_clipboard() -> ClientResult<()> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| ClientError::Action(format!("failed to access clipboard: {}", e)))?;

    let text = clipboard
        .get_text()
        .map_err(|e| ClientError::Action(format!("failed to read clipboard: {}", e)))?;

    // Find a URL in the clipboard text
    let url = find_url_in_text(&text)
        .ok_or_else(|| ClientError::Action("no URL found on clipboard".into()))?;

    info!(url = %url, "opening URL from clipboard");
    open::that(&url).map_err(|e| ClientError::Action(format!("failed to open URL: {}", e)))?;

    Ok(())
}

/// Creates a new meeting by opening the appropriate service's create URL.
pub fn create_meeting(
    service: &str,
    custom_url: Option<&str>,
    google_domain: Option<&str>,
) -> ClientResult<()> {
    let url = if let Some(custom) = custom_url {
        custom.to_string()
    } else {
        match service.to_lowercase().as_str() {
            "meet" | "google" | "gmeet" => {
                if let Some(domain) = google_domain {
                    format!(
                        "https://meet.google.com/new?authuser=0&hs=122&hd={}",
                        domain
                    )
                } else {
                    "https://meet.google.com/new".to_string()
                }
            }
            "zoom" => "https://zoom.us/start/videomeeting".to_string(),
            "teams" => "https://teams.microsoft.com/l/meeting/new".to_string(),
            "gcal" | "calendar" => {
                let base = if let Some(domain) = google_domain {
                    format!("https://calendar.google.com/a/{}", domain)
                } else {
                    "https://calendar.google.com".to_string()
                };
                format!("{}/r/eventedit", base)
            }
            _ => {
                return Err(ClientError::Action(format!(
                    "unknown service '{}'. Supported: meet, zoom, teams, gcal",
                    service
                )));
            }
        }
    };

    info!(url = %url, service = %service, "creating new meeting");
    open::that(&url).map_err(|e| ClientError::Action(format!("failed to open URL: {}", e)))?;

    Ok(())
}

/// Extracts the meeting ID from a URL.
fn extract_meeting_id(url: &str) -> String {
    // Google Meet: last path segment
    if url.contains("meet.google.com")
        && let Some(id) = url.rsplit('/').next()
    {
        return id.split('?').next().unwrap_or(id).to_string();
    }

    // Zoom: extract from /j/ path
    if (url.contains("zoom.us") || url.contains("zoom.com"))
        && let Some(pos) = url.find("/j/")
    {
        let after = &url[pos + 3..];
        return after.split('?').next().unwrap_or(after).to_string();
    }

    // Teams: return meeting ID from path
    if (url.contains("teams.microsoft.com") || url.contains("teams.live.com"))
        && let Some(pos) = url.find("/meetup-join/")
    {
        let after = &url[pos + 13..];
        return after
            .split('?')
            .next()
            .unwrap_or(after)
            .split('/')
            .next()
            .unwrap_or(after)
            .to_string();
    }

    // Fallback: last path segment
    url.rsplit('/')
        .next()
        .and_then(|s| s.split('?').next())
        .unwrap_or(url)
        .to_string()
}

/// Extracts the passcode from a meeting URL query string.
fn extract_passcode(url: &str) -> Option<String> {
    // Check for pwd= or passcode= in query string
    let query = url.split('?').nth(1)?;
    for param in query.split('&') {
        if let Some(value) = param
            .strip_prefix("pwd=")
            .or_else(|| param.strip_prefix("passcode="))
        {
            return Some(value.to_string());
        }
    }
    None
}

/// Finds the first URL in the given text.
fn find_url_in_text(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        if word.starts_with("https://") || word.starts_with("http://") {
            return Some(word.to_string());
        }
    }
    None
}

/// Returns the first non-all-day meeting, or the first meeting.
fn first_meeting_with_link(meetings: &[MeetingView]) -> ClientResult<&MeetingView> {
    // Prefer non-all-day meeting with a link
    if let Some(m) = meetings
        .iter()
        .find(|m| !m.is_all_day && m.primary_link.is_some())
    {
        return Ok(m);
    }

    // Any meeting with a link
    if let Some(m) = meetings.iter().find(|m| m.primary_link.is_some()) {
        return Ok(m);
    }

    // Any meeting at all
    meetings
        .first()
        .ok_or_else(|| ClientError::Action("no meetings found".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_meeting_id_google_meet() {
        assert_eq!(
            extract_meeting_id("https://meet.google.com/abc-def-ghi"),
            "abc-def-ghi"
        );
        assert_eq!(
            extract_meeting_id("https://meet.google.com/abc-def-ghi?authuser=0"),
            "abc-def-ghi"
        );
    }

    #[test]
    fn extract_meeting_id_zoom() {
        assert_eq!(extract_meeting_id("https://zoom.us/j/12345678"), "12345678");
        assert_eq!(
            extract_meeting_id("https://zoom.us/j/12345678?pwd=abc"),
            "12345678"
        );
    }

    #[test]
    fn extract_meeting_id_fallback() {
        assert_eq!(extract_meeting_id("https://example.com/meeting/xyz"), "xyz");
    }

    #[test]
    fn extract_passcode_zoom() {
        assert_eq!(
            extract_passcode("https://zoom.us/j/123?pwd=abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(
            extract_passcode("https://zoom.us/j/123?passcode=xyz"),
            Some("xyz".to_string())
        );
    }

    #[test]
    fn extract_passcode_none() {
        assert_eq!(extract_passcode("https://zoom.us/j/123"), None);
        assert_eq!(extract_passcode("https://meet.google.com/abc"), None);
    }

    #[test]
    fn find_url_in_text_found() {
        assert_eq!(
            find_url_in_text("Join at https://meet.google.com/abc please"),
            Some("https://meet.google.com/abc".to_string())
        );
    }

    #[test]
    fn find_url_in_text_none() {
        assert_eq!(find_url_in_text("no url here"), None);
    }

    #[test]
    fn edit_calendar_event_url_rejects_empty() {
        let result = edit_calendar_event_url("   ", "event123", None);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("calendar event URL is empty")
        );
    }

    #[test]
    fn calendar_edit_url_constructs_eventedit_url() {
        let url = "https://www.google.com/calendar/event?eid=abc123";
        assert_eq!(
            calendar_edit_url(url, "event456", None),
            "https://calendar.google.com/calendar/r/eventedit/event456"
        );
    }

    #[test]
    fn calendar_edit_url_with_google_domain() {
        let url = "https://calendar.google.com/calendar/u/0/r/event?eid=abc123";
        assert_eq!(
            calendar_edit_url(url, "event456", Some("example.com")),
            "https://calendar.google.com/a/example.com/calendar/r/eventedit/event456"
        );
    }

    #[test]
    fn calendar_edit_url_keeps_non_google_url() {
        let url = "https://example.com/event/123";
        assert_eq!(calendar_edit_url(url, "event456", None), url.to_string());
    }
}
