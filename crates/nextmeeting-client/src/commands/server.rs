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
use nextmeeting_providers::{CalendarProvider, FetchOptions, normalize_events};

use nextmeeting_server::{
    PidFile, Scheduler, SchedulerConfig, ServerConfig, SharedState, SignalHandler, SocketServer,
    default_pid_path, make_connection_handler, new_shared_state,
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
        let default_creds = crate::config::ClientConfig::default_data_dir().join("oauth.json");
        return Err(ClientError::Config(format!(
            "no calendar providers configured. To fix this, either:\n  \
             1. Run: nextmeeting auth google --credentials-file <path-to-google-credentials.json>\n  \
             2. Place your Google OAuth credentials JSON at {}\n  \
             3. Add a [google] section to your config.toml with client_id/client_secret \
                or credentials_file",
            default_creds.display()
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

    // 6. Build the sync closure and spawn the scheduler
    let sync_state = state.clone();
    let sync_providers = providers.clone();

    let scheduler_task = tokio::spawn(async move {
        scheduler
            .run(move || {
                let state = sync_state.clone();
                let providers = sync_providers.clone();
                async move { sync_all_providers(&providers, &state).await }
            })
            .await;
    });

    // 7. Socket server
    let socket_path = cli
        .socket_path
        .clone()
        .unwrap_or_else(nextmeeting_server::default_socket_path);

    let server_config = ServerConfig::new(&socket_path);
    let server = SocketServer::new(server_config)
        .await
        .map_err(|e| ClientError::Config(format!("failed to start socket server: {}", e)))?;

    info!(path = %socket_path.display(), "Server listening");

    let handler = make_connection_handler(state.clone());
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
///
/// If an explicit `[google]` section exists in config, uses that.
/// Otherwise, attempts auto-detection by checking for credentials at
/// the default path (`~/.local/share/nextmeeting/oauth.json`) and
/// existing tokens at the default token path.
fn build_providers(config: &ClientConfig) -> ClientResult<Vec<Box<dyn CalendarProvider>>> {
    let mut providers: Vec<Box<dyn CalendarProvider>> = Vec::new();

    #[cfg(feature = "google")]
    {
        if let Some(ref google_settings) = config.google {
            // Explicit [google] section in config — use it (now with credential resolution)
            match google_settings.to_provider_config() {
                Ok(google_config) => {
                    match nextmeeting_providers::google::GoogleProvider::new(google_config) {
                        Ok(provider) => {
                            if provider.is_authenticated() {
                                info!("Google Calendar provider initialized (authenticated)");
                            } else {
                                warn!(
                                    "Google Calendar provider initialized but not authenticated; \
                                     run `nextmeeting auth google` to authenticate"
                                );
                            }
                            providers.push(Box::new(provider));
                        }
                        Err(e) => {
                            return Err(ClientError::Provider(format!(
                                "failed to create Google provider: {}",
                                e
                            )));
                        }
                    }
                }
                Err(e) => {
                    return Err(ClientError::Config(format!(
                        "invalid Google configuration: {}",
                        e
                    )));
                }
            }
        } else {
            // No [google] section — try auto-detection from default paths
            match try_auto_detect_google() {
                Ok(Some(provider)) => {
                    if provider.is_authenticated() {
                        info!(
                            "Google Calendar provider auto-detected from default credentials \
                             (authenticated)"
                        );
                    } else {
                        warn!(
                            "Google Calendar provider auto-detected but not authenticated; \
                             run `nextmeeting auth google` to authenticate"
                        );
                    }
                    providers.push(Box::new(provider));
                }
                Ok(None) => {
                    // No credentials found at default path — skip silently
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        "Found default credentials but failed to create Google provider"
                    );
                }
            }
        }
    }

    Ok(providers)
}

/// Attempts to auto-detect a Google provider from default credential and token paths.
///
/// Returns `Ok(Some(provider))` if credentials were found at the default data dir,
/// `Ok(None)` if no credentials exist, or `Err` if credentials exist but are invalid.
#[cfg(feature = "google")]
fn try_auto_detect_google(
) -> Result<Option<nextmeeting_providers::google::GoogleProvider>, String> {
    use nextmeeting_providers::google::{GoogleConfig, GoogleProvider, OAuthCredentials};

    let default_creds_path =
        crate::config::ClientConfig::default_data_dir().join("oauth.json");
    if !default_creds_path.exists() {
        return Ok(None);
    }

    let credentials = OAuthCredentials::from_file(&default_creds_path).map_err(|e| {
        format!(
            "failed to load credentials from {}: {}",
            default_creds_path.display(),
            e
        )
    })?;
    credentials.validate().map_err(|e| e.to_string())?;

    let config = GoogleConfig::new(credentials);
    let provider = GoogleProvider::new(config).map_err(|e| e.to_string())?;
    Ok(Some(provider))
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
                    .map(|e| MeetingView::from_event(e, now))
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
