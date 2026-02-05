//! CalDAV provider configuration.

use std::time::Duration;
use url::Url;

/// Configuration for the CalDAV provider.
#[derive(Debug, Clone)]
pub struct CalDavConfig {
    /// Base URL of the CalDAV server (principal or calendar collection).
    pub url: Url,

    /// Username for authentication.
    pub username: Option<String>,

    /// Password for authentication.
    pub password: Option<String>,

    /// Specific calendar path/name to use (if not using the URL directly).
    pub calendar_hint: Option<String>,

    /// Hours to look behind for ongoing events.
    pub lookbehind_hours: u32,

    /// Hours to look ahead for upcoming events.
    pub lookahead_hours: u32,

    /// Whether to verify TLS certificates.
    pub verify_tls: bool,

    /// Request timeout.
    pub timeout: Duration,

    /// User agent string.
    pub user_agent: String,
}

impl CalDavConfig {
    /// Default lookbehind hours.
    pub const DEFAULT_LOOKBEHIND_HOURS: u32 = 12;

    /// Default lookahead hours.
    pub const DEFAULT_LOOKAHEAD_HOURS: u32 = 48;

    /// Default timeout in seconds.
    pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

    /// Creates a new CalDAV configuration with the given URL.
    ///
    /// # Errors
    ///
    /// Returns an error if the URL is invalid.
    pub fn new(url: impl AsRef<str>) -> Result<Self, url::ParseError> {
        let parsed = Url::parse(url.as_ref())?;
        Ok(Self {
            url: parsed,
            username: None,
            password: None,
            calendar_hint: None,
            lookbehind_hours: Self::DEFAULT_LOOKBEHIND_HOURS,
            lookahead_hours: Self::DEFAULT_LOOKAHEAD_HOURS,
            verify_tls: true,
            timeout: Duration::from_secs(Self::DEFAULT_TIMEOUT_SECS),
            user_agent: format!("nextmeeting/{}", env!("CARGO_PKG_VERSION")),
        })
    }

    /// Sets the credentials for authentication.
    pub fn with_credentials(
        mut self,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }

    /// Sets the calendar hint (specific calendar name or path).
    pub fn with_calendar_hint(mut self, hint: impl Into<String>) -> Self {
        self.calendar_hint = Some(hint.into());
        self
    }

    /// Sets the lookbehind hours.
    pub fn with_lookbehind_hours(mut self, hours: u32) -> Self {
        self.lookbehind_hours = hours;
        self
    }

    /// Sets the lookahead hours.
    pub fn with_lookahead_hours(mut self, hours: u32) -> Self {
        self.lookahead_hours = hours;
        self
    }

    /// Disables TLS verification (for testing only).
    pub fn with_insecure_tls(mut self) -> Self {
        self.verify_tls = false;
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

    /// Returns the base URL as a string.
    pub fn url_str(&self) -> &str {
        self.url.as_str()
    }

    /// Returns true if credentials are configured.
    pub fn has_credentials(&self) -> bool {
        self.username.is_some() && self.password.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_creation() {
        let config = CalDavConfig::new("https://caldav.example.com/calendars/user/").unwrap();
        assert_eq!(
            config.url.as_str(),
            "https://caldav.example.com/calendars/user/"
        );
        assert!(!config.has_credentials());
        assert!(config.verify_tls);
    }

    #[test]
    fn config_with_credentials() {
        let config = CalDavConfig::new("https://caldav.example.com/")
            .unwrap()
            .with_credentials("user", "pass");

        assert!(config.has_credentials());
        assert_eq!(config.username, Some("user".to_string()));
        assert_eq!(config.password, Some("pass".to_string()));
    }

    #[test]
    fn config_builder_methods() {
        let config = CalDavConfig::new("https://caldav.example.com/")
            .unwrap()
            .with_calendar_hint("work")
            .with_lookbehind_hours(6)
            .with_lookahead_hours(72)
            .with_insecure_tls()
            .with_timeout(Duration::from_secs(60));

        assert_eq!(config.calendar_hint, Some("work".to_string()));
        assert_eq!(config.lookbehind_hours, 6);
        assert_eq!(config.lookahead_hours, 72);
        assert!(!config.verify_tls);
        assert_eq!(config.timeout, Duration::from_secs(60));
    }

    #[test]
    fn invalid_url_returns_error() {
        let result = CalDavConfig::new("not a valid url");
        assert!(result.is_err());
    }
}
