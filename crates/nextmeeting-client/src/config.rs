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
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            max_title_length: None,
            no_meeting_text: "No meeting".to_string(),
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
}

/// Notification settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationSettings {
    /// Minutes before meetings to send notifications.
    #[serde(default)]
    pub minutes_before: Vec<u32>,
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
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nextmeeting")
    }

    /// Returns the default data directory path.
    pub fn default_data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nextmeeting")
    }
}

// ---------------------------------------------------------------------------
// GoogleSettings (in config.toml, including credentials)
// ---------------------------------------------------------------------------

/// Google Calendar provider settings.
///
/// Credentials (`client_id`, `client_secret`) are stored inline and support
/// secret references (`pass::…`, `env::…`).
#[cfg(feature = "google")]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoogleSettings {
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
    pub token_path: Option<PathBuf>,
}

#[cfg(feature = "google")]
fn default_calendar_ids() -> Vec<String> {
    vec!["primary".to_string()]
}

#[cfg(feature = "google")]
impl GoogleSettings {
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

        let mut config = GoogleConfig::new(credentials);

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
                "Google credentials not found. Add to {}:\n  \
                 [google]\n  \
                 client_id = \"YOUR_ID.apps.googleusercontent.com\"\n  \
                 client_secret = \"YOUR_SECRET\"\n\n  \
                 Or run: nextmeeting auth google --credentials-file <path>",
                ClientConfig::default_path().display()
            )
        })?;

        let raw_secret = self.client_secret.as_deref().ok_or_else(|| {
            "client_secret is missing from [google] section in config.toml".to_string()
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

    #[test]
    fn resolve_credentials_plain_text() {
        let settings = GoogleSettings {
            client_id: Some("test-id.apps.googleusercontent.com".to_string()),
            client_secret: Some("test-secret".to_string()),
            ..Default::default()
        };
        let creds = settings.resolve_credentials().unwrap();
        assert_eq!(creds.client_id, "test-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "test-secret");
    }

    #[test]
    fn resolve_credentials_env_prefix() {
        unsafe {
            std::env::set_var(
                "_NM_TEST_CLIENT_ID",
                "env-id.apps.googleusercontent.com",
            );
            std::env::set_var("_NM_TEST_CLIENT_SECRET", "env-secret");
        }

        let settings = GoogleSettings {
            client_id: Some("env::_NM_TEST_CLIENT_ID".to_string()),
            client_secret: Some("env::_NM_TEST_CLIENT_SECRET".to_string()),
            ..Default::default()
        };
        let creds = settings.resolve_credentials().unwrap();
        assert_eq!(creds.client_id, "env-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "env-secret");

        unsafe {
            std::env::remove_var("_NM_TEST_CLIENT_ID");
            std::env::remove_var("_NM_TEST_CLIENT_SECRET");
        }
    }

    #[test]
    fn resolve_credentials_missing_id_errors() {
        let settings = GoogleSettings {
            client_secret: Some("secret".to_string()),
            ..Default::default()
        };
        let result = settings.resolve_credentials();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("credentials not found"));
    }

    #[test]
    fn resolve_credentials_missing_secret_errors() {
        let settings = GoogleSettings {
            client_id: Some("id.apps.googleusercontent.com".to_string()),
            ..Default::default()
        };
        let result = settings.resolve_credentials();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("client_secret"));
    }

    #[test]
    fn resolve_credentials_both_missing_errors() {
        let settings = GoogleSettings::default();
        let result = settings.resolve_credentials();
        assert!(result.is_err());
    }

    #[test]
    fn to_provider_config_with_inline_credentials() {
        let settings = GoogleSettings {
            client_id: Some("test.apps.googleusercontent.com".to_string()),
            client_secret: Some("test-secret".to_string()),
            domain: Some("example.com".to_string()),
            calendar_ids: vec!["cal1".to_string(), "cal2".to_string()],
            token_path: None,
        };
        let config = settings.to_provider_config().unwrap();
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
    }

    #[test]
    fn config_toml_with_inline_credentials() {
        let toml_content = r#"
[google]
client_id = "toml-id.apps.googleusercontent.com"
client_secret = "toml-secret"
calendar_ids = ["primary"]
"#;
        let config: ClientConfig = toml::from_str(toml_content).unwrap();
        let google = config.google.unwrap();
        assert_eq!(
            google.client_id,
            Some("toml-id.apps.googleusercontent.com".to_string())
        );
        assert_eq!(google.client_secret, Some("toml-secret".to_string()));

        let provider_config = google.to_provider_config().unwrap();
        assert_eq!(
            provider_config.credentials.client_id,
            "toml-id.apps.googleusercontent.com"
        );
    }

    #[test]
    fn config_toml_bare_google_section_errors() {
        let toml_content = "[google]\n";
        let config: ClientConfig = toml::from_str(toml_content).unwrap();
        let google = config.google.unwrap();
        let result = google.resolve_credentials();
        assert!(result.is_err());
    }

    #[test]
    fn config_toml_with_env_references() {
        unsafe {
            std::env::set_var(
                "_NM_TOML_TEST_ID",
                "env-toml-id.apps.googleusercontent.com",
            );
            std::env::set_var("_NM_TOML_TEST_SECRET", "env-toml-secret");
        }

        let toml_content = r#"
[google]
client_id = "env::_NM_TOML_TEST_ID"
client_secret = "env::_NM_TOML_TEST_SECRET"
"#;
        let config: ClientConfig = toml::from_str(toml_content).unwrap();
        let google = config.google.unwrap();
        let creds = google.resolve_credentials().unwrap();
        assert_eq!(creds.client_id, "env-toml-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "env-toml-secret");

        unsafe {
            std::env::remove_var("_NM_TOML_TEST_ID");
            std::env::remove_var("_NM_TOML_TEST_SECRET");
        }
    }
}
