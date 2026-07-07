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
    config
        .notifications
        .validate_end_warning()
        .map_err(|e| ClientError::Config(format!("invalid notifications configuration: {}", e)))?;

    config
        .server
        .validate_scheduling()
        .map_err(|e| ClientError::Config(format!("invalid server configuration: {}", e)))?;

    // 1. Build providers from config
    let providers = build_providers(config)?;
    if providers.is_empty() {
        let config_path = ClientConfig::default_path();
        return Err(ClientError::Config(format!(
            "no calendar providers configured. To fix this, either:\n  \
             1. Run: nextmeeting auth google --guide\n  \
             2. Run: nextmeeting auth google --credentials-file /path/to/client_secret_<id>.json\n  \
             3. Add a [caldav] or [[google.accounts]] section in {}",
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

    // 5. Scheduler (tunable via [server] in config.toml)
    let scheduler = Scheduler::new(build_scheduler_config(config));
    let scheduler_handle = scheduler.handle();

    // Store the scheduler handle in server state so Request::Refresh works
    {
        let mut s = state.write().await;
        s.set_scheduler_handle(scheduler_handle.clone());
    }

    // 6. Build notify engine from config
    let notify_config = build_notify_config(config);
    let notify_engine = std::sync::Arc::new(NotifyEngine::new(notify_config));

    // 7. Build the sync closure and spawn the scheduler.
    //
    // The sync closure only fetches calendar data; notifications run on a
    // dedicated ticker below so alert precision does not depend on the sync
    // cadence. After each sync the ticker is woken immediately so fresh
    // meetings are checked without waiting for the next tick.
    let sync_state = state.clone();
    let sync_providers = providers.clone();
    let notify_wakeup = Arc::new(tokio::sync::Notify::new());
    let sync_wakeup = notify_wakeup.clone();

    let scheduler_task = tokio::spawn(async move {
        scheduler
            .run(move || {
                let state = sync_state.clone();
                let providers = sync_providers.clone();
                let wakeup = sync_wakeup.clone();
                async move {
                    let result = sync_all_providers(&providers, &state).await;
                    wakeup.notify_one();
                    result
                }
            })
            .await;
    });

    // 8. Notification ticker — checks upcoming meetings on a short cadence,
    // independent of calendar sync, so short-fuse warnings fire on time.
    let notify_tick = Duration::from_secs(config.server.notify_tick_secs.unwrap_or(30));
    let ticker_state = state.clone();
    let ticker_engine = notify_engine.clone();
    let notify_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(notify_tick);
        // Skip missed ticks (e.g. after system sleep) instead of bursting.
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = notify_wakeup.notified() => {}
            }
            let meetings = ticker_state.read().await.get_meetings(None);
            ticker_engine.check_and_notify(&meetings).await;
            ticker_engine.check_morning_agenda(&meetings).await;
            ticker_engine.cleanup().await;
        }
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

    // Clean shutdown: stop the scheduler and the notification ticker
    info!("Shutting down...");
    notify_task.abort();
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

    #[cfg(feature = "caldav")]
    {
        if let Some(ref caldav_settings) = config.caldav {
            caldav_settings
                .validate()
                .map_err(|e| ClientError::Config(format!("invalid CalDAV configuration: {}", e)))?;

            let caldav_config = caldav_settings
                .to_provider_config()
                .map_err(|e| ClientError::Config(format!("invalid CalDAV configuration: {}", e)))?;

            let provider = nextmeeting_providers::caldav::CalDavProvider::new(caldav_config)?;
            info!("CalDAV provider initialised");
            providers.push(Box::new(provider));
        }
    }

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

/// Builds a SchedulerConfig from client configuration.
fn build_scheduler_config(config: &ClientConfig) -> SchedulerConfig {
    let mut scheduler_config = SchedulerConfig::default();

    if let Some(secs) = config.server.sync_interval_secs {
        scheduler_config.sync_interval = Duration::from_secs(secs);
    }
    if let Some(secs) = config.server.refresh_cooldown_secs {
        scheduler_config.refresh_cooldown = Duration::from_secs(secs);
    }

    scheduler_config
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

    notify_config = notify_config.with_end_warning_minutes(notifications.end_warning_minutes);

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

    let mut had_error = false;

    for provider in providers {
        let provider_name = provider.name().to_string();

        match provider.fetch_events(fetch_options.clone()).await {
            Ok(result) => {
                if result.not_modified {
                    info!(provider = %provider_name, "No changes since last fetch");
                    // Keep existing meetings from this provider in state
                    state.write().await.touch_provider_sync();
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
                let mut s = state.write().await;
                s.set_provider_status(status);
                s.set_provider_meetings(&provider_name, meetings);
            }
            Err(e) => {
                error!(
                    provider = %provider_name,
                    error = %e,
                    "Failed to fetch events; keeping last-known-good meetings"
                );

                // Preserve the provider's previous last_fetch time and
                // report its retained (last-known-good) meeting count —
                // those meetings are kept in state, so the status should
                // reflect what is actually still shown to the user.
                state
                    .write()
                    .await
                    .set_provider_error(&provider_name, e.to_string());

                had_error = true;
            }
        }
    }

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
    use crate::config::ClientConfig;
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

    #[test]
    fn build_notify_config_maps_end_warning_minutes() {
        let mut config = ClientConfig::default();
        config.notifications.end_warning_minutes = Some(7);

        let notify = build_notify_config(&config);
        assert_eq!(notify.end_warning_minutes, Some(7));
    }

    #[test]
    fn build_scheduler_config_uses_defaults_when_unset() {
        let config = ClientConfig::default();
        let scheduler = build_scheduler_config(&config);
        let defaults = SchedulerConfig::default();
        assert_eq!(scheduler.sync_interval, defaults.sync_interval);
        assert_eq!(scheduler.refresh_cooldown, defaults.refresh_cooldown);
    }

    #[test]
    fn build_scheduler_config_maps_settings() {
        let mut config = ClientConfig::default();
        config.server.sync_interval_secs = Some(120);
        config.server.refresh_cooldown_secs = Some(10);

        let scheduler = build_scheduler_config(&config);
        assert_eq!(scheduler.sync_interval, Duration::from_secs(120));
        assert_eq!(scheduler.refresh_cooldown, Duration::from_secs(10));
    }
}
