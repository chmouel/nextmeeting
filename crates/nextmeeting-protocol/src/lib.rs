//! IPC framing and request/response types for nextmeeting.
//!
//! This module defines the Protocol v1 for communication between the
//! nextmeeting client and server over Unix sockets.
//!
//! # Protocol Overview
//!
//! Messages are sent as length-prefixed JSON:
//! - 4 bytes: message length (u32, big-endian)
//! - N bytes: JSON payload
//!
//! # Envelope Structure
//!
//! Every message is wrapped in an [`Envelope`] containing:
//! - `protocol_version`: Always "1" for this version
//! - `request_id`: UUID for request/response correlation
//! - `payload`: The actual request or response
//!
//! # Example
//!
//! ```rust
//! use nextmeeting_protocol::{Envelope, Request, encode_message, decode_message};
//!
//! let request = Envelope::request("req-123", Request::Ping);
//! let bytes = encode_message(&request).unwrap();
//! let decoded: Envelope<Request> = decode_message(&bytes).unwrap();
//! ```

mod error;
mod framing;
mod types;

pub use error::{ProtocolError, ProtocolResult};
pub use framing::{decode_message, encode_message, FrameReader, FrameWriter};
pub use types::{
    Envelope, ErrorCode, ErrorResponse, MeetingsFilter, ProviderStatus, Request, Response,
    StatusInfo,
};

/// Protocol version constant.
pub const PROTOCOL_VERSION: &str = "1";

/// Maximum message size (1 MB).
pub const MAX_MESSAGE_SIZE: u32 = 1024 * 1024;
