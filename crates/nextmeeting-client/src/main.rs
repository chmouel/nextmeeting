//! nextmeeting CLI entry point.

use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use tracing::{Level, debug, info};
use tracing_subscriber::EnvFilter;

use nextmeeting_client::cli::{AuthProvider, Cli, Command, ConfigAction};
use nextmeeting_client::config::ClientConfig;
use nextmeeting_client::error::{ClientError, ClientResult};
use nextmeeting_client::socket::SocketClient;

use nextmeeting_core::{FormatOptions, MeetingView, OutputFormat, OutputFormatter};
use nextmeeting_protocol::{MeetingsFilter, Request, Response};

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    // Load configuration (needed early for debug flag)
    let config = if let Some(ref path) = cli.config {
        match ClientConfig::load_from(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: {}", e);
                return ExitCode::FAILURE;
            }
        }
    } else {
        ClientConfig::load().unwrap_or_default()
    };

    // Initialize tracing: CLI --debug overrides config debug
    let debug_enabled = cli.debug || config.debug;
    let filter = if debug_enabled {
        EnvFilter::new(Level::DEBUG.to_string())
    } else {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(Level::WARN.to_string()))
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    // Run the command
    match run(cli, config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli, config: ClientConfig) -> ClientResult<()> {
    // Handle subcommands
    match cli.command {
        Some(Command::Auth { provider }) => match provider {
            #[cfg(feature = "google")]
            AuthProvider::Google {
                account,
                client_id,
                client_secret,
                credentials_file,
                domain,
                force,
            } => {
                nextmeeting_client::commands::auth::google(
                    account,
                    client_id,
                    client_secret,
                    credentials_file,
                    domain,
                    force,
                    &config,
                )
                .await
            }
        },
        Some(Command::Config { action }) => match action {
            ConfigAction::Dump => nextmeeting_client::commands::config::dump(&config),
            ConfigAction::Validate => nextmeeting_client::commands::config::validate(&config),
            ConfigAction::Path => nextmeeting_client::commands::config::path(),
        },
        Some(Command::Status) => {
            let client = make_client(&cli, &config);
            run_status(&client).await
        }
        Some(Command::Server) => {
            nextmeeting_client::commands::server::run(&cli, &config).await
        }
        None => {
            // Default behavior: connect to server, fetch meetings, render output
            run_default(&cli, &config).await
        }
    }
}

/// Default mode: connect to the server, fetch meetings, and render output.
async fn run_default(cli: &Cli, config: &ClientConfig) -> ClientResult<()> {
    let client = make_client(cli, config);

    // Handle snooze action (fire-and-forget to the server)
    if let Some(minutes) = cli.snooze {
        return nextmeeting_client::actions::snooze(&client, minutes).await;
    }

    // Fetch meetings from the server
    let meetings = fetch_meetings(cli, config, &client).await?;

    // Handle action flags (open/copy) before rendering
    if cli.open_meet_url {
        return nextmeeting_client::actions::open_meeting_url(&meetings);
    }

    if cli.copy_meeting_url {
        return nextmeeting_client::actions::copy_meeting_url(&meetings);
    }

    if cli.open_calendar_day {
        let domain = {
            #[cfg(feature = "google")]
            {
                config
                    .google
                    .as_ref()
                    .and_then(|g| {
                        g.accounts
                            .iter()
                            .find_map(|a| a.domain.as_deref())
                    })
            }
            #[cfg(not(feature = "google"))]
            {
                None
            }
        };
        return nextmeeting_client::actions::open_calendar_day(&meetings, domain);
    }

    // Render output
    render_output(cli, config, &meetings);

    Ok(())
}

/// Fetches meetings from the server, with auto-spawn fallback.
async fn fetch_meetings(
    cli: &Cli,
    config: &ClientConfig,
    client: &SocketClient,
) -> ClientResult<Vec<MeetingView>> {
    // Build filter from config
    let filter = build_filter(config);
    let request = if filter_is_empty(&filter) {
        Request::get_meetings()
    } else {
        Request::get_meetings_with_filter(filter)
    };

    // Try to connect; if server is not running, attempt auto-spawn
    let response = match client.send(request.clone()).await {
        Ok(resp) => resp,
        Err(ClientError::Connection(_)) => {
            info!("server not running, attempting auto-spawn");
            auto_spawn_server(cli, client).await?;

            // Retry after spawn
            client.send(request).await?
        }
        Err(e) => return Err(e),
    };

    match response {
        Response::Meetings { meetings } => {
            debug!(count = meetings.len(), "received meetings");
            Ok(meetings)
        }
        Response::Error { error } => Err(ClientError::Protocol(format!(
            "server error: {}",
            error.message
        ))),
        other => Err(ClientError::Protocol(format!(
            "unexpected response: {:?}",
            other
        ))),
    }
}

/// Builds a MeetingsFilter from config settings.
fn build_filter(config: &ClientConfig) -> MeetingsFilter {
    let filters = &config.filters;
    let mut filter = MeetingsFilter::new();

    if filters.today_only {
        filter = filter.today_only(true);
    }

    if let Some(limit) = filters.limit {
        filter = filter.limit(limit);
    }

    if filters.skip_all_day {
        filter = filter.skip_all_day(true);
    }

    // Use the first include/exclude title pattern (protocol supports one)
    if let Some(pattern) = filters.include_titles.first() {
        filter = filter.include_title(pattern.clone());
    }

    if let Some(pattern) = filters.exclude_titles.first() {
        filter = filter.exclude_title(pattern.clone());
    }

    filter
}

/// Returns true if the filter has no constraints.
fn filter_is_empty(filter: &MeetingsFilter) -> bool {
    !filter.today_only
        && filter.limit.is_none()
        && !filter.skip_all_day
        && filter.include_title.is_none()
        && filter.exclude_title.is_none()
}

/// Renders meetings to stdout based on the output format.
fn render_output(cli: &Cli, config: &ClientConfig, meetings: &[MeetingView]) {
    let format = cli.output_format();
    let display = &config.display;

    let mut format_options = FormatOptions::default();
    if let Some(max_len) = display.max_title_length {
        format_options.max_title_length = Some(max_len);
    }

    let formatter = OutputFormatter::new(format_options);
    let no_meeting_text = &display.no_meeting_text;

    match format {
        OutputFormat::Waybar => {
            let output = formatter.format_waybar(meetings, no_meeting_text);
            // serde_json output for waybar
            match serde_json::to_string(&output) {
                Ok(json) => println!("{}", json),
                Err(e) => eprintln!("error: failed to serialize waybar output: {}", e),
            }
        }
        OutputFormat::Polybar => {
            let output = formatter.format_polybar(meetings, no_meeting_text);
            println!("{}", output);
        }
        OutputFormat::Json => {
            let output = formatter.format_json(meetings);
            match serde_json::to_string_pretty(&output) {
                Ok(json) => println!("{}", json),
                Err(e) => eprintln!("error: failed to serialize JSON output: {}", e),
            }
        }
        OutputFormat::Tty => {
            if meetings.is_empty() {
                println!("{}", no_meeting_text);
                return;
            }

            let formatted = formatter.format_tty(meetings);
            for entry in &formatted {
                println!("{}", entry.text);
            }
        }
    }
}

/// Runs the status command.
async fn run_status(client: &SocketClient) -> ClientResult<()> {
    let response = client.send(Request::Status).await?;

    match response {
        Response::Status { info } => {
            let uptime_str = format_duration_human(info.uptime_seconds);
            println!("Server status:");
            println!("  Uptime: {}", uptime_str);

            if let Some(last_sync) = info.last_sync {
                println!("  Last sync: {}", last_sync.format("%Y-%m-%d %H:%M:%S UTC"));
            } else {
                println!("  Last sync: never");
            }

            if let Some(snoozed) = info.snoozed_until {
                println!(
                    "  Snoozed until: {}",
                    snoozed.format("%Y-%m-%d %H:%M:%S UTC")
                );
            }

            if info.providers.is_empty() {
                println!("  Providers: none configured");
            } else {
                println!("  Providers:");
                for p in &info.providers {
                    let health = if p.healthy { "healthy" } else { "unhealthy" };
                    print!("    {} ({}): {} events", p.name, health, p.event_count);
                    if let Some(ref err) = p.error {
                        print!(" [error: {}]", err);
                    }
                    println!();
                }
            }

            Ok(())
        }
        Response::Error { error } => Err(ClientError::Protocol(format!(
            "status failed: {}",
            error.message
        ))),
        _ => Err(ClientError::Protocol(
            "unexpected response to status request".into(),
        )),
    }
}

/// Attempts to auto-spawn the server daemon.
async fn auto_spawn_server(cli: &Cli, client: &SocketClient) -> ClientResult<()> {
    use tokio::process::Command as TokioCommand;

    let exe = std::env::current_exe()
        .map_err(|e| ClientError::Connection(format!("failed to find executable: {}", e)))?;

    debug!(exe = %exe.display(), "spawning server process");

    // Spawn the server in the background
    let mut cmd = TokioCommand::new(&exe);

    // Pass config file path so the server uses the same configuration
    if let Some(ref config_path) = cli.config {
        cmd.arg("--config").arg(config_path);
    }

    // Pass explicit --socket-path if the CLI flag was used (overrides config)
    if let Some(ref socket_path) = cli.socket_path {
        cmd.arg("--socket-path").arg(socket_path);
    }

    cmd.arg("server");

    // Detach from the current process group
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(unix)]
    {
        // SAFETY: setsid() is async-signal-safe per POSIX
        unsafe {
            cmd.pre_exec(|| {
                // Create a new session so the server survives the client exiting
                libc::setsid();
                Ok(())
            });
        }
    }

    cmd.spawn()
        .map_err(|e| ClientError::Connection(format!("failed to spawn server: {}", e)))?;

    // Wait for the server to become ready
    let max_retries = 20;
    let retry_delay = Duration::from_millis(100);

    for attempt in 1..=max_retries {
        tokio::time::sleep(retry_delay).await;

        if client.socket_exists() {
            match client.ping().await {
                Ok(true) => {
                    debug!(attempt, "server is ready");
                    return Ok(());
                }
                _ => {
                    debug!(attempt, "server socket exists but not responding yet");
                }
            }
        }
    }

    Err(ClientError::Connection(
        "server failed to start within timeout".into(),
    ))
}

/// Creates a SocketClient from CLI and config.
///
/// CLI `--socket-path` takes priority over config `[server] socket_path`,
/// which takes priority over the default.
fn make_client(cli: &Cli, config: &ClientConfig) -> SocketClient {
    let socket_path = cli
        .socket_path
        .clone()
        .or_else(|| config.server.socket_path.clone())
        .unwrap_or_else(nextmeeting_server::default_socket_path);

    let timeout = Duration::from_secs(config.server.timeout);

    SocketClient::new(socket_path, timeout)
}

/// Formats seconds as a human-readable duration.
fn format_duration_human(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{}d {}h {}m {}s", days, hours, minutes, secs)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}
