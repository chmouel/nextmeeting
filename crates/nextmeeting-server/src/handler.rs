//! Request/response dispatch handler.
//!
//! This module provides the request handler that routes incoming requests
//! to the appropriate logic and produces responses.

use std::sync::Arc;
use std::{future::Future, pin::Pin};

use chrono::{DateTime, NaiveTime, Utc};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use nextmeeting_core::{MeetingView, ResponseStatus};
use nextmeeting_protocol::{
    ErrorCode, ErrorResponse, EventMutationAction, MeetingsFilter, ProviderStatus, Request,
    Response, StatusInfo,
};

use crate::error::{ServerError, ServerResult};
use crate::notify::NotifyEngine;
use crate::scheduler::SchedulerHandle;
use crate::socket::Connection;

/// Server state shared across all connections.
#[derive(Debug)]
pub struct ServerState {
    /// Server start time.
    start_time: DateTime<Utc>,
    /// Last successful sync time.
    last_sync: Option<DateTime<Utc>>,
    /// Current cached meetings.
    meetings: Vec<MeetingView>,
    /// Provider status.
    providers: Vec<ProviderStatus>,
    /// When notifications are snoozed until.
    snoozed_until: Option<DateTime<Utc>>,
    /// Whether shutdown has been requested.
    shutdown_requested: bool,
    /// Scheduler handle for triggering refreshes.
    scheduler_handle: Option<SchedulerHandle>,
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerState {
    /// Creates a new server state.
    pub fn new() -> Self {
        Self {
            start_time: Utc::now(),
            last_sync: None,
            meetings: Vec::new(),
            providers: Vec::new(),
            snoozed_until: None,
            shutdown_requested: false,
            scheduler_handle: None,
        }
    }

    /// Returns the server uptime in seconds.
    pub fn uptime_seconds(&self) -> u64 {
        let duration = Utc::now() - self.start_time;
        duration.num_seconds().max(0) as u64
    }

    /// Returns the status info.
    pub fn status_info(&self) -> StatusInfo {
        StatusInfo {
            uptime_seconds: self.uptime_seconds(),
            last_sync: self.last_sync,
            providers: self.providers.clone(),
            snoozed_until: self.snoozed_until,
        }
    }

    /// Updates the cached meetings.
    pub fn set_meetings(&mut self, meetings: Vec<MeetingView>) {
        self.meetings = meetings;
        self.last_sync = Some(Utc::now());
    }

    /// Returns the cached meetings, optionally filtered.
    pub fn get_meetings(&self, filter: Option<&MeetingsFilter>) -> Vec<MeetingView> {
        let now = chrono::Local::now();
        let mut meetings: Vec<_> = self.meetings.to_vec();

        // Filter out non-all-day meetings that have already ended
        meetings.retain(|m| m.is_all_day || m.end_local > now);

        if let Some(filter) = filter {
            // Apply skip_all_day filter
            if filter.skip_all_day {
                meetings.retain(|m| !m.is_all_day);
            }

            // Apply include_titles filter (retain if ANY pattern matches)
            if !filter.include_titles.is_empty() {
                meetings.retain(|m| {
                    let title_lower = m.title.to_lowercase();
                    filter
                        .include_titles
                        .iter()
                        .any(|p| title_lower.contains(&p.to_lowercase()))
                });
            }

            // Apply exclude_titles filter (remove if ANY pattern matches)
            if !filter.exclude_titles.is_empty() {
                meetings.retain(|m| {
                    let title_lower = m.title.to_lowercase();
                    !filter
                        .exclude_titles
                        .iter()
                        .any(|p| title_lower.contains(&p.to_lowercase()))
                });
            }

            // Apply today_only filter
            if filter.today_only {
                let today = chrono::Local::now().date_naive();
                meetings.retain(|m| m.start_local.date_naive() == today);
            }

            // Apply response status filters
            if filter.skip_declined {
                meetings.retain(|m| m.user_response_status != ResponseStatus::Declined);
            }

            if filter.skip_tentative {
                meetings.retain(|m| m.user_response_status != ResponseStatus::Tentative);
            }

            if filter.skip_pending {
                meetings.retain(|m| m.user_response_status != ResponseStatus::NeedsAction);
            }

            // Apply solo event filter
            if filter.skip_without_guests {
                meetings.retain(|m| m.other_attendee_count > 0);
            }

            // Apply include_calendars filter (retain if ANY match)
            if !filter.include_calendars.is_empty() {
                meetings.retain(|m| {
                    let cal_id_lower = m.calendar_id.to_lowercase();
                    filter
                        .include_calendars
                        .iter()
                        .any(|p| cal_id_lower.contains(&p.to_lowercase()))
                });
            }

            // Apply exclude_calendars filter (remove if ANY match)
            if !filter.exclude_calendars.is_empty() {
                meetings.retain(|m| {
                    let cal_id_lower = m.calendar_id.to_lowercase();
                    !filter
                        .exclude_calendars
                        .iter()
                        .any(|p| cal_id_lower.contains(&p.to_lowercase()))
                });
            }

            // Apply within_minutes filter (retain if starts within N minutes, skip all-day)
            if let Some(within) = filter.within_minutes {
                let now = chrono::Local::now();
                meetings.retain(|m| {
                    if m.is_all_day {
                        return false;
                    }
                    let mins = m.minutes_until_start(now);
                    mins >= 0 && mins <= within as i64
                });
            }

            // Apply only_with_link filter
            if filter.only_with_link {
                meetings.retain(|m| m.primary_link.is_some());
            }

            // Apply work_hours filter
            if let Some(ref spec) = filter.work_hours
                && let Some((start_time, end_time)) = parse_work_hours(spec)
            {
                meetings.retain(|m| {
                    if m.is_all_day {
                        return true; // pass all-day events through
                    }
                    let event_time = m.start_local.time();
                    event_time >= start_time && event_time <= end_time
                });
            }

            // Apply privacy filter (mutate titles)
            if filter.privacy {
                let privacy_title = filter
                    .privacy_title
                    .as_deref()
                    .unwrap_or("Busy")
                    .to_string();
                for m in &mut meetings {
                    m.title = privacy_title.clone();
                }
            }

            // Apply limit
            if let Some(limit) = filter.limit {
                meetings.truncate(limit);
            }
        }

        meetings
    }

    /// Snoozes notifications for the given number of minutes.
    pub fn snooze(&mut self, minutes: u32) {
        let until = Utc::now() + chrono::Duration::minutes(minutes as i64);
        self.snoozed_until = Some(until);
        info!(until = %until, minutes = minutes, "Notifications snoozed");
    }

    /// Clears the snooze.
    pub fn clear_snooze(&mut self) {
        self.snoozed_until = None;
    }

    /// Checks if notifications are currently snoozed.
    pub fn is_snoozed(&self) -> bool {
        if let Some(until) = self.snoozed_until {
            Utc::now() < until
        } else {
            false
        }
    }

    /// Requests a shutdown.
    pub fn request_shutdown(&mut self) {
        self.shutdown_requested = true;
    }

    /// Returns true if shutdown has been requested.
    pub fn shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    /// Sets the scheduler handle for triggering refreshes.
    pub fn set_scheduler_handle(&mut self, handle: SchedulerHandle) {
        self.scheduler_handle = Some(handle);
    }

    /// Returns a reference to the scheduler handle, if set.
    pub fn scheduler_handle(&self) -> Option<&SchedulerHandle> {
        self.scheduler_handle.as_ref()
    }

    /// Updates provider status.
    pub fn set_provider_status(&mut self, status: ProviderStatus) {
        // Update existing or add new
        if let Some(existing) = self.providers.iter_mut().find(|p| p.name == status.name) {
            *existing = status;
        } else {
            self.providers.push(status);
        }
    }
}

/// Parses a work hours specification (format: "HH:MM-HH:MM") into start and end times.
pub fn parse_work_hours(spec: &str) -> Option<(NaiveTime, NaiveTime)> {
    let parts: Vec<&str> = spec.split('-').collect();
    if parts.len() != 2 {
        return None;
    }
    let start = NaiveTime::parse_from_str(parts[0].trim(), "%H:%M").ok()?;
    let end = NaiveTime::parse_from_str(parts[1].trim(), "%H:%M").ok()?;
    Some((start, end))
}

/// Shared server state wrapped in an Arc<RwLock>.
pub type SharedState = Arc<RwLock<ServerState>>;

/// Creates a new shared state.
pub fn new_shared_state() -> SharedState {
    Arc::new(RwLock::new(ServerState::new()))
}

/// Event mutation request shape for callback-based mutators.
#[derive(Debug, Clone)]
pub struct EventMutationRequest {
    /// Provider name (e.g. google:work).
    pub provider_name: String,
    /// Provider calendar identifier.
    pub calendar_id: String,
    /// Provider event identifier.
    pub event_id: String,
    /// Mutation action.
    pub action: EventMutationAction,
}

/// Future returned by event mutator callbacks.
pub type EventMutationFuture = Pin<Box<dyn Future<Output = Result<(), ErrorResponse>> + Send>>;

/// Callback used by the server handler to perform event mutations.
pub type EventMutator = Arc<dyn Fn(EventMutationRequest) -> EventMutationFuture + Send + Sync>;

/// Request handler that processes incoming requests and produces responses.
pub struct RequestHandler {
    state: SharedState,
    event_mutator: Option<EventMutator>,
    notify_engine: Option<Arc<NotifyEngine>>,
}

impl RequestHandler {
    /// Creates a new request handler with the given state.
    pub fn new(state: SharedState) -> Self {
        Self {
            state,
            event_mutator: None,
            notify_engine: None,
        }
    }

    /// Creates a new request handler with an event mutator callback.
    pub fn with_event_mutator(state: SharedState, event_mutator: EventMutator) -> Self {
        Self {
            state,
            event_mutator: Some(event_mutator),
            notify_engine: None,
        }
    }

    /// Creates a new request handler with an event mutator and notify engine.
    pub fn with_event_mutator_and_notify(
        state: SharedState,
        event_mutator: EventMutator,
        notify_engine: Arc<NotifyEngine>,
    ) -> Self {
        Self {
            state,
            event_mutator: Some(event_mutator),
            notify_engine: Some(notify_engine),
        }
    }

    /// Handles a single request and returns the response.
    #[tracing::instrument(skip(self), fields(request_type, duration_ms))]
    pub async fn handle(&self, request: &Request) -> Response {
        use tracing::Span;

        let start = std::time::Instant::now();
        let request_type = format!("{:?}", request);
        Span::current().record("request_type", &request_type);

        let response = match request {
            Request::Ping => {
                debug!("Handling Ping request");
                Response::Pong
            }
            Request::Status => {
                debug!("Handling Status request");
                let state = self.state.read().await;
                Response::status(state.status_info())
            }
            Request::GetMeetings { filter } => {
                debug!(?filter, "Handling GetMeetings request");
                let state = self.state.read().await;
                let meetings = state.get_meetings(filter.as_ref());
                debug!(meeting_count = meetings.len(), "Returning meetings");
                Response::meetings(meetings)
            }
            Request::Snooze { minutes } => {
                debug!(minutes = *minutes, "Handling Snooze request");
                {
                    let mut state = self.state.write().await;
                    if *minutes == 0 {
                        state.clear_snooze();
                    } else {
                        state.snooze(*minutes);
                    }
                }
                // Update notify engine outside the ServerState lock to avoid
                // lock ordering issues (engine acquires NotifyState write lock)
                if let Some(ref engine) = self.notify_engine {
                    if *minutes == 0 {
                        engine.clear_snooze().await;
                    } else {
                        engine.snooze(*minutes).await;
                    }
                }
                Response::Ok
            }
            Request::MutateEvent {
                provider_name,
                calendar_id,
                event_id,
                action,
            } => {
                debug!(
                    provider = %provider_name,
                    calendar_id = %calendar_id,
                    event_id = %event_id,
                    action = ?action,
                    "Handling MutateEvent request"
                );
                if let Some(mutator) = &self.event_mutator {
                    let req = EventMutationRequest {
                        provider_name: provider_name.clone(),
                        calendar_id: calendar_id.clone(),
                        event_id: event_id.clone(),
                        action: *action,
                    };
                    match mutator(req).await {
                        Ok(()) => Response::Ok,
                        Err(error) => Response::from_error(error),
                    }
                } else {
                    Response::error(
                        ErrorCode::ProviderError,
                        "event mutation is not configured on this server",
                    )
                }
            }
            Request::Refresh { force } => {
                debug!(force = *force, "Handling Refresh request");
                let state = self.state.read().await;
                if let Some(handle) = state.scheduler_handle() {
                    let handle = handle.clone();
                    drop(state);
                    if let Err(e) = handle.refresh(*force).await {
                        warn!(error = %e, "Failed to send refresh command to scheduler");
                        Response::error(
                            ErrorCode::InternalError,
                            format!("failed to trigger refresh: {}", e),
                        )
                    } else {
                        Response::Ok
                    }
                } else {
                    drop(state);
                    debug!("No scheduler handle available, refresh is a no-op");
                    Response::Ok
                }
            }
            Request::Shutdown => {
                info!("Handling Shutdown request");
                let mut state = self.state.write().await;
                state.request_shutdown();
                Response::Ok
            }
        };

        // Record timing metrics at DEBUG level (level 4+)
        let duration = start.elapsed();
        if tracing::enabled!(tracing::Level::DEBUG) {
            Span::current().record("duration_ms", duration.as_millis());
            debug!(
                request_type = %request_type,
                duration_ms = duration.as_millis(),
                "Request handled"
            );
        }

        response
    }

    /// Handles a connection, processing all requests until the connection closes.
    pub async fn handle_connection(&self, mut conn: Connection) -> ServerResult<()> {
        loop {
            match conn.read_request().await {
                Ok(Some(envelope)) => {
                    let response = self.handle(&envelope.payload).await;
                    conn.respond(&envelope.request_id, response).await?;

                    // Check if shutdown was requested
                    if self.state.read().await.shutdown_requested() {
                        return Err(ServerError::Shutdown);
                    }
                }
                Ok(None) => {
                    // Client disconnected cleanly
                    debug!("Client disconnected");
                    return Ok(());
                }
                Err(e) => {
                    warn!(error = %e, "Error reading request");
                    return Err(e);
                }
            }
        }
    }
}

/// Creates a connection handler function for use with SocketServer::run.
///
/// This returns a closure that can be passed to `SocketServer::run` or
/// `SocketServer::run_until_shutdown`.
pub fn make_connection_handler(
    state: SharedState,
) -> impl Fn(Connection) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
+ Send
+ Sync
+ 'static {
    move |conn| {
        let handler = RequestHandler::new(state.clone());
        Box::pin(async move {
            if let Err(e) = handler.handle_connection(conn).await
                && !matches!(e, ServerError::Shutdown)
            {
                warn!(error = %e, "Connection handler error");
            }
        })
    }
}

/// Creates a connection handler with event mutation support.
pub fn make_connection_handler_with_mutator(
    state: SharedState,
    event_mutator: EventMutator,
) -> impl Fn(Connection) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
+ Send
+ Sync
+ 'static {
    move |conn| {
        let handler = RequestHandler::with_event_mutator(state.clone(), event_mutator.clone());
        Box::pin(async move {
            if let Err(e) = handler.handle_connection(conn).await
                && !matches!(e, ServerError::Shutdown)
            {
                warn!(error = %e, "Connection handler error");
            }
        })
    }
}

/// Creates a connection handler with event mutation and notification engine support.
pub fn make_connection_handler_with_mutator_and_notify(
    state: SharedState,
    event_mutator: EventMutator,
    notify_engine: Arc<NotifyEngine>,
) -> impl Fn(Connection) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
+ Send
+ Sync
+ 'static {
    move |conn| {
        let handler = RequestHandler::with_event_mutator_and_notify(
            state.clone(),
            event_mutator.clone(),
            notify_engine.clone(),
        );
        Box::pin(async move {
            if let Err(e) = handler.handle_connection(conn).await
                && !matches!(e, ServerError::Shutdown)
            {
                warn!(error = %e, "Connection handler error");
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    #[test]
    fn server_state_uptime() {
        let state = ServerState::new();
        assert!(state.uptime_seconds() < 2);
    }

    #[test]
    fn server_state_snooze() {
        let mut state = ServerState::new();
        assert!(!state.is_snoozed());

        state.snooze(30);
        assert!(state.is_snoozed());

        state.clear_snooze();
        assert!(!state.is_snoozed());
    }

    #[test]
    fn server_state_shutdown() {
        let mut state = ServerState::new();
        assert!(!state.shutdown_requested());

        state.request_shutdown();
        assert!(state.shutdown_requested());
    }

    #[test]
    fn server_state_meetings_filter() {
        use chrono::Local;

        let now = Local::now();
        let tomorrow = now + chrono::Duration::days(1);

        let meetings = vec![
            MeetingView {
                id: "1".to_string(),
                provider_name: "unknown".to_string(),
                title: "Daily Standup".to_string(),
                start_local: now,
                end_local: now + chrono::Duration::hours(1),
                is_all_day: false,
                is_ongoing: false,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                calendar_id: "primary".to_string(),
                user_response_status: ResponseStatus::Unknown,
                other_attendee_count: 0,
                location: None,
                description: None,
                attendees: vec![],
            },
            MeetingView {
                id: "2".to_string(),
                provider_name: "unknown".to_string(),
                title: "All Day Event".to_string(),
                start_local: now,
                end_local: now + chrono::Duration::days(1),
                is_all_day: true,
                is_ongoing: false,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                calendar_id: "primary".to_string(),
                user_response_status: ResponseStatus::Unknown,
                other_attendee_count: 0,
                location: None,
                description: None,
                attendees: vec![],
            },
            MeetingView {
                id: "3".to_string(),
                provider_name: "unknown".to_string(),
                title: "Tomorrow Meeting".to_string(),
                start_local: tomorrow,
                end_local: tomorrow + chrono::Duration::hours(1),
                is_all_day: false,
                is_ongoing: false,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                calendar_id: "primary".to_string(),
                user_response_status: ResponseStatus::Unknown,
                other_attendee_count: 0,
                location: None,
                description: None,
                attendees: vec![],
            },
        ];

        let mut state = ServerState::new();
        state.set_meetings(meetings);

        // No filter
        assert_eq!(state.get_meetings(None).len(), 3);

        // Skip all-day
        let filter = MeetingsFilter::new().skip_all_day(true);
        assert_eq!(state.get_meetings(Some(&filter)).len(), 2);

        // Include title
        let filter = MeetingsFilter::new().include_title("standup");
        assert_eq!(state.get_meetings(Some(&filter)).len(), 1);

        // Exclude title
        let filter = MeetingsFilter::new().exclude_title("standup");
        assert_eq!(state.get_meetings(Some(&filter)).len(), 2);

        // Today only
        let filter = MeetingsFilter::new().today_only(true);
        let result = state.get_meetings(Some(&filter));
        // Should exclude tomorrow's meeting
        assert!(!result.iter().any(|m| m.id == "3"));

        // Limit
        let filter = MeetingsFilter::new().limit(1);
        assert_eq!(state.get_meetings(Some(&filter)).len(), 1);
    }

    #[tokio::test]
    async fn request_handler_ping() {
        let state = new_shared_state();
        let handler = RequestHandler::new(state);

        let response = handler.handle(&Request::Ping).await;
        assert_eq!(response, Response::Pong);
    }

    #[tokio::test]
    async fn request_handler_status() {
        let state = new_shared_state();
        let handler = RequestHandler::new(state);

        let response = handler.handle(&Request::Status).await;
        match response {
            Response::Status { info } => {
                assert!(info.uptime_seconds < 2);
            }
            _ => panic!("Expected Status response"),
        }
    }

    #[tokio::test]
    async fn request_handler_snooze() {
        let state = new_shared_state();
        let handler = RequestHandler::new(state.clone());

        let response = handler.handle(&Request::Snooze { minutes: 30 }).await;
        assert_eq!(response, Response::Ok);

        let state = state.read().await;
        assert!(state.is_snoozed());
    }

    #[tokio::test]
    async fn request_handler_shutdown() {
        let state = new_shared_state();
        let handler = RequestHandler::new(state.clone());

        let response = handler.handle(&Request::Shutdown).await;
        assert_eq!(response, Response::Ok);

        let state = state.read().await;
        assert!(state.shutdown_requested());
    }

    #[tokio::test]
    async fn request_handler_mutate_event_without_mutator() {
        let state = new_shared_state();
        let handler = RequestHandler::new(state);

        let response = handler
            .handle(&Request::mutate_event(
                "google:work",
                "primary",
                "evt-1",
                EventMutationAction::Decline,
            ))
            .await;

        match response {
            Response::Error { error } => {
                assert_eq!(error.code, ErrorCode::ProviderError);
                assert!(error.message.contains("not configured"));
            }
            _ => panic!("expected error response"),
        }
    }

    #[tokio::test]
    async fn request_handler_mutate_event_with_mutator_success() {
        let state = new_shared_state();
        let mutator: EventMutator = Arc::new(|request: EventMutationRequest| {
            Box::pin(async move {
                assert_eq!(request.provider_name, "google:work");
                assert_eq!(request.calendar_id, "primary");
                assert_eq!(request.event_id, "evt-1");
                assert_eq!(request.action, EventMutationAction::Decline);
                Ok(())
            })
        });
        let handler = RequestHandler::with_event_mutator(state, mutator);

        let response = handler
            .handle(&Request::mutate_event(
                "google:work",
                "primary",
                "evt-1",
                EventMutationAction::Decline,
            ))
            .await;

        assert_eq!(response, Response::Ok);
    }

    #[tokio::test]
    async fn request_handler_mutate_event_with_mutator_error() {
        let state = new_shared_state();
        let mutator: EventMutator = Arc::new(|_request: EventMutationRequest| {
            Box::pin(async move { Err(ErrorResponse::new(ErrorCode::NotFound, "not found")) })
        });
        let handler = RequestHandler::with_event_mutator(state, mutator);

        let response = handler
            .handle(&Request::mutate_event(
                "google:work",
                "primary",
                "evt-1",
                EventMutationAction::Delete,
            ))
            .await;

        match response {
            Response::Error { error } => {
                assert_eq!(error.code, ErrorCode::NotFound);
            }
            _ => panic!("expected error response"),
        }
    }

    #[test]
    fn filter_include_calendars() {
        let now = Local::now();
        let meetings = vec![
            MeetingView {
                id: "1".to_string(),
                provider_name: "unknown".to_string(),
                title: "Work Meeting".to_string(),
                start_local: now,
                end_local: now + chrono::Duration::hours(1),
                is_all_day: false,
                is_ongoing: false,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                calendar_id: "work@example.com".to_string(),
                user_response_status: ResponseStatus::Unknown,
                other_attendee_count: 0,
                location: None,
                description: None,
                attendees: vec![],
            },
            MeetingView {
                id: "2".to_string(),
                provider_name: "unknown".to_string(),
                title: "Personal Meeting".to_string(),
                start_local: now,
                end_local: now + chrono::Duration::hours(1),
                is_all_day: false,
                is_ongoing: false,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                calendar_id: "personal@example.com".to_string(),
                user_response_status: ResponseStatus::Unknown,
                other_attendee_count: 0,
                location: None,
                description: None,
                attendees: vec![],
            },
        ];

        let mut state = ServerState::new();
        state.set_meetings(meetings);

        let filter = MeetingsFilter::new().include_calendar("work");
        let result = state.get_meetings(Some(&filter));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].calendar_id, "work@example.com");

        let filter = MeetingsFilter::new().exclude_calendar("work");
        let result = state.get_meetings(Some(&filter));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].calendar_id, "personal@example.com");
    }

    #[test]
    fn filter_only_with_link() {
        use nextmeeting_core::{EventLink, LinkKind};

        let now = Local::now();
        let meetings = vec![
            MeetingView {
                id: "1".to_string(),
                provider_name: "unknown".to_string(),
                title: "With Link".to_string(),
                start_local: now,
                end_local: now + chrono::Duration::hours(1),
                is_all_day: false,
                is_ongoing: false,
                primary_link: Some(EventLink::new(LinkKind::Zoom, "https://zoom.us/j/123")),
                secondary_links: vec![],
                calendar_url: None,
                calendar_id: "primary".to_string(),
                user_response_status: ResponseStatus::Unknown,
                other_attendee_count: 0,
                location: None,
                description: None,
                attendees: vec![],
            },
            MeetingView {
                id: "2".to_string(),
                provider_name: "unknown".to_string(),
                title: "No Link".to_string(),
                start_local: now,
                end_local: now + chrono::Duration::hours(1),
                is_all_day: false,
                is_ongoing: false,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                calendar_id: "primary".to_string(),
                user_response_status: ResponseStatus::Unknown,
                other_attendee_count: 0,
                location: None,
                description: None,
                attendees: vec![],
            },
        ];

        let mut state = ServerState::new();
        state.set_meetings(meetings);

        let filter = MeetingsFilter::new().only_with_link(true);
        let result = state.get_meetings(Some(&filter));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "1");
    }

    #[test]
    fn filter_privacy() {
        let now = Local::now();
        let meetings = vec![MeetingView {
            id: "1".to_string(),
            provider_name: "unknown".to_string(),
            title: "Secret Meeting".to_string(),
            start_local: now,
            end_local: now + chrono::Duration::hours(1),
            is_all_day: false,
            is_ongoing: false,
            primary_link: None,
            secondary_links: vec![],
            calendar_url: None,
            calendar_id: "primary".to_string(),
            user_response_status: ResponseStatus::Unknown,
            other_attendee_count: 0,
            location: None,
            description: None,
            attendees: vec![],
        }];

        let mut state = ServerState::new();
        state.set_meetings(meetings);

        let filter = MeetingsFilter::new()
            .privacy(true)
            .privacy_title("Occupied");
        let result = state.get_meetings(Some(&filter));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "Occupied");
    }

    #[test]
    fn filter_multi_title_patterns() {
        let now = Local::now();
        let meetings = vec![
            MeetingView {
                id: "1".to_string(),
                provider_name: "unknown".to_string(),
                title: "Daily Standup".to_string(),
                start_local: now,
                end_local: now + chrono::Duration::hours(1),
                is_all_day: false,
                is_ongoing: false,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                calendar_id: "primary".to_string(),
                user_response_status: ResponseStatus::Unknown,
                other_attendee_count: 0,
                location: None,
                description: None,
                attendees: vec![],
            },
            MeetingView {
                id: "2".to_string(),
                provider_name: "unknown".to_string(),
                title: "Sprint Review".to_string(),
                start_local: now,
                end_local: now + chrono::Duration::hours(1),
                is_all_day: false,
                is_ongoing: false,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                calendar_id: "primary".to_string(),
                user_response_status: ResponseStatus::Unknown,
                other_attendee_count: 0,
                location: None,
                description: None,
                attendees: vec![],
            },
            MeetingView {
                id: "3".to_string(),
                provider_name: "unknown".to_string(),
                title: "Lunch".to_string(),
                start_local: now,
                end_local: now + chrono::Duration::hours(1),
                is_all_day: false,
                is_ongoing: false,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                calendar_id: "primary".to_string(),
                user_response_status: ResponseStatus::Unknown,
                other_attendee_count: 0,
                location: None,
                description: None,
                attendees: vec![],
            },
        ];

        let mut state = ServerState::new();
        state.set_meetings(meetings);

        // Include multiple patterns
        let filter = MeetingsFilter::new()
            .include_title("standup")
            .include_title("review");
        let result = state.get_meetings(Some(&filter));
        assert_eq!(result.len(), 2);

        // Exclude multiple patterns
        let filter = MeetingsFilter::new()
            .exclude_title("standup")
            .exclude_title("lunch");
        let result = state.get_meetings(Some(&filter));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "Sprint Review");
    }

    #[test]
    fn parse_work_hours_valid() {
        let result = super::parse_work_hours("09:00-18:00");
        assert!(result.is_some());
        let (start, end) = result.unwrap();
        assert_eq!(start, chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        assert_eq!(end, chrono::NaiveTime::from_hms_opt(18, 0, 0).unwrap());
    }

    #[test]
    fn parse_work_hours_invalid() {
        assert!(super::parse_work_hours("invalid").is_none());
        assert!(super::parse_work_hours("09:00").is_none());
        assert!(super::parse_work_hours("").is_none());
    }
}
