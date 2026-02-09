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

    /// Enable debug mode (equivalent to --debug-level 2)
    #[arg(long, short = 'd')]
    pub debug: bool,

    /// Increase verbosity (can be repeated: -v, -vv, -vvv, etc.)
    #[arg(long, short = 'v', action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Set explicit debug level (0-5)
    #[arg(long, value_name = "LEVEL")]
    pub debug_level: Option<u8>,

    // --- Output format flags ---
    /// Output in Waybar JSON format
    #[arg(long, group = "output_format")]
    pub waybar: bool,

    /// Output in JSON format
    #[arg(long, group = "output_format")]
    pub json: bool,

    // --- Privacy flags ---
    /// Enable privacy mode (replace titles)
    #[arg(long)]
    pub privacy: bool,

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

    /// Create a new meeting (meet/zoom/teams/gcal)
    #[arg(long)]
    pub create: Option<String>,

    /// Custom URL for --create
    #[arg(long)]
    pub create_url: Option<String>,

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

    /// Determine effective debug level from CLI args, env, and config
    pub fn effective_debug_level(
        &self,
        config: &crate::config::ClientConfig,
    ) -> nextmeeting_core::tracing::DebugLevel {
        use nextmeeting_core::tracing::DebugLevel;

        // 1. CLI explicit level
        if let Some(level) = self.debug_level {
            return DebugLevel::from_u8(level);
        }

        // 2. CLI verbose flags (-v, -vv, etc.)
        if self.verbose > 0 {
            return DebugLevel::from_u8(self.verbose);
        }

        // 3. CLI --debug flag (backward compat)
        if self.debug {
            return DebugLevel::Debug;
        }

        // 4. Environment variable
        if let Ok(level_str) = std::env::var("NEXTMEETING_DEBUG_LEVEL") {
            if let Ok(level) = level_str.parse::<u8>() {
                return DebugLevel::from_u8(level);
            }
        }

        // 5. Config file
        config.effective_debug_level()
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
