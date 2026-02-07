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

    // --- Format flags ---
    /// Custom format template for main display
    #[arg(long)]
    pub format: Option<String>,

    /// Custom format template for tooltip
    #[arg(long)]
    pub tooltip_format: Option<String>,

    /// Hour separator character (e.g., ":", "h")
    #[arg(long)]
    pub hour_separator: Option<String>,

    /// Minutes offset after which absolute time is shown instead of countdown
    #[arg(long)]
    pub until_offset: Option<i64>,

    // --- Filter flags ---
    /// Only include events from these calendars (repeatable)
    #[arg(long, action = clap::ArgAction::Append)]
    pub include_calendar: Vec<String>,

    /// Exclude events from these calendars (repeatable)
    #[arg(long, action = clap::ArgAction::Append)]
    pub exclude_calendar: Vec<String>,

    /// Only include events starting within N minutes
    #[arg(long)]
    pub within_mins: Option<u32>,

    /// Only include events within work hours (format: "HH:MM-HH:MM")
    #[arg(long)]
    pub work_hours: Option<String>,

    /// Only include events that have a meeting link
    #[arg(long)]
    pub only_with_link: bool,

    // --- Privacy flags ---
    /// Enable privacy mode (replace titles)
    #[arg(long)]
    pub privacy: bool,

    /// Title to use when privacy mode is enabled
    #[arg(long)]
    pub privacy_title: Option<String>,

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

    /// Copy the meeting ID to the clipboard
    #[arg(long)]
    pub copy_meeting_id: bool,

    /// Copy the meeting passcode to the clipboard
    #[arg(long)]
    pub copy_meeting_passcode: bool,

    /// Open the calendar day view in the browser
    #[arg(long)]
    pub open_calendar_day: bool,

    /// Open a meeting link from the clipboard
    #[arg(long)]
    pub open_link_from_clipboard: bool,

    /// Open meeting URL with a custom command instead of default browser
    #[arg(long)]
    pub open_with: Option<String>,

    /// Create a new meeting (meet/zoom/teams/gcal)
    #[arg(long)]
    pub create: Option<String>,

    /// Custom URL for --create
    #[arg(long)]
    pub create_url: Option<String>,

    /// Google Workspace domain for calendar URLs
    #[arg(long)]
    pub google_domain: Option<String>,

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
            || self.copy_meeting_id
            || self.copy_meeting_passcode
            || self.open_calendar_day
            || self.open_link_from_clipboard
            || self.create.is_some()
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
