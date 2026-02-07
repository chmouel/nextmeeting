//! Client configuration.
//!
//! Configuration is split across two files:
//!
//! - `config.toml` — non-secret settings (display, filters, notifications, server, google
//!   calendar IDs, domain, etc.)
//! - `auth.yaml` — sensitive credentials (OAuth client_id/client_secret)
//!
//! Both files live in `~/.config/nextmeeting/` by default.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ClientConfig (config.toml)
// ---------------------------------------------------------------------------

/// Configuration for the nextmeeting client.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    /// Google Calendar settings (non-secret).
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
// AuthConfig (auth.yaml)
// ---------------------------------------------------------------------------

/// Authentication credentials loaded from `auth.yaml`.
///
/// This file holds sensitive OAuth credentials separately from the main config.
///
/// # Format
///
/// ```yaml
/// google:
///   client_id: "xxx.apps.googleusercontent.com"
///   client_secret: "xxx"
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Google OAuth credentials.
    #[cfg(feature = "google")]
    pub google: Option<GoogleAuthCredentials>,
}

/// Google OAuth credentials stored in `auth.yaml`.
#[cfg(feature = "google")]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoogleAuthCredentials {
    /// OAuth client ID.
    pub client_id: String,

    /// OAuth client secret.
    pub client_secret: String,
}

impl AuthConfig {
    /// Loads auth config from the default path (`~/.config/nextmeeting/auth.yaml`).
    pub fn load() -> Result<Self, String> {
        let path = Self::default_path();
        if path.exists() {
            Self::load_from(&path)
        } else {
            Ok(Self::default())
        }
    }

    /// Loads auth config from a specific path.
    pub fn load_from(path: &PathBuf) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read auth config: {}", e))?;
        serde_yaml::from_str(&content).map_err(|e| format!("failed to parse auth config: {}", e))
    }

    /// Saves auth config to the default path.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::default_path();
        Self::save_to(self, &path)
    }

    /// Saves auth config to a specific path.
    pub fn save_to(&self, path: &PathBuf) -> Result<(), String> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create directory {}: {}", parent.display(), e))?;
        }

        let yaml = serde_yaml::to_string(self)
            .map_err(|e| format!("failed to serialize auth config: {}", e))?;
        std::fs::write(path, yaml)
            .map_err(|e| format!("failed to write auth config to {}: {}", path.display(), e))?;

        // Set restrictive permissions on the auth file (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(path, perms);
        }

        Ok(())
    }

    /// Returns the default auth config file path (`~/.config/nextmeeting/auth.yaml`).
    pub fn default_path() -> PathBuf {
        ClientConfig::default_config_dir().join("auth.yaml")
    }
}

// ---------------------------------------------------------------------------
// GoogleSettings (non-secret, in config.toml)
// ---------------------------------------------------------------------------

/// Google Calendar provider settings (non-secret).
///
/// OAuth credentials (`client_id`, `client_secret`) are stored separately
/// in `auth.yaml`. This struct holds only non-secret configuration like
/// domain, calendar IDs, and token path.
#[cfg(feature = "google")]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoogleSettings {
    /// Path to Google Cloud Console credentials JSON file.
    ///
    /// This is the JSON file downloaded from the Google Cloud Console OAuth 2.0
    /// credentials page. If provided, client_id and client_secret are extracted
    /// from this file instead of `auth.yaml`.
    pub credentials_file: Option<PathBuf>,

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
    /// Resolves credentials using the following priority:
    /// 1. `auth.yaml` (`client_id` + `client_secret`)
    /// 2. `credentials_file` from config.toml
    /// 3. Default credentials file at `~/.local/share/nextmeeting/oauth.json`
    pub fn to_provider_config(
        &self,
        auth: &AuthConfig,
    ) -> Result<nextmeeting_providers::google::GoogleConfig, String> {
        use nextmeeting_providers::google::GoogleConfig;

        let credentials = self.resolve_credentials(auth)?;
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

    /// Resolves Google OAuth credentials from multiple sources.
    ///
    /// Priority (highest to lowest):
    /// 1. `auth.yaml` (`client_id` + `client_secret`)
    /// 2. `credentials_file` path from config.toml
    /// 3. Default credentials file at `~/.local/share/nextmeeting/oauth.json`
    pub(crate) fn resolve_credentials(
        &self,
        auth: &AuthConfig,
    ) -> Result<nextmeeting_providers::google::OAuthCredentials, String> {
        use nextmeeting_providers::google::OAuthCredentials;

        // Priority 1: auth.yaml client_id + client_secret
        if let Some(ref google_auth) = auth.google {
            if !google_auth.client_id.is_empty() && !google_auth.client_secret.is_empty() {
                return Ok(OAuthCredentials::new(
                    &google_auth.client_id,
                    &google_auth.client_secret,
                ));
            }
        }

        // Priority 2: credentials_file from config.toml
        if let Some(ref path) = self.credentials_file {
            return OAuthCredentials::from_file(path)
                .map_err(|e| format!("failed to load credentials from {}: {}", path.display(), e));
        }

        // Priority 3: Default credentials file
        let default_path = ClientConfig::default_data_dir().join("oauth.json");
        if default_path.exists() {
            return OAuthCredentials::from_file(&default_path).map_err(|e| {
                format!(
                    "failed to load credentials from {}: {}",
                    default_path.display(),
                    e
                )
            });
        }

        let auth_path = AuthConfig::default_path();
        Err(format!(
            "Google credentials not found. Provide via:\n  \
             - client_id + client_secret in {}\n  \
             - credentials_file in config.toml [google] section\n  \
             - Place credentials JSON at ~/.local/share/nextmeeting/oauth.json\n  \
             - Run: nextmeeting auth google --credentials-file <path>",
            auth_path.display()
        ))
    }
}

#[cfg(test)]
#[cfg(feature = "google")]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: create a temp credentials JSON file with valid Google OAuth format.
    fn write_credentials_file(dir: &std::path::Path, filename: &str) -> PathBuf {
        let path = dir.join(filename);
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"{{
  "installed": {{
    "client_id": "file-id.apps.googleusercontent.com",
    "client_secret": "file-secret"
  }}
}}"#
        )
        .unwrap();
        path
    }

    /// Helper: create an AuthConfig with inline credentials.
    fn auth_with_creds(client_id: &str, client_secret: &str) -> AuthConfig {
        AuthConfig {
            google: Some(GoogleAuthCredentials {
                client_id: client_id.to_string(),
                client_secret: client_secret.to_string(),
            }),
        }
    }

    /// Helper: create an empty AuthConfig (no credentials).
    fn auth_empty() -> AuthConfig {
        AuthConfig::default()
    }

    #[test]
    fn resolve_credentials_from_auth_yaml() {
        let auth = auth_with_creds("auth-id.apps.googleusercontent.com", "auth-secret");
        let settings = GoogleSettings::default();
        let creds = settings.resolve_credentials(&auth).unwrap();
        assert_eq!(creds.client_id, "auth-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "auth-secret");
    }

    #[test]
    fn resolve_credentials_from_credentials_file() {
        let tmp = tempfile::tempdir().unwrap();
        let creds_path = write_credentials_file(tmp.path(), "creds.json");

        let auth = auth_empty();
        let settings = GoogleSettings {
            credentials_file: Some(creds_path),
            ..Default::default()
        };
        let creds = settings.resolve_credentials(&auth).unwrap();
        assert_eq!(creds.client_id, "file-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "file-secret");
    }

    #[test]
    fn resolve_credentials_auth_yaml_takes_priority_over_file() {
        let tmp = tempfile::tempdir().unwrap();
        let creds_path = write_credentials_file(tmp.path(), "creds.json");

        let auth = auth_with_creds("auth-id.apps.googleusercontent.com", "auth-secret");
        let settings = GoogleSettings {
            credentials_file: Some(creds_path),
            ..Default::default()
        };
        let creds = settings.resolve_credentials(&auth).unwrap();
        assert_eq!(creds.client_id, "auth-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "auth-secret");
    }

    #[test]
    fn resolve_credentials_missing_file_errors() {
        let auth = auth_empty();
        let settings = GoogleSettings {
            credentials_file: Some(PathBuf::from("/nonexistent/path/creds.json")),
            ..Default::default()
        };
        let result = settings.resolve_credentials(&auth);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to load credentials"));
    }

    #[test]
    fn resolve_credentials_no_sources_errors() {
        // Skip if default credentials file exists (would be used as fallback)
        if ClientConfig::default_data_dir().join("oauth.json").exists() {
            return;
        }
        let auth = auth_empty();
        let settings = GoogleSettings::default();
        let result = settings.resolve_credentials(&auth);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("credentials not found"));
    }

    #[test]
    fn to_provider_config_with_credentials_file() {
        let tmp = tempfile::tempdir().unwrap();
        let creds_path = write_credentials_file(tmp.path(), "creds.json");

        let auth = auth_empty();
        let settings = GoogleSettings {
            credentials_file: Some(creds_path),
            domain: Some("example.com".to_string()),
            calendar_ids: vec!["cal1".to_string(), "cal2".to_string()],
            token_path: None,
        };
        let config = settings.to_provider_config(&auth).unwrap();
        assert_eq!(
            config.credentials.client_id,
            "file-id.apps.googleusercontent.com"
        );
        assert_eq!(config.credentials.client_secret, "file-secret");
        assert_eq!(config.domain, Some("example.com".to_string()));
        assert_eq!(
            config.calendar_ids,
            vec!["cal1".to_string(), "cal2".to_string()]
        );
    }

    #[test]
    fn to_provider_config_with_auth_yaml_credentials() {
        let auth = auth_with_creds("test.apps.googleusercontent.com", "test-secret");
        let settings = GoogleSettings {
            calendar_ids: vec!["primary".to_string()],
            ..Default::default()
        };
        let config = settings.to_provider_config(&auth).unwrap();
        assert_eq!(
            config.credentials.client_id,
            "test.apps.googleusercontent.com"
        );
        assert_eq!(config.credentials.client_secret, "test-secret");
    }

    #[test]
    fn config_toml_with_credentials_file_only() {
        let tmp = tempfile::tempdir().unwrap();
        let creds_path = write_credentials_file(tmp.path(), "creds.json");

        let toml_content = format!(
            r#"[google]
credentials_file = "{}"
"#,
            creds_path.display()
        );
        let config: ClientConfig = toml::from_str(&toml_content).unwrap();
        let google = config.google.unwrap();
        assert_eq!(google.credentials_file, Some(creds_path.clone()));

        // Should successfully resolve credentials from the file
        let auth = auth_empty();
        let provider_config = google.to_provider_config(&auth).unwrap();
        assert_eq!(
            provider_config.credentials.client_id,
            "file-id.apps.googleusercontent.com"
        );
    }

    #[test]
    fn config_toml_bare_google_section_no_default_file() {
        // Skip if default credentials file exists (would be used as fallback)
        if ClientConfig::default_data_dir().join("oauth.json").exists() {
            return;
        }
        let toml_content = "[google]\n";
        let config: ClientConfig = toml::from_str(toml_content).unwrap();
        let google = config.google.unwrap();
        let auth = auth_empty();
        let result = google.resolve_credentials(&auth);
        assert!(result.is_err());
    }

    #[test]
    fn auth_yaml_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let auth_path = tmp.path().join("auth.yaml");

        let auth = auth_with_creds("test.apps.googleusercontent.com", "test-secret");
        auth.save_to(&auth_path).unwrap();

        let loaded = AuthConfig::load_from(&auth_path).unwrap();
        let google = loaded.google.unwrap();
        assert_eq!(google.client_id, "test.apps.googleusercontent.com");
        assert_eq!(google.client_secret, "test-secret");
    }

    #[test]
    fn auth_yaml_file_permissions() {
        let tmp = tempfile::tempdir().unwrap();
        let auth_path = tmp.path().join("auth.yaml");

        let auth = auth_with_creds("id", "secret");
        auth.save_to(&auth_path).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&auth_path).unwrap().permissions();
            assert_eq!(perms.mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn auth_yaml_missing_file_returns_default() {
        let tmp = tempfile::tempdir().unwrap();
        let auth_path = tmp.path().join("nonexistent.yaml");

        // load_from should fail on missing file
        assert!(AuthConfig::load_from(&auth_path).is_err());

        // But load() returns default when default path doesn't exist
        // (we can't test this without mocking the default path)
    }
}
