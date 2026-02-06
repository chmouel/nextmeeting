//! Client error types.

use std::fmt;

/// Result type for client operations.
pub type ClientResult<T> = Result<T, ClientError>;

/// Errors that can occur in the client.
#[derive(Debug)]
pub enum ClientError {
    /// Configuration error.
    Config(String),
    /// Provider error.
    Provider(String),
    /// IO error.
    Io(std::io::Error),
    /// Authentication required.
    AuthRequired(String),
    /// Connection to server failed.
    Connection(String),
    /// Protocol/framing error.
    Protocol(String),
    /// Request timed out.
    Timeout(String),
    /// Action failed (open, copy, etc).
    Action(String),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(msg) => write!(f, "configuration error: {}", msg),
            Self::Provider(msg) => write!(f, "provider error: {}", msg),
            Self::Io(err) => write!(f, "IO error: {}", err),
            Self::AuthRequired(msg) => write!(f, "authentication required: {}", msg),
            Self::Connection(msg) => write!(f, "connection error: {}", msg),
            Self::Protocol(msg) => write!(f, "protocol error: {}", msg),
            Self::Timeout(msg) => write!(f, "timeout: {}", msg),
            Self::Action(msg) => write!(f, "action failed: {}", msg),
        }
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ClientError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

#[cfg(feature = "google")]
impl From<nextmeeting_providers::ProviderError> for ClientError {
    fn from(err: nextmeeting_providers::ProviderError) -> Self {
        Self::Provider(err.to_string())
    }
}
