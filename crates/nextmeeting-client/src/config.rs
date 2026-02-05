//! Client configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Configuration for the nextmeeting client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    /// Google Calendar settings.
    #[cfg(feature = "google")]
    pub google: Option<GoogleSettings>,

    /// Debug mode.
    pub debug: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            #[cfg(feature = "google")]
            google: None,
            debug: false,
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
    pub fn to_provider_config(
        &self,
    ) -> Result<nextmeeting_providers::google::GoogleConfig, String> {
        use nextmeeting_providers::google::{GoogleConfig, OAuthCredentials};

        let credentials = OAuthCredentials::new(&self.client_id, &self.client_secret);
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
}
