//! Client configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Configuration for the nextmeeting client.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    /// Google Calendar settings.
    #[cfg(feature = "google")]
    pub google: Option<GoogleSettings>,

    /// Debug mode.
    pub debug: bool,
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
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nextmeeting")
            .join("config.toml")
    }

    /// Returns the default data directory path.
    pub fn default_data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nextmeeting")
    }
}

/// Google Calendar provider settings.
#[cfg(feature = "google")]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoogleSettings {
    /// OAuth client ID.
    #[serde(default)]
    pub client_id: String,

    /// OAuth client secret.
    #[serde(default)]
    pub client_secret: String,

    /// Path to Google Cloud Console credentials JSON file.
    ///
    /// This is the JSON file downloaded from the Google Cloud Console OAuth 2.0
    /// credentials page. If provided, client_id and client_secret are extracted
    /// from this file.
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
    /// 1. Inline `client_id` + `client_secret` from config
    /// 2. `credentials_file` from config
    /// 3. Default credentials file at `~/.local/share/nextmeeting/oauth.json`
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

    /// Resolves Google OAuth credentials from multiple sources.
    ///
    /// Priority (highest to lowest):
    /// 1. Inline `client_id` + `client_secret` from config
    /// 2. `credentials_file` path from config
    /// 3. Default credentials file at `~/.local/share/nextmeeting/oauth.json`
    pub(crate) fn resolve_credentials(
        &self,
    ) -> Result<nextmeeting_providers::google::OAuthCredentials, String> {
        use nextmeeting_providers::google::OAuthCredentials;

        // Priority 1: Inline client_id + client_secret
        if !self.client_id.is_empty() && !self.client_secret.is_empty() {
            return Ok(OAuthCredentials::new(&self.client_id, &self.client_secret));
        }

        // Priority 2: credentials_file from config
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

        Err("Google credentials not found. Provide via:\n  \
             - client_id + client_secret in config.toml [google] section\n  \
             - credentials_file in config.toml [google] section\n  \
             - Place credentials JSON at ~/.local/share/nextmeeting/oauth.json\n  \
             - Run: nextmeeting auth google --credentials-file <path>"
            .to_string())
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

    #[test]
    fn resolve_credentials_inline() {
        let settings = GoogleSettings {
            client_id: "inline-id.apps.googleusercontent.com".to_string(),
            client_secret: "inline-secret".to_string(),
            credentials_file: None,
            domain: None,
            calendar_ids: vec![],
            token_path: None,
        };
        let creds = settings.resolve_credentials().unwrap();
        assert_eq!(creds.client_id, "inline-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "inline-secret");
    }

    #[test]
    fn resolve_credentials_from_credentials_file() {
        let tmp = tempfile::tempdir().unwrap();
        let creds_path = write_credentials_file(tmp.path(), "creds.json");

        let settings = GoogleSettings {
            client_id: String::new(),
            client_secret: String::new(),
            credentials_file: Some(creds_path),
            domain: None,
            calendar_ids: vec![],
            token_path: None,
        };
        let creds = settings.resolve_credentials().unwrap();
        assert_eq!(creds.client_id, "file-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "file-secret");
    }

    #[test]
    fn resolve_credentials_inline_takes_priority_over_file() {
        let tmp = tempfile::tempdir().unwrap();
        let creds_path = write_credentials_file(tmp.path(), "creds.json");

        let settings = GoogleSettings {
            client_id: "inline-id.apps.googleusercontent.com".to_string(),
            client_secret: "inline-secret".to_string(),
            credentials_file: Some(creds_path),
            domain: None,
            calendar_ids: vec![],
            token_path: None,
        };
        let creds = settings.resolve_credentials().unwrap();
        assert_eq!(creds.client_id, "inline-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "inline-secret");
    }

    #[test]
    fn resolve_credentials_missing_file_errors() {
        let settings = GoogleSettings {
            client_id: String::new(),
            client_secret: String::new(),
            credentials_file: Some(PathBuf::from("/nonexistent/path/creds.json")),
            domain: None,
            calendar_ids: vec![],
            token_path: None,
        };
        let result = settings.resolve_credentials();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to load credentials"));
    }

    #[test]
    fn resolve_credentials_no_sources_errors() {
        // Skip if default credentials file exists (would be used as fallback)
        if ClientConfig::default_data_dir().join("oauth.json").exists() {
            return;
        }
        let settings = GoogleSettings {
            client_id: String::new(),
            client_secret: String::new(),
            credentials_file: None,
            domain: None,
            calendar_ids: vec![],
            token_path: None,
        };
        let result = settings.resolve_credentials();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("credentials not found"));
    }

    #[test]
    fn to_provider_config_with_credentials_file() {
        let tmp = tempfile::tempdir().unwrap();
        let creds_path = write_credentials_file(tmp.path(), "creds.json");

        let settings = GoogleSettings {
            client_id: String::new(),
            client_secret: String::new(),
            credentials_file: Some(creds_path),
            domain: Some("example.com".to_string()),
            calendar_ids: vec!["cal1".to_string(), "cal2".to_string()],
            token_path: None,
        };
        let config = settings.to_provider_config().unwrap();
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
    fn to_provider_config_with_inline_credentials() {
        let settings = GoogleSettings {
            client_id: "test.apps.googleusercontent.com".to_string(),
            client_secret: "test-secret".to_string(),
            credentials_file: None,
            domain: None,
            calendar_ids: vec!["primary".to_string()],
            token_path: None,
        };
        let config = settings.to_provider_config().unwrap();
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
        assert!(google.client_id.is_empty());
        assert!(google.client_secret.is_empty());
        assert_eq!(google.credentials_file, Some(creds_path.clone()));

        // Should successfully resolve credentials from the file
        let provider_config = google.to_provider_config().unwrap();
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
        let result = google.resolve_credentials();
        assert!(result.is_err());
    }
}
