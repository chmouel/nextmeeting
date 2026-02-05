//! nextmeeting CLI entry point.

use std::process::ExitCode;

use clap::Parser;
use tracing::Level;
use tracing_subscriber::EnvFilter;

use nextmeeting_client::cli::{AuthProvider, Cli, Command, ConfigAction};
use nextmeeting_client::config::ClientConfig;
use nextmeeting_client::error::ClientResult;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    // Initialize tracing
    let filter = if cli.debug {
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
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> ClientResult<()> {
    // Load configuration
    let config = if let Some(ref path) = cli.config {
        ClientConfig::load_from(path)
            .map_err(|e| nextmeeting_client::error::ClientError::Config(e))?
    } else {
        ClientConfig::load().unwrap_or_default()
    };

    // Handle subcommands
    match cli.command {
        Some(Command::Auth { provider }) => match provider {
            #[cfg(feature = "google")]
            AuthProvider::Google {
                client_id,
                client_secret,
                credentials_file,
                domain,
                force,
            } => {
                nextmeeting_client::commands::auth::google(
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
            // TODO: Implement status command when server is ready
            println!("Status command not yet implemented.");
            Ok(())
        }
        None => {
            // Default behavior: show next meeting (to be implemented)
            println!("nextmeeting - Your next meeting at a glance");
            println!();
            println!("Run 'nextmeeting --help' for usage information.");
            println!();
            println!("Quick start:");
            println!("  1. Set up Google Calendar: nextmeeting auth google --client-id <ID> --client-secret <SECRET>");
            println!("  2. View next meeting: nextmeeting");
            Ok(())
        }
    }
}
