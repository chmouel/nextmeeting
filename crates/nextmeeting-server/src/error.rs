//! Server error types.

use std::io;
use thiserror::Error;

/// Result type for server operations.
pub type ServerResult<T> = Result<T, ServerError>;

/// Errors that can occur in the server.
#[derive(Debug, Error)]
pub enum ServerError {
    /// IO error (socket, file, etc.).
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// Protocol error (framing, encoding, etc.).
    #[error("Protocol error: {0}")]
    Protocol(#[from] nextmeeting_protocol::ProtocolError),

    /// Socket path already in use.
    #[error("Socket path already in use: {path}")]
    SocketInUse { path: String },

    /// Socket path parent directory does not exist.
    #[error("Socket path parent directory does not exist: {path}")]
    SocketPathInvalid { path: String },

    /// Server is already running.
    #[error("Server is already running (PID file exists: {path})")]
    AlreadyRunning { path: String },

    /// Configuration error.
    #[error("Configuration error: {message}")]
    Config { message: String },

    /// Shutdown requested.
    #[error("Server shutdown requested")]
    Shutdown,
}

impl ServerError {
    /// Creates a configuration error.
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config {
            message: message.into(),
        }
    }

    /// Creates a socket in use error.
    pub fn socket_in_use(path: impl Into<String>) -> Self {
        Self::SocketInUse { path: path.into() }
    }

    /// Creates a socket path invalid error.
    pub fn socket_path_invalid(path: impl Into<String>) -> Self {
        Self::SocketPathInvalid { path: path.into() }
    }

    /// Creates an already running error.
    pub fn already_running(path: impl Into<String>) -> Self {
        Self::AlreadyRunning { path: path.into() }
    }
}
