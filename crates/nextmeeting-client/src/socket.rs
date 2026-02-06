//! Unix socket client for communicating with the nextmeeting daemon.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tracing::{debug, warn};
use uuid::Uuid;

use nextmeeting_protocol::{Envelope, Request, Response, MAX_MESSAGE_SIZE};

use crate::error::{ClientError, ClientResult};

/// Client for communicating with the nextmeeting server over a Unix socket.
pub struct SocketClient {
    socket_path: PathBuf,
    timeout: Duration,
}

impl SocketClient {
    /// Creates a new socket client.
    pub fn new(socket_path: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            socket_path: socket_path.into(),
            timeout,
        }
    }

    /// Creates a socket client with the default socket path.
    pub fn with_defaults() -> Self {
        Self::new(
            nextmeeting_server::default_socket_path(),
            Duration::from_secs(5),
        )
    }

    /// Returns the socket path.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Checks if the server socket exists.
    pub fn socket_exists(&self) -> bool {
        self.socket_path.exists()
    }

    /// Sends a request and waits for a response.
    pub async fn send(&self, request: Request) -> ClientResult<Response> {
        let request_id = Uuid::new_v4().to_string();
        let envelope = Envelope::request(&request_id, request);

        debug!(
            socket = %self.socket_path.display(),
            request_id = %request_id,
            "connecting to server"
        );

        // Connect with timeout
        let stream = tokio::time::timeout(self.timeout, UnixStream::connect(&self.socket_path))
            .await
            .map_err(|_| {
                ClientError::Connection(format!(
                    "connection timed out after {}s",
                    self.timeout.as_secs()
                ))
            })?
            .map_err(|e| {
                ClientError::Connection(format!(
                    "failed to connect to {}: {}",
                    self.socket_path.display(),
                    e
                ))
            })?;

        // Send request
        let response = self.exchange(stream, &envelope).await?;

        // Validate response correlation
        if response.request_id != request_id {
            warn!(
                expected = %request_id,
                received = %response.request_id,
                "response request_id mismatch"
            );
        }

        Ok(response.payload)
    }

    /// Performs the framed request-response exchange on a connected stream.
    async fn exchange(
        &self,
        mut stream: UnixStream,
        envelope: &Envelope<Request>,
    ) -> ClientResult<Envelope<Response>> {
        // Serialize to JSON
        let json = serde_json::to_vec(envelope).map_err(|e| {
            ClientError::Protocol(format!("failed to serialize request: {}", e))
        })?;

        let len = json.len() as u32;
        if len > MAX_MESSAGE_SIZE {
            return Err(ClientError::Protocol(format!(
                "request too large: {} bytes (max: {})",
                len, MAX_MESSAGE_SIZE
            )));
        }

        // Write length-prefixed message
        let write_result = tokio::time::timeout(self.timeout, async {
            stream.write_all(&len.to_be_bytes()).await?;
            stream.write_all(&json).await?;
            stream.flush().await?;
            Ok::<(), std::io::Error>(())
        })
        .await
        .map_err(|_| ClientError::Timeout("sending request".into()))?
        .map_err(|e| ClientError::Io(e));

        write_result?;

        debug!("request sent, waiting for response");

        // Read length-prefixed response
        let response = tokio::time::timeout(self.timeout, async {
            // Read 4-byte length prefix
            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).await?;
            let resp_len = u32::from_be_bytes(len_buf) as usize;

            if resp_len as u32 > MAX_MESSAGE_SIZE {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "response too large: {} bytes (max: {})",
                        resp_len, MAX_MESSAGE_SIZE
                    ),
                ));
            }

            // Read payload
            let mut payload = vec![0u8; resp_len];
            stream.read_exact(&mut payload).await?;

            Ok(payload)
        })
        .await
        .map_err(|_| ClientError::Timeout("reading response".into()))?
        .map_err(ClientError::Io)?;

        // Deserialize response
        let envelope: Envelope<Response> = serde_json::from_slice(&response).map_err(|e| {
            ClientError::Protocol(format!("failed to deserialize response: {}", e))
        })?;

        debug!(
            request_id = %envelope.request_id,
            "response received"
        );

        Ok(envelope)
    }

    /// Pings the server to check if it's alive.
    pub async fn ping(&self) -> ClientResult<bool> {
        match self.send(Request::Ping).await {
            Ok(Response::Pong) => Ok(true),
            Ok(_) => Ok(false),
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_client_creation() {
        let client = SocketClient::new("/tmp/test.sock", Duration::from_secs(10));
        assert_eq!(client.socket_path(), Path::new("/tmp/test.sock"));
        assert!(!client.socket_exists());
    }

    #[test]
    fn default_client() {
        let client = SocketClient::with_defaults();
        assert!(client
            .socket_path()
            .to_string_lossy()
            .contains("nextmeeting"));
    }
}
