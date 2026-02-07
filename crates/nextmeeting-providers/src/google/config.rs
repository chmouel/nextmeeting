//! Google Calendar provider configuration.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

/// OAuth 2.0 credentials for Google API access.
///
/// Users must provide their own OAuth client ID and secret, as Google
/// requires registered applications for API access.
#[derive(Debug, Clone)]
pub struct OAuthCredentials {
    /// The OAuth 2.0 client ID from Google Cloud Console.
    pub client_id: String,
    /// The OAuth 2.0 client secret from Google Cloud Console.
    pub client_secret: String,
}

/// Structure of Google's OAuth credentials JSON file.
///
/// Supports multiple formats:
/// 1. Google Cloud Console format with "installed" or "web" section
/// 2. Flat format with client_id and client_secret at root level (e.g., from gcloud)
#[derive(Debug, Deserialize)]
pub struct GoogleCredentialsFile {
    /// Credentials for installed (desktop) applications.
    pub installed: Option<NestedCredentials>,
    /// Credentials for web applications.
    pub web: Option<NestedCredentials>,
    /// Direct client_id (flat format).
    pub client_id: Option<String>,
    /// Direct client_secret (flat format).
    pub client_secret: Option<String>,
}

/// OAuth credentials within a nested section of the credentials JSON file.
#[derive(Debug, Deserialize)]
pub struct NestedCredentials {
    /// The OAuth 2.0 client ID.
    pub client_id: String,
    /// The OAuth 2.0 client secret.
    pub client_secret: String,
    /// The project ID (optional, present in the JSON but not used).
    #[serde(default)]
    #[allow(dead_code)]
    pub project_id: Option<String>,
}

impl OAuthCredentials {
    /// Creates new OAuth credentials.
    pub fn new(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
        }
    }

    /// Loads OAuth credentials from a Google Cloud Console JSON file.
    ///
    /// The file should be the JSON downloaded from the Google Cloud Console
    /// OAuth 2.0 credentials page. It contains either an "installed" or "web"
    /// section with the client_id and client_secret.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("failed to read credentials file: {}", e))?;
        Self::from_json(&content)
    }

    /// Parses OAuth credentials from a Google credentials JSON string.
    ///
    /// Supports multiple formats:
    /// 1. Google Cloud Console format: `{"installed": {"client_id": "...", "client_secret": "..."}}`
    /// 2. Flat format: `{"client_id": "...", "client_secret": "..."}`
    pub fn from_json(json: &str) -> Result<Self, String> {
        let file: GoogleCredentialsFile = serde_json::from_str(json)
            .map_err(|e| format!("failed to parse credentials JSON: {}", e))?;

        // Try nested format first (installed or web section)
        if let Some(creds) = file.installed.or(file.web) {
            return Ok(Self::new(creds.client_id, creds.client_secret));
        }

        // Try flat format (client_id and client_secret at root level)
        if let (Some(client_id), Some(client_secret)) = (file.client_id, file.client_secret) {
            return Ok(Self::new(client_id, client_secret));
        }

        Err("credentials file must contain 'installed'/'web' section or 'client_id'/'client_secret' at root level".to_string())
    }

    /// Validates that the credentials appear to be correctly formatted.
    ///
    /// This checks that:
    /// - Client ID ends with `.apps.googleusercontent.com`
    /// - Client secret is non-empty
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.client_id.is_empty() {
            return Err("client_id is required");
        }
        if !self.client_id.ends_with(".apps.googleusercontent.com") {
            return Err("client_id should end with .apps.googleusercontent.com");
        }
        if self.client_secret.is_empty() {
            return Err("client_secret is required");
        }
        Ok(())
    }
}

/// Configuration for the Google Calendar provider.
#[derive(Debug, Clone)]
pub struct GoogleConfig {
    /// Account name for multi-account support.
    ///
    /// Used to distinguish between multiple Google accounts.
    /// Defaults to `"default"`.
    pub account_name: String,

    /// OAuth credentials for API access.
    pub credentials: OAuthCredentials,

    /// Google Workspace domain for URL generation.
    ///
    /// When set, calendar URLs will use `calendar.google.com/a/DOMAIN/`
    /// instead of the default `calendar.google.com/`.
    pub domain: Option<String>,

    /// Path to store OAuth tokens.
    ///
    /// Defaults to `~/.local/share/nextmeeting/google-tokens-{account}.json`.
    pub token_path: PathBuf,

    /// Specific calendar IDs to fetch from.
    ///
    /// If empty, fetches from the primary calendar.
    pub calendar_ids: Vec<String>,

    /// Request timeout.
    pub timeout: Duration,

    /// User agent string for API requests.
    pub user_agent: String,

    /// Port range for the loopback OAuth server.
    ///
    /// The OAuth flow will try to bind to ports in this range.
    /// Defaults to (8080, 8090).
    pub loopback_port_range: (u16, u16),

    /// OAuth scopes to request.
    ///
    /// Defaults to `["https://www.googleapis.com/auth/calendar.readonly"]`.
    pub scopes: Vec<String>,
}

impl GoogleConfig {
    /// Default timeout in seconds.
    pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

    /// Default OAuth scope for read-only calendar access.
    pub const DEFAULT_SCOPE: &'static str = "https://www.googleapis.com/auth/calendar.readonly";

    /// Creates a new Google configuration with the given credentials.
    pub fn new(credentials: OAuthCredentials) -> Self {
        Self {
            account_name: "default".to_string(),
            credentials,
            domain: None,
            token_path: Self::default_token_path("default"),
            calendar_ids: vec!["primary".to_string()],
            timeout: Duration::from_secs(Self::DEFAULT_TIMEOUT_SECS),
            user_agent: format!("nextmeeting/{}", env!("CARGO_PKG_VERSION")),
            loopback_port_range: (8080, 8090),
            scopes: vec![Self::DEFAULT_SCOPE.to_string()],
        }
    }

    /// Returns the default token storage path for a given account name.
    pub fn default_token_path(account_name: &str) -> PathBuf {
        let data_dir = dirs::home_dir()
            .map(|h| h.join(".local").join("share"))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nextmeeting");
        data_dir.join(format!("google-tokens-{}.json", account_name))
    }

    /// Sets the account name.
    pub fn with_account_name(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        // Update token_path to match the new account name if it's still the default
        let old_default = Self::default_token_path(&self.account_name);
        if self.token_path == old_default {
            self.token_path = Self::default_token_path(&name);
        }
        self.account_name = name;
        self
    }

    /// Returns the provider name for this account (e.g. `"google:work"`).
    pub fn provider_name(&self) -> String {
        format!("google:{}", self.account_name)
    }

    /// Sets the Google Workspace domain.
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Sets the token storage path.
    pub fn with_token_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.token_path = path.into();
        self
    }

    /// Sets the calendar IDs to fetch from.
    pub fn with_calendar_ids(mut self, ids: Vec<String>) -> Self {
        self.calendar_ids = ids;
        self
    }

    /// Adds a calendar ID to fetch from.
    pub fn with_calendar_id(mut self, id: impl Into<String>) -> Self {
        self.calendar_ids.push(id.into());
        self
    }

    /// Sets the request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the user agent string.
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Sets the loopback port range for OAuth.
    pub fn with_loopback_port_range(mut self, start: u16, end: u16) -> Self {
        self.loopback_port_range = (start, end);
        self
    }

    /// Sets the OAuth scopes.
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Returns the calendar URL for a given date.
    ///
    /// If a domain is configured, returns a workspace URL.
    pub fn calendar_url(&self, date: &chrono::NaiveDate) -> String {
        let date_str = date.format("%Y/%m/%d").to_string();
        match &self.domain {
            Some(domain) => format!(
                "https://calendar.google.com/calendar/b/{}/r/day/{date_str}",
                domain
            ),
            None => format!("https://calendar.google.com/calendar/r/day/{date_str}"),
        }
    }

    /// Validates the configuration.
    pub fn validate(&self) -> Result<(), String> {
        self.credentials
            .validate()
            .map_err(|e| format!("invalid credentials: {}", e))?;

        if self.scopes.is_empty() {
            return Err("at least one OAuth scope is required".to_string());
        }

        if self.loopback_port_range.0 > self.loopback_port_range.1 {
            return Err("invalid loopback port range".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_credentials() -> OAuthCredentials {
        OAuthCredentials::new("test-client.apps.googleusercontent.com", "test-secret")
    }

    #[test]
    fn credentials_validation() {
        let valid = test_credentials();
        assert!(valid.validate().is_ok());

        let empty_id = OAuthCredentials::new("", "secret");
        assert!(empty_id.validate().is_err());

        let bad_id = OAuthCredentials::new("bad-id", "secret");
        assert!(bad_id.validate().is_err());

        let empty_secret = OAuthCredentials::new("test.apps.googleusercontent.com", "");
        assert!(empty_secret.validate().is_err());
    }

    #[test]
    fn config_creation() {
        let config = GoogleConfig::new(test_credentials());
        assert!(config.domain.is_none());
        assert_eq!(config.calendar_ids, vec!["primary".to_string()]);
        assert_eq!(config.scopes, vec![GoogleConfig::DEFAULT_SCOPE.to_string()]);
    }

    #[test]
    fn config_with_domain() {
        let config = GoogleConfig::new(test_credentials()).with_domain("example.com");
        assert_eq!(config.domain, Some("example.com".to_string()));
    }

    #[test]
    fn calendar_url_without_domain() {
        let config = GoogleConfig::new(test_credentials());
        let date = chrono::NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let url = config.calendar_url(&date);
        assert_eq!(url, "https://calendar.google.com/calendar/r/day/2024/03/15");
    }

    #[test]
    fn calendar_url_with_domain() {
        let config = GoogleConfig::new(test_credentials()).with_domain("example.com");
        let date = chrono::NaiveDate::from_ymd_opt(2024, 3, 15).unwrap();
        let url = config.calendar_url(&date);
        assert_eq!(
            url,
            "https://calendar.google.com/calendar/b/example.com/r/day/2024/03/15"
        );
    }

    #[test]
    fn config_validation() {
        let config = GoogleConfig::new(test_credentials());
        assert!(config.validate().is_ok());

        let bad_config = GoogleConfig::new(test_credentials()).with_scopes(vec![]);
        assert!(bad_config.validate().is_err());
    }

    #[test]
    fn config_builder_methods() {
        let config = GoogleConfig::new(test_credentials())
            .with_domain("example.com")
            .with_calendar_ids(vec!["cal1".to_string()])
            .with_calendar_id("cal2")
            .with_timeout(Duration::from_secs(60))
            .with_loopback_port_range(9000, 9010);

        assert_eq!(config.domain, Some("example.com".to_string()));
        assert_eq!(
            config.calendar_ids,
            vec!["cal1".to_string(), "cal2".to_string()]
        );
        assert_eq!(config.timeout, Duration::from_secs(60));
        assert_eq!(config.loopback_port_range, (9000, 9010));
    }

    #[test]
    fn credentials_from_json_installed() {
        let json = r#"{
            "installed": {
                "client_id": "test-id.apps.googleusercontent.com",
                "client_secret": "test-secret",
                "project_id": "my-project"
            }
        }"#;

        let creds = OAuthCredentials::from_json(json).unwrap();
        assert_eq!(creds.client_id, "test-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "test-secret");
    }

    #[test]
    fn credentials_from_json_web() {
        let json = r#"{
            "web": {
                "client_id": "web-id.apps.googleusercontent.com",
                "client_secret": "web-secret"
            }
        }"#;

        let creds = OAuthCredentials::from_json(json).unwrap();
        assert_eq!(creds.client_id, "web-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "web-secret");
    }

    #[test]
    fn credentials_from_json_flat() {
        // Format used by gcloud and other tools
        let json = r#"{
            "client_id": "flat-id.apps.googleusercontent.com",
            "client_secret": "flat-secret",
            "token": "some-token",
            "refresh_token": "some-refresh-token"
        }"#;

        let creds = OAuthCredentials::from_json(json).unwrap();
        assert_eq!(creds.client_id, "flat-id.apps.googleusercontent.com");
        assert_eq!(creds.client_secret, "flat-secret");
    }

    #[test]
    fn credentials_from_json_invalid() {
        let json = r#"{ "other": {} }"#;
        let result = OAuthCredentials::from_json(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("client_id"));
    }

    #[test]
    fn credentials_from_json_malformed() {
        let json = "not json";
        let result = OAuthCredentials::from_json(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("parse"));
    }
}
