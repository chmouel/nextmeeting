//! Request/response dispatch handler.
//!
//! This module provides the request handler that routes incoming requests
//! to the appropriate logic and produces responses.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use nextmeeting_core::{MeetingView, ResponseStatus};
use nextmeeting_protocol::{
    ErrorCode, MeetingsFilter, ProviderStatus, Request, Response, StatusInfo,
};

use crate::error::{ServerError, ServerResult};
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
        let mut meetings: Vec<_> = self.meetings.to_vec();

        if let Some(filter) = filter {
            // Apply skip_all_day filter
            if filter.skip_all_day {
                meetings.retain(|m| !m.is_all_day);
            }

            // Apply include_title filter
            if let Some(ref pattern) = filter.include_title {
                let pattern_lower = pattern.to_lowercase();
                meetings.retain(|m| m.title.to_lowercase().contains(&pattern_lower));
            }

            // Apply exclude_title filter
            if let Some(ref pattern) = filter.exclude_title {
                let pattern_lower = pattern.to_lowercase();
                meetings.retain(|m| !m.title.to_lowercase().contains(&pattern_lower));
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

/// Shared server state wrapped in an Arc<RwLock>.
pub type SharedState = Arc<RwLock<ServerState>>;

/// Creates a new shared state.
pub fn new_shared_state() -> SharedState {
    Arc::new(RwLock::new(ServerState::new()))
}

/// Request handler that processes incoming requests and produces responses.
pub struct RequestHandler {
    state: SharedState,
}

impl RequestHandler {
    /// Creates a new request handler with the given state.
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }

    /// Handles a single request and returns the response.
    pub async fn handle(&self, request: &Request) -> Response {
        match request {
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
                Response::meetings(meetings)
            }
            Request::Snooze { minutes } => {
                debug!(minutes = *minutes, "Handling Snooze request");
                let mut state = self.state.write().await;
                state.snooze(*minutes);
                Response::Ok
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
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
                title: "Daily Standup".to_string(),
                start_local: now,
                end_local: now + chrono::Duration::hours(1),
                is_all_day: false,
                is_ongoing: false,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                user_response_status: ResponseStatus::Unknown,
                other_attendee_count: 0,
            },
            MeetingView {
                id: "2".to_string(),
                title: "All Day Event".to_string(),
                start_local: now,
                end_local: now + chrono::Duration::days(1),
                is_all_day: true,
                is_ongoing: false,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                user_response_status: ResponseStatus::Unknown,
                other_attendee_count: 0,
            },
            MeetingView {
                id: "3".to_string(),
                title: "Tomorrow Meeting".to_string(),
                start_local: tomorrow,
                end_local: tomorrow + chrono::Duration::hours(1),
                is_all_day: false,
                is_ongoing: false,
                primary_link: None,
                secondary_links: vec![],
                calendar_url: None,
                user_response_status: ResponseStatus::Unknown,
                other_attendee_count: 0,
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
}
