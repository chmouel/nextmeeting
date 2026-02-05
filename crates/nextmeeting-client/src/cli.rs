//! Command-line interface definition.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// nextmeeting - Your next meeting at a glance
#[derive(Debug, Parser)]
#[command(name = "nextmeeting")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to configuration file
    #[arg(long, short, env = "NEXTMEETING_CONFIG")]
    pub config: Option<PathBuf>,

    /// Enable debug output
    #[arg(long, short)]
    pub debug: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Available commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Authentication commands
    Auth {
        #[command(subcommand)]
        provider: AuthProvider,
    },

    /// Configuration commands
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Show daemon status
    Status,
}

/// Authentication providers.
#[derive(Debug, Subcommand)]
pub enum AuthProvider {
    /// Authenticate with Google Calendar
    #[cfg(feature = "google")]
    Google {
        /// OAuth client ID (from Google Cloud Console)
        #[arg(long, env = "GOOGLE_CLIENT_ID")]
        client_id: Option<String>,

        /// OAuth client secret (from Google Cloud Console)
        #[arg(long, env = "GOOGLE_CLIENT_SECRET")]
        client_secret: Option<String>,

        /// Path to Google Cloud Console credentials JSON file
        ///
        /// This is the JSON file downloaded from the Google Cloud Console
        /// OAuth 2.0 credentials page. Alternative to providing client_id
        /// and client_secret separately.
        #[arg(long, env = "GOOGLE_CREDENTIALS_FILE")]
        credentials_file: Option<PathBuf>,

        /// Google Workspace domain (optional)
        #[arg(long)]
        domain: Option<String>,

        /// Force re-authentication even if already authenticated
        #[arg(long, short)]
        force: bool,
    },
}

/// Configuration actions.
#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Dump current configuration
    Dump,

    /// Validate configuration
    Validate,

    /// Show configuration file path
    Path,
}
