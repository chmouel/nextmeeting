//! Client configuration.
//!
//! All settings live in a single `config.toml` file at
//! `~/.config/nextmeeting/config.toml` by default.
//!
//! Credential values (`client_id`, `client_secret`) support secret references:
//! - `pass::path/in/store` — resolved via `pass show`
//! - `env::VAR_NAME` — resolved from the environment
//! - plain text — used as-is

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ClientConfig (config.toml)
// ---------------------------------------------------------------------------

/// Configuration for the nextmeeting client.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    /// Google Calendar settings.
    #[cfg(feature = "google")]
    pub google: Option<GoogleSettings>,

    /// Debug mode.
    pub debug: bool,

    /// Google Workspace domain (used for calendar URLs and meeting creation).
    pub google_domain: Option<String>,

    /// Display settings.
    #[serde(default)]
    pub display: DisplaySettings,

    /// Filter settings.
    #[serde(default)]
    pub filters: FilterSettings,

    /// Notification settings.
    #[serde(default)]
    pub notifications: NotificationSettings,

    /// Server/connection settings.
    #[serde(default)]
    pub server: ServerSettings,
}

/// Display settings for output formatting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplaySettings {
    /// Maximum title length (truncated with ellipsis).
    pub max_title_length: Option<usize>,

    /// Text to show when there are no meetings.
    pub no_meeting_text: String,

    /// Custom format template for main display.
    pub format: Option<String>,

    /// Custom format template for tooltip.
    pub tooltip_format: Option<String>,

    /// Hour separator character (e.g., ":", "h").
    pub hour_separator: Option<String>,

    /// Minutes offset after which absolute time is shown instead of countdown.
    pub until_offset: Option<i64>,

    /// Time format preference ("24h" or "12h").
    pub time_format: Option<String>,

    /// Maximum number of meetings in tooltip.
    pub tooltip_limit: Option<usize>,

    /// Whether to show all-day meetings in Waybar output.
    pub waybar_show_all_day: Option<bool>,

    /// Number of hours to treat all-day meetings as (for display).
    pub all_day_meeting_hours: Option<u32>,

    /// Custom command for opening URLs (e.g., "firefox", "open -a Safari").
    pub open_with: Option<String>,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            max_title_length: None,
            no_meeting_text: "No meeting".to_string(),
            format: None,
            tooltip_format: None,
            hour_separator: None,
            until_offset: None,
            time_format: None,
            tooltip_limit: None,
            waybar_show_all_day: None,
            all_day_meeting_hours: None,
            open_with: None,
        }
    }
}

/// Filter settings for meeting selection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterSettings {
    /// Only show meetings for today.
    pub today_only: bool,

    /// Maximum number of meetings to display.
    pub limit: Option<usize>,

    /// Skip all-day meetings.
    pub skip_all_day: bool,

    /// Only include meetings matching these title patterns.
    #[serde(default)]
    pub include_titles: Vec<String>,

    /// Exclude meetings matching these title patterns.
    #[serde(default)]
    pub exclude_titles: Vec<String>,

    /// Only include events from these calendars.
    #[serde(default)]
    pub include_calendars: Vec<String>,

    /// Exclude events from these calendars.
    #[serde(default)]
    pub exclude_calendars: Vec<String>,

    /// Only include events starting within N minutes.
    pub within_minutes: Option<u32>,

    /// Only include events within work hours (format: "HH:MM-HH:MM").
    pub work_hours: Option<String>,

    /// Only include events that have a meeting link.
    #[serde(default)]
    pub only_with_link: bool,

    /// Enable privacy mode.
    #[serde(default)]
    pub privacy: bool,

    /// Title to use when privacy mode is enabled.
    pub privacy_title: Option<String>,

    /// Skip events where the user has declined.
    pub skip_declined: bool,

    /// Skip events where the user has tentatively accepted.
    pub skip_tentative: bool,

    /// Skip events where the user hasn't responded yet.
    pub skip_pending: bool,

    /// Skip events without other attendees (solo events).
    pub skip_without_guests: bool,
}

/// Notification settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationSettings {
    /// Minutes before meetings to send notifications.
    #[serde(default)]
    pub minutes_before: Vec<u32>,

    /// Override urgency level ("low", "normal", "critical").
    pub urgency: Option<String>,

    /// Override notification expiry in seconds.
    pub expiry: Option<u32>,

    /// Custom notification icon path.
    pub icon: Option<String>,

    /// Time for morning agenda notification (format: "HH:MM").
    pub morning_agenda: Option<String>,

    /// Background color for "soon" notifications in Waybar.
    pub min_color: Option<String>,

    /// Foreground color for "soon" notifications in Waybar.
    pub min_color_foreground: Option<String>,

    /// Snooze duration in minutes (default: 10).
    pub snooze_minutes: Option<u32>,

    /// Minutes before meeting end to trigger end-warning behaviour.
    ///
    /// If not set, end-warning notifications and Waybar class changes are disabled.
    pub end_warning_minutes: Option<u32>,
}

impl NotificationSettings {
    /// Validates end-warning notification settings.
    pub fn validate_end_warning(&self) -> Result<(), String> {
        if matches!(self.end_warning_minutes, Some(0)) {
            return Err("notifications.end_warning_minutes must be greater than zero".to_string());
        }

        Ok(())
    }
}

/// Server/connection settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerSettings {
    /// Path to the server socket.
    pub socket_path: Option<PathBuf>,

    /// Connection timeout in seconds.
    pub timeout: u64,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            socket_path: None,
            timeout: 5,
        }
    }
}

/// Returns the XDG-style config directory on all platforms.
fn xdg_config_dir() -> Option<PathBuf> {
    dirs::config_dir()
}

/// Returns the XDG-style data directory on all platforms.
fn xdg_data_dir() -> Option<PathBuf> {
    dirs::data_dir()
}

impl ClientConfig {
    /// Loads configuration from the default path.
    pub fn load() -> Result<Self, String> {
        let path = Self::default_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("failed to read config: {}", e))?;
            toml::from_str(&content).map_err(|e| format!("failed to parse config: {}", e))
        } else {
            Ok(Self::default())
        }
    }

    /// Loads configuration from a specific path.
    pub fn load_from(path: &PathBuf) -> Result<Self, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("failed to read config: {}", e))?;
        toml::from_str(&content).map_err(|e| format!("failed to parse config: {}", e))
    }

    /// Returns the default configuration file path.
    pub fn default_path() -> PathBuf {
        Self::default_config_dir().join("config.toml")
    }

    /// Returns the default configuration directory.
    pub fn default_config_dir() -> PathBuf {
        xdg_config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nextmeeting")
    }

    /// Returns the default data directory path.
    pub fn default_data_dir() -> PathBuf {
        xdg_data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nextmeeting")
    }

    /// Returns the path for storing dismissed event IDs.
    pub fn dismissed_events_path() -> PathBuf {
        Self::default_data_dir().join("dismissed-events.json")
    }
}

// ---------------------------------------------------------------------------
// GoogleSettings (in config.toml, including credentials)
// ---------------------------------------------------------------------------

/// Google Calendar provider settings.
///
/// Supports multiple accounts via `[[google.accounts]]` array-of-tables.
#[cfg(feature = "google")]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoogleSettings {
    /// Google accounts, each with separate credentials and tokens.
    #[serde(default)]
    pub accounts: Vec<GoogleAccountSettings>,
}

/// Per-account Google Calendar settings.
///
/// Credentials (`client_id`, `client_secret`) support secret references
/// (`pass::…`, `env::…`).
#[cfg(feature = "google")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleAccountSettings {
    /// Account name (e.g. `"work"`, `"personal"`).
    ///
    /// Must be unique across accounts. Used in provider names (`google:work`)
    /// and default token file names (`google-tokens-work.json`).
    pub name: String,

    /// OAuth client ID (supports `pass::` and `env::` prefixes).
    pub client_id: Option<String>,

    /// OAuth client secret (supports `pass::` and `env::` prefixes).
    pub client_secret: Option<String>,

    /// Google Workspace domain (optional).
    pub domain: Option<String>,

    /// Calendar IDs to fetch.
    #[serde(default = "default_calendar_ids")]
    pub calendar_ids: Vec<String>,

    /// Path to token storage.
    ///
    /// Defaults to `google-tokens-{name}.json` in the config directory.
    pub token_path: Option<PathBuf>,
}

#[cfg(feature = "google")]
fn default_calendar_ids() -> Vec<String> {
    vec!["primary".to_string()]
}

#[cfg(feature = "google")]
impl GoogleSettings {
    /// Validates all accounts.
    pub fn validate(&self) -> Result<(), String> {
        // Check for duplicate account names
        let mut seen_names = std::collections::HashSet::new();
        let mut seen_token_paths = std::collections::HashSet::new();

        for account in &self.accounts {
            if !seen_names.insert(&account.name) {
                return Err(format!("duplicate Google account name: '{}'", account.name));
            }

            // Validate account name format
            if !account
                .name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                return Err(format!(
                    "account name '{}' contains invalid characters (only alphanumeric, hyphen, underscore allowed)",
                    account.name
                ));
            }

            if account.name.is_empty() {
                return Err("account name must not be empty".to_string());
            }

            // Check for duplicate token paths
            let token_path = account.effective_token_path();
            if !seen_token_paths.insert(token_path.clone()) {
                return Err(format!(
                    "duplicate token path across accounts: {}",
                    token_path.display()
                ));
            }
        }

        Ok(())
    }
}

#[cfg(feature = "google")]
impl GoogleAccountSettings {
    /// Returns the effective token path (explicit or default).
    pub fn effective_token_path(&self) -> PathBuf {
        self.token_path.clone().unwrap_or_else(|| {
            nextmeeting_providers::google::GoogleConfig::default_token_path(&self.name)
        })
    }

    /// Converts to provider configuration.
    ///
    /// Resolves credentials (expanding `pass::` / `env::` references) and
    /// builds a `GoogleConfig` suitable for the provider.
    pub fn to_provider_config(
        &self,
    ) -> Result<nextmeeting_providers::google::GoogleConfig, String> {
        use nextmeeting_providers::google::GoogleConfig;

        let credentials = self.resolve_credentials()?;
        credentials.validate().map_err(|e| e.to_string())?;

        let mut config = GoogleConfig::new(credentials).with_account_name(&self.name);

        if let Some(ref domain) = self.domain {
            config = config.with_domain(domain);
        }

        if !self.calendar_ids.is_empty() {
            config = config.with_calendar_ids(self.calendar_ids.clone());
        }

        if let Some(ref path) = self.token_path {
            config = config.with_token_path(path);
        }

        Ok(config)
    }

    /// Resolves Google OAuth credentials from inline fields.
    ///
    /// Both `client_id` and `client_secret` must be set. Each value is passed
    /// through `secret::resolve()` to expand `pass::` and `env::` references.
    pub(crate) fn resolve_credentials(
        &self,
    ) -> Result<nextmeeting_providers::google::OAuthCredentials, String> {
        use nextmeeting_providers::google::OAuthCredentials;

        let raw_id = self.client_id.as_deref().ok_or_else(|| {
            format!(
                "Google credentials not found for account '{}'. Add to {}:\n  \
                 [[google.accounts]]\n  \
                 name = \"{}\"\n  \
                 client_id = \"YOUR_ID.apps.googleusercontent.com\"\n  \
                 client_secret = \"YOUR_SECRET\"\n\n  \
                 Or run: nextmeeting auth google --account {} --credentials-file <path>",
                self.name,
                ClientConfig::default_path().display(),
                self.name,
                self.name,
            )
        })?;

        let raw_secret = self.client_secret.as_deref().ok_or_else(|| {
            format!(
                "client_secret is missing for account '{}' in config.toml",
                self.name,
            )
        })?;

        let resolved_id = crate::secret::resolve(raw_id)
            .map_err(|e| format!("failed to resolve client_id: {}", e))?;
        let resolved_secret = crate::secret::resolve(raw_secret)
            .map_err(|e| format!("failed to resolve client_secret: {}", e))?;

        Ok(OAuthCredentials::new(resolved_id, resolved_secret))
    }
}

#[cfg(test)]
#[cfg(feature = "google")]
mod tests {
    use super::*;

    fn test_account(name: &str) -> GoogleAccountSettings {
        GoogleAccountSettings {
            name: name.to_string(),
            client_id: Some("test-id.apps.googleusercontent.com".to_string()),
            client_secret: Some("test-secret".to_string()),
            domain: None,
            calendar_ids: vec!["primary".to_string()],
            token_path: None,
        }
    }

    #[test]
    fn resolve_credentials_plain_text() {
        let account = test_account("default");
        let creds = account.resolve_credentials().unwrap();
        assert_eq!(creds.client_id, "test-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "test-secret");
    }

    #[test]
    fn resolve_credentials_env_prefix() {
        unsafe {
            std::env::set_var("_NM_TEST_CLIENT_ID", "env-id.apps.googleusercontent.com");
            std::env::set_var("_NM_TEST_CLIENT_SECRET", "env-secret");
        }

        let account = GoogleAccountSettings {
            name: "test".to_string(),
            client_id: Some("env::_NM_TEST_CLIENT_ID".to_string()),
            client_secret: Some("env::_NM_TEST_CLIENT_SECRET".to_string()),
            domain: None,
            calendar_ids: vec!["primary".to_string()],
            token_path: None,
        };
        let creds = account.resolve_credentials().unwrap();
        assert_eq!(creds.client_id, "env-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "env-secret");

        unsafe {
            std::env::remove_var("_NM_TEST_CLIENT_ID");
            std::env::remove_var("_NM_TEST_CLIENT_SECRET");
        }
    }

    #[test]
    fn resolve_credentials_missing_id_errors() {
        let account = GoogleAccountSettings {
            name: "test".to_string(),
            client_id: None,
            client_secret: Some("secret".to_string()),
            domain: None,
            calendar_ids: vec!["primary".to_string()],
            token_path: None,
        };
        let result = account.resolve_credentials();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("credentials not found"));
    }

    #[test]
    fn resolve_credentials_missing_secret_errors() {
        let account = GoogleAccountSettings {
            name: "test".to_string(),
            client_id: Some("id.apps.googleusercontent.com".to_string()),
            client_secret: None,
            domain: None,
            calendar_ids: vec!["primary".to_string()],
            token_path: None,
        };
        let result = account.resolve_credentials();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("client_secret"));
    }

    #[test]
    fn to_provider_config_with_inline_credentials() {
        let account = GoogleAccountSettings {
            name: "work".to_string(),
            client_id: Some("test.apps.googleusercontent.com".to_string()),
            client_secret: Some("test-secret".to_string()),
            domain: Some("example.com".to_string()),
            calendar_ids: vec!["cal1".to_string(), "cal2".to_string()],
            token_path: None,
        };
        let config = account.to_provider_config().unwrap();
        assert_eq!(
            config.credentials.client_id,
            "test.apps.googleusercontent.com"
        );
        assert_eq!(config.credentials.client_secret, "test-secret");
        assert_eq!(config.domain, Some("example.com".to_string()));
        assert_eq!(
            config.calendar_ids,
            vec!["cal1".to_string(), "cal2".to_string()]
        );
        assert_eq!(config.account_name, "work");
    }

    #[test]
    fn config_toml_with_accounts() {
        let toml_content = r#"
[[google.accounts]]
name = "work"
client_id = "work-id.apps.googleusercontent.com"
client_secret = "work-secret"
calendar_ids = ["primary"]

[[google.accounts]]
name = "personal"
client_id = "personal-id.apps.googleusercontent.com"
client_secret = "personal-secret"
"#;
        let config: ClientConfig = toml::from_str(toml_content).unwrap();
        let google = config.google.unwrap();
        assert_eq!(google.accounts.len(), 2);
        assert_eq!(google.accounts[0].name, "work");
        assert_eq!(
            google.accounts[0].client_id,
            Some("work-id.apps.googleusercontent.com".to_string())
        );
        assert_eq!(google.accounts[1].name, "personal");

        let provider_config = google.accounts[0].to_provider_config().unwrap();
        assert_eq!(
            provider_config.credentials.client_id,
            "work-id.apps.googleusercontent.com"
        );
        assert_eq!(provider_config.account_name, "work");
    }

    #[test]
    fn config_toml_empty_google_section() {
        let toml_content = "[google]\n";
        let config: ClientConfig = toml::from_str(toml_content).unwrap();
        let google = config.google.unwrap();
        assert!(google.accounts.is_empty());
    }

    #[test]
    fn config_toml_with_env_references() {
        unsafe {
            std::env::set_var("_NM_TOML_TEST_ID", "env-toml-id.apps.googleusercontent.com");
            std::env::set_var("_NM_TOML_TEST_SECRET", "env-toml-secret");
        }

        let toml_content = r#"
[[google.accounts]]
name = "test"
client_id = "env::_NM_TOML_TEST_ID"
client_secret = "env::_NM_TOML_TEST_SECRET"
"#;
        let config: ClientConfig = toml::from_str(toml_content).unwrap();
        let google = config.google.unwrap();
        let creds = google.accounts[0].resolve_credentials().unwrap();
        assert_eq!(creds.client_id, "env-toml-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "env-toml-secret");

        unsafe {
            std::env::remove_var("_NM_TOML_TEST_ID");
            std::env::remove_var("_NM_TOML_TEST_SECRET");
        }
    }

    #[test]
    fn validate_duplicate_account_names() {
        let settings = GoogleSettings {
            accounts: vec![test_account("work"), test_account("work")],
        };
        let result = settings.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("duplicate"));
    }

    #[test]
    fn validate_invalid_account_name() {
        let mut account = test_account("work");
        account.name = "has spaces".to_string();
        let settings = GoogleSettings {
            accounts: vec![account],
        };
        let result = settings.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));
    }

    #[test]
    fn validate_empty_account_name() {
        let mut account = test_account("default");
        account.name = String::new();
        let settings = GoogleSettings {
            accounts: vec![account],
        };
        let result = settings.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn validate_valid_accounts() {
        let settings = GoogleSettings {
            accounts: vec![test_account("work"), test_account("personal")],
        };
        assert!(settings.validate().is_ok());
    }
}

#[cfg(test)]
mod notification_settings_tests {
    use super::NotificationSettings;

    #[test]
    fn end_warning_missing_value_is_allowed() {
        let settings = NotificationSettings::default();
        assert!(settings.validate_end_warning().is_ok());
    }

    #[test]
    fn end_warning_rejects_zero() {
        let settings = NotificationSettings {
            end_warning_minutes: Some(0),
            ..NotificationSettings::default()
        };
        let err = settings.validate_end_warning().unwrap_err();
        assert!(err.contains("end_warning_minutes"));
    }

    #[test]
    fn end_warning_accepts_positive_value() {
        let settings = NotificationSettings {
            end_warning_minutes: Some(5),
            ..NotificationSettings::default()
        };
        assert!(settings.validate_end_warning().is_ok());
    }
}
