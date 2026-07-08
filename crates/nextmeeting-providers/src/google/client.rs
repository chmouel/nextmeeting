//! Google Calendar API client.
//!
//! This module provides a low-level HTTP client for the Google Calendar API,
//! handling authentication, request building, and response parsing.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, trace, warn};

use crate::error::{ProviderError, ProviderResult};
use crate::raw_event::{
    RawAttendee, RawConferenceData, RawEntryPoint, RawEvent, RawEventTime, ResponseStatus,
};

/// Base URL for Google Calendar API v3.
const CALENDAR_API_BASE: &str = "https://www.googleapis.com/calendar/v3";

/// Google Calendar API client.
#[derive(Debug)]
pub struct GoogleCalendarClient {
    http_client: reqwest::Client,
    access_token: String,
}

impl GoogleCalendarClient {
    /// Creates a new Google Calendar client with the given access token.
    pub fn new(access_token: impl Into<String>, timeout: Duration) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("failed to create HTTP client");

        Self {
            http_client,
            access_token: access_token.into(),
        }
    }

    /// Updates the access token (after refresh).
    pub fn set_access_token(&mut self, token: impl Into<String>) {
        self.access_token = token.into();
    }

    /// Lists events from a calendar.
    ///
    /// # Arguments
    ///
    /// * `calendar_id` - The calendar identifier (e.g., "primary")
    /// * `time_min` - Lower bound for event start time
    /// * `time_max` - Upper bound for event start time
    /// * `max_results` - Maximum number of events to return
    /// * `single_events` - Whether to expand recurring events
    /// * `etag` - Optional ETag for conditional fetch
    /// * `default_reminder_minutes` - The calendar's default popup reminder
    ///   minutes (from `calendarList.entry.defaultReminders`), used to
    ///   resolve events whose `reminders.useDefault` is true and which have
    ///   no explicit popup overrides of their own.
    ///
    /// # Returns
    ///
    /// Returns a tuple of (events, new_etag, not_modified).
    #[allow(clippy::too_many_arguments)]
    pub async fn list_events(
        &self,
        calendar_id: &str,
        time_min: DateTime<Utc>,
        time_max: DateTime<Utc>,
        max_results: Option<usize>,
        single_events: bool,
        etag: Option<&str>,
        default_reminder_minutes: Option<&[u32]>,
    ) -> ProviderResult<(Vec<RawEvent>, Option<String>, bool)> {
        let mut all_events = Vec::new();
        let mut page_token: Option<String> = None;
        let mut response_etag: Option<String> = None;

        loop {
            let result = self
                .list_events_page(
                    calendar_id,
                    time_min,
                    time_max,
                    max_results,
                    single_events,
                    etag,
                    page_token.as_deref(),
                )
                .await?;

            // Check for not-modified response
            if result.not_modified {
                return Ok((Vec::new(), None, true));
            }

            // Store the ETag from first page
            if response_etag.is_none() {
                response_etag = result.etag.clone();
            }

            // Convert API events to RawEvents
            for event in result.items {
                if let Some(raw_event) =
                    self.convert_event(event, calendar_id, default_reminder_minutes)
                {
                    all_events.push(raw_event);
                }
            }

            // Check for more pages
            match result.next_page_token {
                Some(token) => page_token = Some(token),
                None => break,
            }

            // Respect max_results across pages
            if let Some(max) = max_results
                && all_events.len() >= max
            {
                all_events.truncate(max);
                break;
            }
        }

        debug!(
            "fetched {} events from calendar {}",
            all_events.len(),
            calendar_id
        );
        Ok((all_events, response_etag, false))
    }

    /// Fetches a single page of events.
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip(self), fields(calendar_id, status, elapsed_ms))]
    async fn list_events_page(
        &self,
        calendar_id: &str,
        time_min: DateTime<Utc>,
        time_max: DateTime<Utc>,
        max_results: Option<usize>,
        single_events: bool,
        etag: Option<&str>,
        page_token: Option<&str>,
    ) -> ProviderResult<EventListResponse> {
        use tracing::Span;

        if tracing::enabled!(tracing::Level::DEBUG) {
            debug!(
                calendar_id = %calendar_id,
                "Fetching events from Google Calendar API"
            );
        }

        let start = std::time::Instant::now();

        let url = format!(
            "{}/calendars/{}/events",
            CALENDAR_API_BASE,
            urlencoding::encode(calendar_id)
        );

        let mut request = self
            .http_client
            .get(&url)
            .bearer_auth(&self.access_token)
            .query(&[
                ("timeMin", time_min.to_rfc3339()),
                ("timeMax", time_max.to_rfc3339()),
                ("singleEvents", single_events.to_string()),
                ("orderBy", "startTime".to_string()),
            ]);

        if let Some(max) = max_results {
            request = request.query(&[("maxResults", max.to_string())]);
        }

        if let Some(token) = page_token {
            request = request.query(&[("pageToken", token.to_string())]);
        }

        if let Some(etag) = etag {
            request = request.header("If-None-Match", etag);
        }

        let response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                ProviderError::network("request timeout")
            } else if e.is_connect() {
                ProviderError::network(format!("connection failed: {}", e))
            } else {
                ProviderError::network(format!("request failed: {}", e))
            }
        })?;

        let status = response.status();
        let elapsed = start.elapsed();

        // Log API response at DEBUG level (level 3+)
        if tracing::enabled!(tracing::Level::DEBUG) {
            debug!(
                calendar_id = %calendar_id,
                status = %status,
                elapsed_ms = elapsed.as_millis(),
                "Google Calendar API response"
            );
            Span::current().record("status", status.as_u16());
            Span::current().record("elapsed_ms", elapsed.as_millis());
        }

        // Log response headers at TRACE level (level 5)
        if tracing::enabled!(tracing::Level::TRACE) {
            trace!(
                headers = ?response.headers(),
                "API response headers"
            );
        }

        // Handle 304 Not Modified
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(EventListResponse {
                items: Vec::new(),
                next_page_token: None,
                etag: None,
                not_modified: true,
            });
        }

        // Handle rate limiting
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            return Err(ProviderError::rate_limited(format!(
                "rate limit exceeded{}",
                retry_after
                    .map(|s| format!(", retry after {} seconds", s))
                    .unwrap_or_default()
            )));
        }

        // Handle authentication errors
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ProviderError::authentication(
                "access token expired or invalid",
            ));
        }

        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::authorization("access denied to calendar"));
        }

        // Handle other errors
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::server(format!(
                "API error ({}): {}",
                status, body
            )));
        }

        // Extract ETag from response
        let etag = response
            .headers()
            .get("ETag")
            .and_then(|v| v.to_str().ok())
            .map(String::from);

        // Parse response
        let body = response
            .text()
            .await
            .map_err(|e| ProviderError::network(format!("failed to read response: {}", e)))?;

        let mut list_response: EventListResponse = serde_json::from_str(&body).map_err(|e| {
            ProviderError::invalid_response(format!("failed to parse response: {}", e))
        })?;

        list_response.etag = etag;
        list_response.not_modified = false;

        Ok(list_response)
    }

    /// Lists available calendars.
    pub async fn list_calendars(&self) -> ProviderResult<Vec<CalendarListEntry>> {
        let url = format!("{}/users/me/calendarList", CALENDAR_API_BASE);

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(|e| ProviderError::network(format!("request failed: {}", e)))?;

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ProviderError::authentication(
                "access token expired or invalid",
            ));
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::server(format!(
                "API error ({}): {}",
                status, body
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|e| ProviderError::network(format!("failed to read response: {}", e)))?;

        let list: CalendarListResponse = serde_json::from_str(&body).map_err(|e| {
            ProviderError::invalid_response(format!("failed to parse response: {}", e))
        })?;

        Ok(list.items)
    }

    /// Declines an event occurrence for the current user.
    pub async fn decline_event(&self, calendar_id: &str, event_id: &str) -> ProviderResult<()> {
        let url = self.event_url(calendar_id, event_id);
        let body = json!({
            "attendees": [
                {
                    "self": true,
                    "responseStatus": "declined"
                }
            ]
        });
        let body =
            serde_json::to_vec(&body).map_err(|e| ProviderError::bad_request(e.to_string()))?;

        let response = self
            .http_client
            .patch(url)
            .bearer_auth(&self.access_token)
            .query(&[("sendUpdates", "none")])
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::network("request timeout")
                } else if e.is_connect() {
                    ProviderError::network(format!("connection failed: {}", e))
                } else {
                    ProviderError::network(format!("request failed: {}", e))
                }
            })?;

        Self::handle_empty_success_response(response, "decline event")
    }

    /// Deletes an event occurrence.
    pub async fn delete_event(&self, calendar_id: &str, event_id: &str) -> ProviderResult<()> {
        let url = self.event_url(calendar_id, event_id);
        let response = self
            .http_client
            .delete(url)
            .bearer_auth(&self.access_token)
            .query(&[("sendUpdates", "none")])
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::network("request timeout")
                } else if e.is_connect() {
                    ProviderError::network(format!("connection failed: {}", e))
                } else {
                    ProviderError::network(format!("request failed: {}", e))
                }
            })?;

        Self::handle_empty_success_response(response, "delete event")
    }

    fn event_url(&self, calendar_id: &str, event_id: &str) -> String {
        format!(
            "{}/calendars/{}/events/{}",
            CALENDAR_API_BASE,
            urlencoding::encode(calendar_id),
            urlencoding::encode(event_id)
        )
    }

    fn handle_empty_success_response(
        response: reqwest::Response,
        action: &str,
    ) -> ProviderResult<()> {
        let status = response.status();

        if status.is_success() {
            return Ok(());
        }

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ProviderError::authentication(
                "access token expired or invalid",
            ));
        }

        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::authorization("access denied to calendar"));
        }

        if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::GONE {
            return Err(ProviderError::not_found("event not found"));
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::rate_limited("rate limit exceeded"));
        }

        if status.is_server_error() {
            return Err(ProviderError::server(format!(
                "API error during {} ({})",
                action, status
            )));
        }

        Err(ProviderError::bad_request(format!(
            "request failed during {} ({})",
            action, status
        )))
    }

    /// Converts a Google Calendar API event to a RawEvent.
    ///
    /// `default_reminder_minutes` is the calendar's default popup reminder
    /// minutes, used to resolve events whose `reminders.useDefault` is true
    /// and which have no explicit popup overrides of their own.
    fn convert_event(
        &self,
        event: ApiEvent,
        calendar_id: &str,
        default_reminder_minutes: Option<&[u32]>,
    ) -> Option<RawEvent> {
        // Skip cancelled events
        if event.status.as_deref() == Some("cancelled") {
            return None;
        }

        let id = event.id?;
        let summary = event.summary.unwrap_or_default();

        // Parse start time
        let start = match (event.start.date_time, event.start.date) {
            (Some(dt), _) => {
                let parsed = DateTime::parse_from_rfc3339(&dt)
                    .map_err(|e| warn!("failed to parse start time: {}", e))
                    .ok()?;
                RawEventTime::DateTime(parsed.with_timezone(&Utc))
            }
            (None, Some(date)) => {
                let parsed = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                    .map_err(|e| warn!("failed to parse start date: {}", e))
                    .ok()?;
                RawEventTime::Date(parsed)
            }
            (None, None) => {
                warn!("event {} has no start time", id);
                return None;
            }
        };

        // Parse end time
        let end = match (event.end.date_time, event.end.date) {
            (Some(dt), _) => {
                let parsed = DateTime::parse_from_rfc3339(&dt)
                    .map_err(|e| warn!("failed to parse end time: {}", e))
                    .ok()?;
                RawEventTime::DateTime(parsed.with_timezone(&Utc))
            }
            (None, Some(date)) => {
                let parsed = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                    .map_err(|e| warn!("failed to parse end date: {}", e))
                    .ok()?;
                RawEventTime::Date(parsed)
            }
            (None, None) => {
                warn!("event {} has no end time", id);
                return None;
            }
        };

        // Convert attendees
        let attendees = event
            .attendees
            .unwrap_or_default()
            .into_iter()
            .filter_map(|a| {
                let email = a.email?;
                let status = match a.response_status.as_deref() {
                    Some("accepted") => ResponseStatus::Accepted,
                    Some("declined") => ResponseStatus::Declined,
                    Some("tentative") => ResponseStatus::Tentative,
                    Some("needsAction") => ResponseStatus::NeedsAction,
                    _ => ResponseStatus::NeedsAction,
                };
                Some(RawAttendee {
                    email,
                    display_name: a.display_name,
                    is_self: a.is_self.unwrap_or(false),
                    organizer: a.organizer.unwrap_or(false),
                    resource: false,
                    optional: false,
                    response_status: status,
                })
            })
            .collect();

        // Convert conference data
        let conference_data = event.conference_data.map(|cd| {
            let entry_points = cd
                .entry_points
                .unwrap_or_default()
                .into_iter()
                .map(|ep| RawEntryPoint {
                    entry_point_type: ep.entry_point_type,
                    uri: ep.uri,
                    label: ep.label,
                    meeting_code: ep.meeting_code,
                    passcode: ep.passcode,
                    pin: ep.password, // Map password to pin
                })
                .collect();

            RawConferenceData {
                conference_type: None,
                solution_name: cd.conference_solution.map(|cs| cs.name),
                entry_points,
            }
        });

        let mut raw_event = RawEvent::new(id, start, end, calendar_id)
            .with_status(event.status.unwrap_or_default());

        raw_event.summary = Some(summary);
        raw_event.description = event.description;
        raw_event.location = event.location;
        raw_event.timezone = event.start.time_zone;
        raw_event.html_link = event.html_link;
        raw_event.is_recurring_instance = event.recurring_event_id.is_some();
        raw_event.recurring_event_id = event.recurring_event_id;
        raw_event.attendees = attendees;
        raw_event.conference_data = conference_data;
        raw_event.etag = event.etag;
        raw_event.reminder_minutes =
            resolve_reminder_minutes(event.reminders, default_reminder_minutes);

        Some(raw_event)
    }
}

/// Resolves the effective popup reminder minutes for an event.
///
/// - Explicit popup overrides on the event always take priority.
/// - If the event has `useDefault: true` and no popup overrides, the
///   calendar's default popup reminders are used instead (if any).
/// - Otherwise (e.g. reminders explicitly disabled, or nothing resolvable),
///   returns `None` so callers fall back to their own default notification
///   configuration.
fn resolve_reminder_minutes(
    reminders: Option<ApiReminders>,
    default_reminder_minutes: Option<&[u32]>,
) -> Option<Vec<u32>> {
    let reminders = reminders?;

    let mut popup_minutes: Vec<u32> = reminders
        .overrides
        .unwrap_or_default()
        .into_iter()
        .filter(|o| o.method == "popup")
        .map(|o| o.minutes)
        .collect();

    if !popup_minutes.is_empty() {
        popup_minutes.sort_unstable();
        popup_minutes.dedup();
        return Some(popup_minutes);
    }

    if reminders.use_default
        && let Some(defaults) = default_reminder_minutes
        && !defaults.is_empty()
    {
        let mut minutes = defaults.to_vec();
        minutes.sort_unstable();
        minutes.dedup();
        return Some(minutes);
    }

    None
}

/// Response from the events.list endpoint.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventListResponse {
    #[serde(default)]
    items: Vec<ApiEvent>,
    next_page_token: Option<String>,
    #[serde(skip)]
    etag: Option<String>,
    #[serde(skip)]
    not_modified: bool,
}

/// A single event from the Google Calendar API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiEvent {
    id: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    location: Option<String>,
    start: ApiEventTime,
    end: ApiEventTime,
    html_link: Option<String>,
    status: Option<String>,
    recurring_event_id: Option<String>,
    #[allow(dead_code)]
    recurrence: Option<Vec<String>>,
    attendees: Option<Vec<ApiAttendee>>,
    conference_data: Option<ApiConferenceData>,
    etag: Option<String>,
    reminders: Option<ApiReminders>,
}

/// Event time from the API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiEventTime {
    date: Option<String>,
    date_time: Option<String>,
    time_zone: Option<String>,
}

/// Attendee from the API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiAttendee {
    email: Option<String>,
    display_name: Option<String>,
    #[serde(rename = "self")]
    is_self: Option<bool>,
    organizer: Option<bool>,
    response_status: Option<String>,
}

/// Conference data from the API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiConferenceData {
    conference_solution: Option<ApiConferenceSolution>,
    entry_points: Option<Vec<ApiEntryPoint>>,
}

/// Conference solution from the API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiConferenceSolution {
    name: String,
}

/// Entry point from the API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiEntryPoint {
    entry_point_type: String,
    uri: Option<String>,
    label: Option<String>,
    meeting_code: Option<String>,
    passcode: Option<String>,
    password: Option<String>,
}

/// Reminders settings on an event, as returned by the Calendar API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiReminders {
    /// Whether the event uses the calendar's default reminders. Google
    /// omits this field on some events; it defaults to `true` in that case.
    #[serde(default = "default_true")]
    use_default: bool,
    /// Explicit per-event reminder overrides (replaces calendar defaults
    /// entirely when present).
    overrides: Option<Vec<ApiReminderOverride>>,
}

fn default_true() -> bool {
    true
}

/// A single reminder override (event- or calendar-level).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiReminderOverride {
    method: String,
    minutes: u32,
}

/// Response from the calendarList endpoint.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarListResponse {
    #[serde(default)]
    items: Vec<CalendarListEntry>,
}

/// A calendar from the calendar list.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarListEntry {
    /// The calendar ID.
    pub id: String,
    /// The calendar summary (name).
    pub summary: String,
    /// The calendar description.
    pub description: Option<String>,
    /// Whether this is the primary calendar.
    #[serde(default)]
    pub primary: bool,
    /// The calendar timezone.
    pub time_zone: Option<String>,
    /// Background color.
    pub background_color: Option<String>,
    /// Foreground color.
    pub foreground_color: Option<String>,
    /// The calendar's default reminders (used when an event's `reminders.useDefault` is true).
    #[serde(default)]
    default_reminders: Option<Vec<ApiReminderOverride>>,
}

impl CalendarListEntry {
    /// Returns the calendar's default popup reminder minutes (sorted, deduped).
    pub fn default_popup_reminder_minutes(&self) -> Vec<u32> {
        let mut minutes: Vec<u32> = self
            .default_reminders
            .clone()
            .unwrap_or_default()
            .into_iter()
            .filter(|o| o.method == "popup")
            .map(|o| o.minutes)
            .collect();
        minutes.sort_unstable();
        minutes.dedup();
        minutes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_event_list_response() {
        let json = r#"{
            "items": [
                {
                    "id": "event1",
                    "summary": "Test Meeting",
                    "start": {
                        "dateTime": "2024-03-15T10:00:00Z"
                    },
                    "end": {
                        "dateTime": "2024-03-15T11:00:00Z"
                    },
                    "status": "confirmed"
                }
            ]
        }"#;

        let response: EventListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].summary, Some("Test Meeting".to_string()));
    }

    #[test]
    fn parse_all_day_event() {
        let json = r#"{
            "id": "event1",
            "summary": "All Day Event",
            "start": {
                "date": "2024-03-15"
            },
            "end": {
                "date": "2024-03-16"
            }
        }"#;

        let event: ApiEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.start.date, Some("2024-03-15".to_string()));
        assert!(event.start.date_time.is_none());
    }

    #[test]
    fn parse_event_with_conference() {
        let json = r#"{
            "id": "event1",
            "summary": "Meeting with Zoom",
            "start": {
                "dateTime": "2024-03-15T10:00:00Z"
            },
            "end": {
                "dateTime": "2024-03-15T11:00:00Z"
            },
            "conferenceData": {
                "conferenceSolution": {
                    "name": "Zoom Meeting"
                },
                "entryPoints": [
                    {
                        "entryPointType": "video",
                        "uri": "https://zoom.us/j/123456789"
                    }
                ]
            }
        }"#;

        let event: ApiEvent = serde_json::from_str(json).unwrap();
        assert!(event.conference_data.is_some());
        let cd = event.conference_data.unwrap();
        assert_eq!(cd.conference_solution.unwrap().name, "Zoom Meeting");
    }

    #[test]
    fn parse_calendar_list() {
        let json = r#"{
            "items": [
                {
                    "id": "primary",
                    "summary": "My Calendar",
                    "primary": true,
                    "timeZone": "America/New_York"
                },
                {
                    "id": "work@example.com",
                    "summary": "Work Calendar",
                    "primary": false
                }
            ]
        }"#;

        let response: CalendarListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.items.len(), 2);
        assert!(response.items[0].primary);
        assert!(!response.items[1].primary);
    }

    #[test]
    fn event_url_escapes_calendar_and_event_ids() {
        let client = GoogleCalendarClient::new("token", Duration::from_secs(1));
        let url = client.event_url("work@example.com", "abc 123");
        assert!(url.contains("work%40example.com"));
        assert!(url.contains("abc%20123"));
    }

    fn sample_event_json(reminders: &str) -> String {
        format!(
            r#"{{
                "id": "event1",
                "summary": "Test Meeting",
                "start": {{ "dateTime": "2024-03-15T10:00:00Z" }},
                "end": {{ "dateTime": "2024-03-15T11:00:00Z" }},
                "status": "confirmed",
                "reminders": {reminders}
            }}"#
        )
    }

    #[test]
    fn parse_event_with_popup_override() {
        let json = sample_event_json(
            r#"{ "useDefault": false, "overrides": [{ "method": "popup", "minutes": 10 }] }"#,
        );
        let event: ApiEvent = serde_json::from_str(&json).unwrap();
        let reminders = event.reminders.unwrap();
        assert!(!reminders.use_default);
        assert_eq!(reminders.overrides.unwrap()[0].minutes, 10);
    }

    #[test]
    fn resolve_reminder_minutes_prefers_explicit_popup_overrides() {
        let reminders = ApiReminders {
            use_default: true,
            overrides: Some(vec![
                ApiReminderOverride {
                    method: "popup".to_string(),
                    minutes: 10,
                },
                ApiReminderOverride {
                    method: "email".to_string(),
                    minutes: 30,
                },
            ]),
        };

        let resolved = resolve_reminder_minutes(Some(reminders), Some(&[15, 5, 1]));
        assert_eq!(resolved, Some(vec![10]));
    }

    #[test]
    fn resolve_reminder_minutes_falls_back_to_calendar_defaults() {
        let reminders = ApiReminders {
            use_default: true,
            overrides: None,
        };

        let resolved = resolve_reminder_minutes(Some(reminders), Some(&[20, 10]));
        assert_eq!(resolved, Some(vec![10, 20]));
    }

    #[test]
    fn resolve_reminder_minutes_disabled_falls_back_to_none() {
        // useDefault: false with no overrides means reminders were
        // explicitly disabled for this event; callers fall back to their
        // own default configuration.
        let reminders = ApiReminders {
            use_default: false,
            overrides: None,
        };

        let resolved = resolve_reminder_minutes(Some(reminders), Some(&[20, 10]));
        assert_eq!(resolved, None);
    }

    #[test]
    fn resolve_reminder_minutes_missing_field_returns_none() {
        // No `reminders` field at all in the API response.
        let resolved = resolve_reminder_minutes(None, Some(&[20, 10]));
        assert_eq!(resolved, None);
    }

    #[test]
    fn resolve_reminder_minutes_use_default_without_calendar_defaults() {
        let reminders = ApiReminders {
            use_default: true,
            overrides: None,
        };

        let resolved = resolve_reminder_minutes(Some(reminders), None);
        assert_eq!(resolved, None);
    }

    #[test]
    fn convert_event_resolves_reminder_minutes_end_to_end() {
        let client = GoogleCalendarClient::new("token", Duration::from_secs(1));
        let json = sample_event_json(
            r#"{ "useDefault": false, "overrides": [{ "method": "popup", "minutes": 10 }, { "method": "popup", "minutes": 2 }] }"#,
        );
        let api_event: ApiEvent = serde_json::from_str(&json).unwrap();

        let raw = client
            .convert_event(api_event, "primary", Some(&[15, 5, 1]))
            .unwrap();

        assert_eq!(raw.reminder_minutes, Some(vec![2, 10]));
    }

    #[test]
    fn parse_calendar_list_default_reminders() {
        let json = r#"{
            "items": [
                {
                    "id": "primary",
                    "summary": "My Calendar",
                    "primary": true,
                    "defaultReminders": [
                        { "method": "popup", "minutes": 30 },
                        { "method": "email", "minutes": 60 }
                    ]
                }
            ]
        }"#;

        let response: CalendarListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.items[0].default_popup_reminder_minutes(), vec![30]);
    }
}
