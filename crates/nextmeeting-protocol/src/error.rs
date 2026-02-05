//! Protocol error types.

use thiserror::Error;

/// Result type for protocol operations.
pub type ProtocolResult<T> = Result<T, ProtocolError>;

/// Errors that can occur during protocol operations.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// Message exceeds maximum allowed size.
    #[error("message too large: {size} bytes (max: {max})")]
    MessageTooLarge { size: u32, max: u32 },

    /// Failed to serialize message to JSON.
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid protocol version in message.
    #[error("unsupported protocol version: {0}")]
    UnsupportedVersion(String),

    /// IO error during read/write.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Incomplete message (connection closed before full message received).
    #[error("incomplete message: expected {expected} bytes, got {received}")]
    IncompleteMessage { expected: usize, received: usize },

    /// Empty message received.
    #[error("empty message")]
    EmptyMessage,

    /// Operation timed out.
    #[error("timeout during {operation}")]
    Timeout { operation: String },
}
