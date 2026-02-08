//! Server command — starts the daemon in the foreground.
//!
//! This module orchestrates all server components:
//! - PID file (prevents duplicate instances)
//! - Signal handler (SIGTERM/SIGINT for shutdown, SIGHUP for reload)
//! - Provider instantiation from config
//! - Scheduler (periodic calendar sync)
//! - Socket server (IPC with clients)

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tracing::{error, info, warn};

use nextmeeting_core::{MeetingView, TimeWindow};
use nextmeeting_protocol::{ErrorCode, ErrorResponse, EventMutationAction as ProtocolEventAction};
use nextmeeting_providers::{
    CalendarProvider, EventMutationAction as ProviderEventMutationAction, FetchOptions,
    ProviderErrorCode, normalize_events,
};

use nextmeeting_server::{
    EventMutationRequest, EventMutator, NotifyConfig, NotifyEngine, PidFile, Scheduler,
    SchedulerConfig, ServerConfig, SharedState, SignalHandler, SocketServer, default_pid_path,
    make_connection_handler_with_mutator_and_notify, new_shared_state,
};

use crate::cli::Cli;
use crate::config::ClientConfig;
use crate::error::{ClientError, ClientResult};

/// Starts the server daemon in the foreground.
///
/// This function blocks until a shutdown signal is received (SIGTERM/SIGINT)
/// or the process is otherwise terminated.
pub async fn run(cli: &Cli, config: &ClientConfig) -> ClientResult<()> {
    // 1. Build providers from config
    let providers = build_providers(config)?;
    if providers.is_empty() {
        let config_path = ClientConfig::default_path();
        return Err(ClientError::Config(format!(
            "no calendar providers configured. To fix this, either:\n  \
             1. Run: nextmeeting auth google --credentials-file <path-to-google-credentials.json>\n  \
             2. Add a [[google.accounts]] entry in {}",
            config_path.display()
        )));
    }

    info!(
        provider_count = providers.len(),
        "Starting server with providers"
    );
    for p in &providers {
        info!(name = p.name(), "Provider registered");
    }

    let providers: Arc<Vec<Box<dyn CalendarProvider>>> = Arc::new(providers);

    // 2. Create PID file (prevents duplicate server instances)
    let _pid_file = PidFile::create(default_pid_path())
        .map_err(|e| ClientError::Config(format!("failed to create PID file: {}", e)))?;

    // 3. Signal handler
    let signal_handler = SignalHandler::new();
    signal_handler.spawn_listener();

    // 4. Shared state
    let state = new_shared_state();

    // 5. Scheduler
    let scheduler = Scheduler::new(SchedulerConfig::default());
    let scheduler_handle = scheduler.handle();

    // Store the scheduler handle in server state so Request::Refresh works
    {
        let mut s = state.write().await;
        s.set_scheduler_handle(scheduler_handle.clone());
    }

    // 6. Build notify engine from config
    let notify_config = build_notify_config(config);
    let notify_engine = std::sync::Arc::new(NotifyEngine::new(notify_config));

    // 7. Build the sync closure and spawn the scheduler
    let sync_state = state.clone();
    let sync_providers = providers.clone();
    let sync_notify = notify_engine.clone();

    let scheduler_task = tokio::spawn(async move {
        scheduler
            .run(move || {
                let state = sync_state.clone();
                let providers = sync_providers.clone();
                let engine = sync_notify.clone();
                async move {
                    let result = sync_all_providers(&providers, &state).await;

                    // After sync, run notifications on the updated meetings
                    let meetings = state.read().await.get_meetings(None);
                    engine.check_and_notify(&meetings).await;
                    engine.check_morning_agenda(&meetings).await;

                    result
                }
            })
            .await;
    });

    // 7. Socket server
    // CLI --socket-path overrides config, which overrides default
    let socket_path = cli
        .socket_path
        .clone()
        .or_else(|| config.server.socket_path.clone())
        .unwrap_or_else(nextmeeting_server::default_socket_path);

    let server_config = ServerConfig::new(&socket_path);
    let server = SocketServer::new(server_config)
        .await
        .map_err(|e| ClientError::Config(format!("failed to start socket server: {}", e)))?;

    info!(path = %socket_path.display(), "Server listening");

    let mutation_providers = providers.clone();
    let event_mutator: EventMutator = Arc::new(move |request: EventMutationRequest| {
        let providers = mutation_providers.clone();
        Box::pin(async move {
            let provider = providers
                .iter()
                .find(|p| p.name() == request.provider_name)
                .ok_or_else(|| {
                    ErrorResponse::new(
                        ErrorCode::NotFound,
                        format!("provider '{}' not found", request.provider_name),
                    )
                })?;

            let provider_action = match request.action {
                ProtocolEventAction::Decline => ProviderEventMutationAction::Decline,
                ProtocolEventAction::Delete => ProviderEventMutationAction::Delete,
            };

            provider
                .mutate_event(&request.calendar_id, &request.event_id, provider_action)
                .await
                .map_err(|e| map_provider_error(&request.provider_name, e))
        })
    });

    let handler = make_connection_handler_with_mutator_and_notify(
        state.clone(),
        event_mutator,
        notify_engine.clone(),
    );
    let shutdown = signal_handler.shutdown();

    // Run until shutdown signal
    server
        .run_until_shutdown(handler, shutdown.wait())
        .await
        .map_err(|e| ClientError::Config(format!("server error: {}", e)))?;

    // Clean shutdown: stop the scheduler
    info!("Shutting down...");
    if let Err(e) = scheduler_handle.stop().await {
        warn!(error = %e, "Failed to send stop command to scheduler");
    }

    // Give the scheduler a moment to finish
    let _ = tokio::time::timeout(Duration::from_secs(5), scheduler_task).await;

    info!("Server stopped");
    Ok(())
}

/// Builds calendar providers from client configuration.
fn build_providers(config: &ClientConfig) -> ClientResult<Vec<Box<dyn CalendarProvider>>> {
    let mut providers: Vec<Box<dyn CalendarProvider>> = Vec::new();

    #[cfg(feature = "google")]
    {
        if let Some(ref google_settings) = config.google {
            // Validate accounts before creating providers
            google_settings
                .validate()
                .map_err(|e| ClientError::Config(format!("invalid Google configuration: {}", e)))?;

            for account in &google_settings.accounts {
                let account_name = &account.name;

                match account.to_provider_config() {
                    Ok(google_config) => {
                        match nextmeeting_providers::google::GoogleProvider::new(google_config) {
                            Ok(provider) => {
                                if provider.is_authenticated() {
                                    info!(
                                        account = %account_name,
                                        "Google Calendar provider initialized (authenticated)"
                                    );
                                } else {
                                    warn!(
                                        account = %account_name,
                                        "Google Calendar provider initialized but not authenticated; \
                                         run `nextmeeting auth google --account {account_name}` to authenticate"
                                    );
                                }
                                providers.push(Box::new(provider));
                            }
                            Err(e) => {
                                return Err(ClientError::Provider(format!(
                                    "failed to create Google provider for account '{}': {}",
                                    account_name, e
                                )));
                            }
                        }
                    }
                    Err(e) => {
                        return Err(ClientError::Config(format!(
                            "invalid Google configuration for account '{}': {}",
                            account_name, e
                        )));
                    }
                }
            }
        }
    }

    Ok(providers)
}

/// Builds a NotifyConfig from client configuration.
fn build_notify_config(config: &ClientConfig) -> NotifyConfig {
    let notifications = &config.notifications;

    let mut notify_config = if notifications.minutes_before.is_empty() {
        NotifyConfig::default()
    } else {
        NotifyConfig::new(notifications.minutes_before.clone())
    };

    if let Some(ref urgency) = notifications.urgency {
        notify_config = notify_config.with_urgency(urgency);
    }

    if let Some(expiry) = notifications.expiry {
        notify_config = notify_config.with_expiry_secs(expiry);
    }

    if let Some(ref icon) = notifications.icon {
        notify_config = notify_config.with_icon_path(icon);
    }

    if let Some(ref time) = notifications.morning_agenda {
        notify_config = notify_config.with_morning_agenda_time(time);
    }

    notify_config
}

/// Fetches events from all providers, normalizes them, and updates shared state.
async fn sync_all_providers(
    providers: &[Box<dyn CalendarProvider>],
    state: &SharedState,
) -> Result<(), String> {
    let now = Utc::now();

    // Fetch events for the next 24 hours (a reasonable default window)
    let time_window = TimeWindow::from_duration(now, chrono::Duration::hours(24));
    let fetch_options = FetchOptions::new()
        .with_time_window(time_window)
        .with_expand_recurring(true);

    let mut all_meetings: Vec<MeetingView> = Vec::new();
    let mut had_error = false;

    for provider in providers {
        let provider_name = provider.name().to_string();

        match provider.fetch_events(fetch_options.clone()).await {
            Ok(result) => {
                if result.not_modified {
                    info!(provider = %provider_name, "No changes since last fetch");
                    // Keep existing meetings from this provider in state
                    continue;
                }

                let normalized = normalize_events(&result.events);
                let meetings: Vec<MeetingView> = normalized
                    .iter()
                    .map(|e| MeetingView::from_event_with_provider(e, &provider_name, now))
                    .collect();

                info!(
                    provider = %provider_name,
                    event_count = meetings.len(),
                    "Fetched and normalized events"
                );

                // Update provider status
                let status = nextmeeting_protocol::ProviderStatus {
                    name: provider_name.clone(),
                    healthy: true,
                    last_fetch: Some(now),
                    error: None,
                    event_count: meetings.len(),
                };
                state.write().await.set_provider_status(status);

                all_meetings.extend(meetings);
            }
            Err(e) => {
                error!(
                    provider = %provider_name,
                    error = %e,
                    "Failed to fetch events"
                );

                // Update provider status with error
                let status = nextmeeting_protocol::ProviderStatus {
                    name: provider_name.clone(),
                    healthy: false,
                    last_fetch: None,
                    error: Some(e.to_string()),
                    event_count: 0,
                };
                state.write().await.set_provider_status(status);

                had_error = true;
            }
        }
    }

    // Sort by start time
    all_meetings.sort_by(|a, b| a.start_local.cmp(&b.start_local));

    // Update shared state with all collected meetings
    state.write().await.set_meetings(all_meetings);

    if had_error {
        Err("one or more providers failed to sync".into())
    } else {
        Ok(())
    }
}

fn map_provider_error(
    provider_name: &str,
    err: nextmeeting_providers::ProviderError,
) -> ErrorResponse {
    let code = match err.code() {
        ProviderErrorCode::AuthenticationFailed => ErrorCode::AuthenticationFailed,
        ProviderErrorCode::RateLimited => ErrorCode::RateLimited,
        ProviderErrorCode::NotFound => ErrorCode::NotFound,
        ProviderErrorCode::BadRequest => ErrorCode::InvalidRequest,
        ProviderErrorCode::ConfigurationError => ErrorCode::InvalidRequest,
        ProviderErrorCode::AuthorizationFailed
        | ProviderErrorCode::NetworkError
        | ProviderErrorCode::ServerError
        | ProviderErrorCode::InvalidResponse
        | ProviderErrorCode::CalendarError
        | ProviderErrorCode::InternalError => ErrorCode::ProviderError,
    };

    ErrorResponse::new(code, format!("{}: {}", provider_name, err.message()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nextmeeting_providers::ProviderError;

    #[test]
    fn map_provider_error_maps_not_found() {
        let response = map_provider_error("google:work", ProviderError::not_found("event missing"));
        assert_eq!(response.code, ErrorCode::NotFound);
        assert!(response.message.contains("google:work"));
    }

    #[test]
    fn map_provider_error_maps_rate_limit() {
        let response = map_provider_error("google:work", ProviderError::rate_limited("slow down"));
        assert_eq!(response.code, ErrorCode::RateLimited);
    }

    #[test]
    fn map_provider_error_maps_bad_request() {
        let response = map_provider_error("google:work", ProviderError::bad_request("invalid"));
        assert_eq!(response.code, ErrorCode::InvalidRequest);
    }
}
