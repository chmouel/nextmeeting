//! Error types for calendar provider operations.
//!
//! This module defines the error types that can occur when interacting with
//! calendar providers (Google Calendar, CalDAV, etc.).

use std::fmt;
use thiserror::Error;

/// The category of a provider error.
///
/// This enum provides a high-level classification of errors for use in
/// protocol responses and retry logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderErrorCode {
    /// Authentication failed or credentials are invalid/expired.
    AuthenticationFailed,
    /// Authorization failed - user lacks permission.
    AuthorizationFailed,
    /// Network error - connection failed, timeout, DNS resolution, etc.
    NetworkError,
    /// Rate limit exceeded - too many requests.
    RateLimited,
    /// Server returned an error (5xx status codes).
    ServerError,
    /// Invalid response from the server - parse error, unexpected format.
    InvalidResponse,
    /// Resource not found (404).
    NotFound,
    /// Request was invalid (400) - bad parameters, malformed request.
    BadRequest,
    /// Configuration error - missing or invalid config.
    ConfigurationError,
    /// Calendar-specific error - e.g., calendar not found, event conflicts.
    CalendarError,
    /// Internal provider error - unexpected state, bug.
    InternalError,
}

impl ProviderErrorCode {
    /// Returns true if this error is transient and the operation may be retried.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::NetworkError | Self::RateLimited | Self::ServerError
        )
    }

    /// Returns a human-readable name for this error code.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AuthenticationFailed => "authentication_failed",
            Self::AuthorizationFailed => "authorization_failed",
            Self::NetworkError => "network_error",
            Self::RateLimited => "rate_limited",
            Self::ServerError => "server_error",
            Self::InvalidResponse => "invalid_response",
            Self::NotFound => "not_found",
            Self::BadRequest => "bad_request",
            Self::ConfigurationError => "configuration_error",
            Self::CalendarError => "calendar_error",
            Self::InternalError => "internal_error",
        }
    }
}

impl fmt::Display for ProviderErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// An error that occurred while interacting with a calendar provider.
#[derive(Debug, Error)]
pub struct ProviderError {
    /// The error code categorizing this error.
    code: ProviderErrorCode,
    /// A human-readable message describing the error.
    message: String,
    /// The provider that generated this error (e.g., "google", "caldav").
    provider: Option<String>,
    /// The underlying cause of this error, if any.
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl ProviderError {
    /// Creates a new provider error with the given code and message.
    pub fn new(code: ProviderErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            provider: None,
            source: None,
        }
    }

    /// Creates an authentication error.
    pub fn authentication(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorCode::AuthenticationFailed, message)
    }

    /// Creates an authorization error.
    pub fn authorization(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorCode::AuthorizationFailed, message)
    }

    /// Creates a network error.
    pub fn network(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorCode::NetworkError, message)
    }

    /// Creates a rate limit error.
    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorCode::RateLimited, message)
    }

    /// Creates a server error.
    pub fn server(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorCode::ServerError, message)
    }

    /// Creates an invalid response error.
    pub fn invalid_response(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorCode::InvalidResponse, message)
    }

    /// Creates a not found error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorCode::NotFound, message)
    }

    /// Creates a bad request error.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorCode::BadRequest, message)
    }

    /// Creates a configuration error.
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorCode::ConfigurationError, message)
    }

    /// Creates a calendar-specific error.
    pub fn calendar(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorCode::CalendarError, message)
    }

    /// Creates an internal error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorCode::InternalError, message)
    }

    /// Sets the provider name for this error.
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    /// Sets the source error for this error.
    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.source = Some(Box::new(source));
        self
    }

    /// Returns the error code.
    pub fn code(&self) -> ProviderErrorCode {
        self.code
    }

    /// Returns the error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the provider name, if set.
    pub fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }

    /// Returns true if this error is transient and may be retried.
    pub fn is_retryable(&self) -> bool {
        self.code.is_retryable()
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref provider) = self.provider {
            write!(f, "[{}] ", provider)?;
        }
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// A specialized Result type for provider operations.
pub type ProviderResult<T> = Result<T, ProviderError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_retryable() {
        assert!(ProviderErrorCode::NetworkError.is_retryable());
        assert!(ProviderErrorCode::RateLimited.is_retryable());
        assert!(ProviderErrorCode::ServerError.is_retryable());
        assert!(!ProviderErrorCode::AuthenticationFailed.is_retryable());
        assert!(!ProviderErrorCode::NotFound.is_retryable());
    }

    #[test]
    fn error_code_display() {
        assert_eq!(
            ProviderErrorCode::AuthenticationFailed.as_str(),
            "authentication_failed"
        );
        assert_eq!(ProviderErrorCode::RateLimited.as_str(), "rate_limited");
    }

    #[test]
    fn provider_error_creation() {
        let err = ProviderError::authentication("token expired");
        assert_eq!(err.code(), ProviderErrorCode::AuthenticationFailed);
        assert_eq!(err.message(), "token expired");
        assert!(err.provider().is_none());
        assert!(!err.is_retryable());
    }

    #[test]
    fn provider_error_with_provider() {
        let err = ProviderError::network("connection timeout").with_provider("google");
        assert_eq!(err.code(), ProviderErrorCode::NetworkError);
        assert_eq!(err.provider(), Some("google"));
        assert!(err.is_retryable());
    }

    #[test]
    fn provider_error_display() {
        let err = ProviderError::rate_limited("too many requests").with_provider("caldav");
        let display = format!("{}", err);
        assert!(display.contains("[caldav]"));
        assert!(display.contains("rate_limited"));
        assert!(display.contains("too many requests"));
    }

    #[test]
    fn provider_error_with_source() {
        use std::error::Error;
        let io_err = std::io::Error::other("disk full");
        let err = ProviderError::internal("failed to cache").with_source(io_err);
        assert!(err.source().is_some());
    }
}
