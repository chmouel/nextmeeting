//! Meeting actions: open URLs, copy to clipboard, snooze.

use nextmeeting_core::MeetingView;
use nextmeeting_protocol::{Request, Response};
use tracing::{debug, info};

use crate::error::{ClientError, ClientResult};
use crate::socket::SocketClient;

/// Opens the meeting URL in the default browser.
pub fn open_meeting_url(meetings: &[MeetingView]) -> ClientResult<()> {
    let meeting = first_meeting_with_link(meetings)?;
    let link = meeting.primary_link.as_ref().ok_or_else(|| {
        ClientError::Action("next meeting has no meeting URL".into())
    })?;

    info!(url = %link.url, "opening meeting URL");
    open::that(&link.url).map_err(|e| {
        ClientError::Action(format!("failed to open URL: {}", e))
    })?;

    Ok(())
}

/// Copies the meeting URL to the clipboard.
pub fn copy_meeting_url(meetings: &[MeetingView]) -> ClientResult<()> {
    let meeting = first_meeting_with_link(meetings)?;
    let link = meeting.primary_link.as_ref().ok_or_else(|| {
        ClientError::Action("next meeting has no meeting URL".into())
    })?;

    info!(url = %link.url, "copying meeting URL to clipboard");

    let mut clipboard = arboard::Clipboard::new().map_err(|e| {
        ClientError::Action(format!("failed to access clipboard: {}", e))
    })?;

    clipboard.set_text(&link.url).map_err(|e| {
        ClientError::Action(format!("failed to copy to clipboard: {}", e))
    })?;

    println!("{}", link.url);
    Ok(())
}

/// Opens the calendar day view in the default browser.
pub fn open_calendar_day(meetings: &[MeetingView], google_domain: Option<&str>) -> ClientResult<()> {
    // Try to find a calendar URL from meetings, or construct Google Calendar URL
    if let Some(meeting) = meetings.first() {
        if let Some(ref url) = meeting.calendar_url {
            debug!(url = %url, "opening calendar URL from meeting");
            open::that(url).map_err(|e| {
                ClientError::Action(format!("failed to open URL: {}", e))
            })?;
            return Ok(());
        }
    }

    // Fallback: open Google Calendar
    let base = if let Some(domain) = google_domain {
        format!("https://calendar.google.com/a/{}", domain)
    } else {
        "https://calendar.google.com".to_string()
    };

    info!(url = %base, "opening Google Calendar");
    open::that(&base).map_err(|e| {
        ClientError::Action(format!("failed to open URL: {}", e))
    })?;

    Ok(())
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

/// Returns the first non-all-day meeting, or the first meeting.
fn first_meeting_with_link(meetings: &[MeetingView]) -> ClientResult<&MeetingView> {
    // Prefer non-all-day meeting with a link
    if let Some(m) = meetings.iter().find(|m| !m.is_all_day && m.primary_link.is_some()) {
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
