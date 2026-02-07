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
    #[arg(long, short = 'v')]
    pub debug: bool,

    // --- Output format flags ---
    /// Output in Waybar JSON format
    #[arg(long, group = "output_format")]
    pub waybar: bool,

    /// Output in Polybar format
    #[arg(long, group = "output_format")]
    pub polybar: bool,

    /// Output in JSON format
    #[arg(long, group = "output_format")]
    pub json: bool,

    // --- Action flags ---
    /// Snooze notifications for N minutes
    #[arg(long)]
    pub snooze: Option<u32>,

    /// Open the meeting URL in the default browser
    #[arg(long)]
    pub open_meet_url: bool,

    /// Copy the meeting URL to the clipboard
    #[arg(long)]
    pub copy_meeting_url: bool,

    /// Open the calendar day view in the browser
    #[arg(long)]
    pub open_calendar_day: bool,

    /// Force refresh calendar data from providers
    #[arg(long)]
    pub refresh: bool,

    // --- Connection flags ---
    /// Path to the server socket (overrides config)
    #[arg(long, env = "NEXTMEETING_SOCKET")]
    pub socket_path: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    /// Returns the output format based on CLI flags.
    pub fn output_format(&self) -> nextmeeting_core::OutputFormat {
        if self.waybar {
            nextmeeting_core::OutputFormat::Waybar
        } else if self.polybar {
            nextmeeting_core::OutputFormat::Polybar
        } else if self.json {
            nextmeeting_core::OutputFormat::Json
        } else {
            nextmeeting_core::OutputFormat::Tty
        }
    }

    /// Returns whether any action flag is set.
    pub fn has_action(&self) -> bool {
        self.open_meet_url
            || self.copy_meeting_url
            || self.open_calendar_day
            || self.snooze.is_some()
            || self.refresh
    }
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

    /// Start the server daemon in the foreground
    Server,
}

/// Authentication providers.
#[derive(Debug, Subcommand)]
pub enum AuthProvider {
    /// Authenticate with Google Calendar
    #[cfg(feature = "google")]
    Google {
        /// Account name to authenticate (required when multiple accounts exist)
        #[arg(long, short)]
        account: Option<String>,

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
