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

    // --- Display options ---
    /// Maximum title length (truncated with ellipsis)
    #[arg(long)]
    pub max_title_length: Option<usize>,

    /// Text to show when there are no meetings
    #[arg(long, default_value = "No meeting")]
    pub no_meeting_text: String,

    // --- Filter flags ---
    /// Only show meetings for today
    #[arg(long)]
    pub today_only: bool,

    /// Maximum number of meetings to display
    #[arg(long)]
    pub limit: Option<usize>,

    /// Skip all-day meetings
    #[arg(long)]
    pub skip_all_day_meeting: bool,

    /// Only include meetings matching this title pattern (can be repeated)
    #[arg(long, action = clap::ArgAction::Append)]
    pub include_title: Vec<String>,

    /// Exclude meetings matching this title pattern (can be repeated)
    #[arg(long, action = clap::ArgAction::Append)]
    pub exclude_title: Vec<String>,

    // --- Notification flags ---
    /// Minutes before meetings to send notifications (can be repeated)
    #[arg(long, action = clap::ArgAction::Append)]
    pub notify_min_before_events: Vec<u32>,

    /// Snooze notifications for N minutes
    #[arg(long)]
    pub snooze: Option<u32>,

    // --- Action flags ---
    /// Open the meeting URL in the default browser
    #[arg(long)]
    pub open_meet_url: bool,

    /// Copy the meeting URL to the clipboard
    #[arg(long)]
    pub copy_meeting_url: bool,

    /// Open the calendar day view in the browser
    #[arg(long)]
    pub open_calendar_day: bool,

    // --- Connection flags ---
    /// Google Workspace domain
    #[arg(long)]
    pub google_domain: Option<String>,

    /// Path to the server socket
    #[arg(long, env = "NEXTMEETING_SOCKET")]
    pub socket_path: Option<PathBuf>,

    /// Connection timeout in seconds
    #[arg(long, default_value = "5")]
    pub timeout: u64,

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
