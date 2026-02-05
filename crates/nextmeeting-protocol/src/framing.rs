//! Length-prefixed message framing for IPC.
//!
//! Messages are framed with a 4-byte big-endian length prefix followed by
//! the JSON payload:
//!
//! ```text
//! +----------------+------------------+
//! | length (4 BE)  |  JSON payload    |
//! +----------------+------------------+
//! ```

use std::io::{Read, Write};

use serde::{Serialize, de::DeserializeOwned};

use crate::MAX_MESSAGE_SIZE;
use crate::error::{ProtocolError, ProtocolResult};

/// Encodes a message to bytes with length prefix.
///
/// Returns the complete framed message ready for transmission.
///
/// # Example
///
/// ```rust
/// use nextmeeting_protocol::{encode_message, Request, Envelope};
///
/// let envelope = Envelope::request("req-1", Request::Ping);
/// let bytes = encode_message(&envelope).unwrap();
/// assert!(bytes.len() > 4); // At least length prefix
/// ```
pub fn encode_message<T: Serialize>(message: &T) -> ProtocolResult<Vec<u8>> {
    let json = serde_json::to_vec(message)?;
    let len = json.len() as u32;

    if len > MAX_MESSAGE_SIZE {
        return Err(ProtocolError::MessageTooLarge {
            size: len,
            max: MAX_MESSAGE_SIZE,
        });
    }

    let mut buffer = Vec::with_capacity(4 + json.len());
    buffer.extend_from_slice(&len.to_be_bytes());
    buffer.extend_from_slice(&json);
    Ok(buffer)
}

/// Decodes a message from bytes with length prefix.
///
/// The input should be a complete framed message (length prefix + payload).
///
/// # Example
///
/// ```rust
/// use nextmeeting_protocol::{encode_message, decode_message, Request, Envelope};
///
/// let envelope = Envelope::request("req-1", Request::Ping);
/// let bytes = encode_message(&envelope).unwrap();
/// let decoded: Envelope<Request> = decode_message(&bytes).unwrap();
/// assert_eq!(decoded.request_id, "req-1");
/// ```
pub fn decode_message<T: DeserializeOwned>(data: &[u8]) -> ProtocolResult<T> {
    if data.len() < 4 {
        return Err(ProtocolError::IncompleteMessage {
            expected: 4,
            received: data.len(),
        });
    }

    let len_bytes: [u8; 4] = data[0..4].try_into().unwrap();
    let len = u32::from_be_bytes(len_bytes) as usize;

    if len > MAX_MESSAGE_SIZE as usize {
        return Err(ProtocolError::MessageTooLarge {
            size: len as u32,
            max: MAX_MESSAGE_SIZE,
        });
    }

    if data.len() < 4 + len {
        return Err(ProtocolError::IncompleteMessage {
            expected: 4 + len,
            received: data.len(),
        });
    }

    let json = &data[4..4 + len];
    let message = serde_json::from_slice(json)?;
    Ok(message)
}

/// Reads framed messages from a byte stream.
///
/// This struct wraps a reader and provides methods to read complete messages.
pub struct FrameReader<R> {
    reader: R,
}

impl<R: Read> FrameReader<R> {
    /// Creates a new FrameReader wrapping the given reader.
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    /// Reads a single framed message.
    ///
    /// Returns `Ok(None)` if the stream is empty (EOF before any bytes).
    /// Returns an error if the message is incomplete or malformed.
    pub fn read_message<T: DeserializeOwned>(&mut self) -> ProtocolResult<Option<T>> {
        // Read length prefix
        let mut len_buf = [0u8; 4];
        match self.reader.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        }

        let len = u32::from_be_bytes(len_buf) as usize;

        if len > MAX_MESSAGE_SIZE as usize {
            return Err(ProtocolError::MessageTooLarge {
                size: len as u32,
                max: MAX_MESSAGE_SIZE,
            });
        }

        if len == 0 {
            return Err(ProtocolError::EmptyMessage);
        }

        // Read payload
        let mut payload = vec![0u8; len];
        self.reader.read_exact(&mut payload)?;

        let message = serde_json::from_slice(&payload)?;
        Ok(Some(message))
    }

    /// Returns a reference to the underlying reader.
    pub fn get_ref(&self) -> &R {
        &self.reader
    }

    /// Returns a mutable reference to the underlying reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Unwraps this FrameReader, returning the underlying reader.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

/// Writes framed messages to a byte stream.
///
/// This struct wraps a writer and provides methods to write complete messages.
pub struct FrameWriter<W> {
    writer: W,
}

impl<W: Write> FrameWriter<W> {
    /// Creates a new FrameWriter wrapping the given writer.
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    /// Writes a single framed message.
    pub fn write_message<T: Serialize>(&mut self, message: &T) -> ProtocolResult<()> {
        let data = encode_message(message)?;
        self.writer.write_all(&data)?;
        Ok(())
    }

    /// Flushes the underlying writer.
    pub fn flush(&mut self) -> ProtocolResult<()> {
        self.writer.flush()?;
        Ok(())
    }

    /// Returns a reference to the underlying writer.
    pub fn get_ref(&self) -> &W {
        &self.writer
    }

    /// Returns a mutable reference to the underlying writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Unwraps this FrameWriter, returning the underlying writer.
    pub fn into_inner(self) -> W {
        self.writer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Envelope, Request, Response};
    use std::io::Cursor;

    #[test]
    fn encode_decode_roundtrip() {
        let envelope = Envelope::request("req-123", Request::Ping);
        let bytes = encode_message(&envelope).unwrap();

        // Verify length prefix
        let len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert_eq!(len as usize, bytes.len() - 4);

        let decoded: Envelope<Request> = decode_message(&bytes).unwrap();
        assert_eq!(envelope, decoded);
    }

    #[test]
    fn decode_incomplete_length() {
        let result: ProtocolResult<Envelope<Request>> = decode_message(&[0, 0]);
        assert!(matches!(
            result,
            Err(ProtocolError::IncompleteMessage { expected: 4, .. })
        ));
    }

    #[test]
    fn decode_incomplete_payload() {
        // Claim 100 bytes but only provide 10
        let mut data = vec![0, 0, 0, 100];
        data.extend_from_slice(&[0u8; 10]);

        let result: ProtocolResult<Envelope<Request>> = decode_message(&data);
        assert!(matches!(
            result,
            Err(ProtocolError::IncompleteMessage { .. })
        ));
    }

    #[test]
    fn message_too_large() {
        // Create a message claiming to be larger than MAX_MESSAGE_SIZE
        let huge_len = MAX_MESSAGE_SIZE + 1;
        let data = huge_len.to_be_bytes();

        let result: ProtocolResult<Envelope<Request>> = decode_message(&data);
        assert!(matches!(result, Err(ProtocolError::MessageTooLarge { .. })));
    }

    #[test]
    fn frame_reader_single_message() {
        let envelope = Envelope::request("req-1", Request::Ping);
        let bytes = encode_message(&envelope).unwrap();

        let mut reader = FrameReader::new(Cursor::new(bytes));
        let decoded: Option<Envelope<Request>> = reader.read_message().unwrap();

        assert!(decoded.is_some());
        assert_eq!(decoded.unwrap(), envelope);
    }

    #[test]
    fn frame_reader_empty_stream() {
        let mut reader = FrameReader::new(Cursor::new(Vec::new()));
        let result: Option<Envelope<Request>> = reader.read_message().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn frame_reader_multiple_messages() {
        let msg1 = Envelope::request("req-1", Request::Ping);
        let msg2 = Envelope::request("req-2", Request::Status);

        let mut bytes = encode_message(&msg1).unwrap();
        bytes.extend(encode_message(&msg2).unwrap());

        let mut reader = FrameReader::new(Cursor::new(bytes));

        let decoded1: Envelope<Request> = reader.read_message().unwrap().unwrap();
        let decoded2: Envelope<Request> = reader.read_message().unwrap().unwrap();

        assert_eq!(decoded1, msg1);
        assert_eq!(decoded2, msg2);
    }

    #[test]
    fn frame_writer_single_message() {
        let envelope = Envelope::response("req-1", Response::Pong);
        let mut buffer = Vec::new();

        {
            let mut writer = FrameWriter::new(&mut buffer);
            writer.write_message(&envelope).unwrap();
            writer.flush().unwrap();
        }

        let decoded: Envelope<Response> = decode_message(&buffer).unwrap();
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn frame_reader_writer_roundtrip() {
        let requests = vec![
            Envelope::request("1", Request::Ping),
            Envelope::request("2", Request::Status),
            Envelope::request("3", Request::refresh(true)),
        ];

        let mut buffer = Vec::new();

        // Write all messages
        {
            let mut writer = FrameWriter::new(&mut buffer);
            for req in &requests {
                writer.write_message(req).unwrap();
            }
            writer.flush().unwrap();
        }

        // Read all messages
        let mut reader = FrameReader::new(Cursor::new(buffer));
        for expected in &requests {
            let actual: Envelope<Request> = reader.read_message().unwrap().unwrap();
            assert_eq!(&actual, expected);
        }

        // Should be at EOF
        let eof: Option<Envelope<Request>> = reader.read_message().unwrap();
        assert!(eof.is_none());
    }

    #[test]
    fn frame_reader_empty_message_error() {
        // Encode a message with zero length
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&0u32.to_be_bytes());

        let mut reader = FrameReader::new(Cursor::new(buffer));
        let result: ProtocolResult<Option<Envelope<Request>>> = reader.read_message();
        assert!(matches!(result, Err(ProtocolError::EmptyMessage)));
    }
}
