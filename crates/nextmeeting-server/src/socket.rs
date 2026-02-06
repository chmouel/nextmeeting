//! Unix socket listener for IPC.
//!
//! This module provides an async Unix socket server that handles
//! client connections using the nextmeeting protocol.

use std::path::Path;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use nextmeeting_protocol::{
    Envelope, MAX_MESSAGE_SIZE, PROTOCOL_VERSION, ProtocolError, Request, Response,
};

use crate::config::ServerConfig;
use crate::error::{ServerError, ServerResult};

/// Unix socket server for handling client connections.
pub struct SocketServer {
    /// Server configuration.
    config: ServerConfig,
    /// Unix socket listener.
    listener: UnixListener,
    /// Semaphore for limiting concurrent connections.
    connection_semaphore: Arc<Semaphore>,
}

impl SocketServer {
    /// Creates a new socket server with the given configuration.
    ///
    /// This will bind to the socket path specified in the configuration.
    /// If `cleanup_stale_socket` is true, it will attempt to remove any
    /// existing socket file before binding.
    pub async fn new(config: ServerConfig) -> ServerResult<Self> {
        let socket_path = &config.socket_path;

        // Check if parent directory exists
        if let Some(parent) = socket_path.parent()
            && !parent.exists()
        {
            return Err(ServerError::socket_path_invalid(
                parent.to_string_lossy().to_string(),
            ));
        }

        // Clean up stale socket if configured
        if config.cleanup_stale_socket && socket_path.exists() {
            // Try to connect to see if it's a live socket
            match tokio::net::UnixStream::connect(socket_path).await {
                Ok(_) => {
                    // Socket is live, another server is running
                    return Err(ServerError::socket_in_use(
                        socket_path.to_string_lossy().to_string(),
                    ));
                }
                Err(_) => {
                    // Socket is stale, remove it
                    info!(
                        path = %socket_path.display(),
                        "Removing stale socket"
                    );
                    std::fs::remove_file(socket_path)?;
                }
            }
        } else if socket_path.exists() {
            return Err(ServerError::socket_in_use(
                socket_path.to_string_lossy().to_string(),
            ));
        }

        // Bind to the socket
        let listener = UnixListener::bind(socket_path)?;
        info!(
            path = %socket_path.display(),
            "Socket server listening"
        );

        let connection_semaphore = Arc::new(Semaphore::new(config.max_connections));

        Ok(Self {
            config,
            listener,
            connection_semaphore,
        })
    }

    /// Returns the socket path.
    pub fn socket_path(&self) -> &Path {
        &self.config.socket_path
    }

    /// Accepts a single connection.
    ///
    /// Returns a `Connection` that can be used to read requests and write responses.
    pub async fn accept(&self) -> ServerResult<Connection> {
        let permit = self.connection_semaphore.clone().acquire_owned().await;
        let permit = permit.expect("semaphore should not be closed");

        let (stream, _addr) = self.listener.accept().await?;
        debug!("Accepted new connection");

        Ok(Connection {
            stream,
            timeout: self.config.connection_timeout,
            _permit: permit,
        })
    }

    /// Runs the server accept loop, calling the handler for each connection.
    ///
    /// This method runs indefinitely until an error occurs or the server is stopped.
    pub async fn run<F, Fut>(&self, handler: F) -> ServerResult<()>
    where
        F: Fn(Connection) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        loop {
            match self.accept().await {
                Ok(connection) => {
                    let fut = handler(connection);
                    tokio::spawn(fut);
                }
                Err(e) => {
                    error!(error = %e, "Failed to accept connection");
                    // Continue accepting despite errors
                }
            }
        }
    }

    /// Runs the server accept loop with a shutdown signal.
    ///
    /// The server will stop when the shutdown future completes.
    pub async fn run_until_shutdown<F, Fut, S>(&self, handler: F, shutdown: S) -> ServerResult<()>
    where
        F: Fn(Connection) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
        S: std::future::Future<Output = ()> + Send,
    {
        tokio::select! {
            result = self.run(handler) => result,
            _ = shutdown => {
                info!("Shutdown signal received");
                Ok(())
            }
        }
    }
}

impl Drop for SocketServer {
    fn drop(&mut self) {
        // Clean up the socket file
        if self.config.socket_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.config.socket_path) {
                warn!(
                    path = %self.config.socket_path.display(),
                    error = %e,
                    "Failed to remove socket file"
                );
            } else {
                debug!(
                    path = %self.config.socket_path.display(),
                    "Removed socket file"
                );
            }
        }
    }
}

/// A client connection to the server.
pub struct Connection {
    stream: UnixStream,
    timeout: std::time::Duration,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl Connection {
    /// Reads a request envelope from the connection.
    ///
    /// Returns `Ok(None)` if the connection was closed cleanly.
    pub async fn read_request(&mut self) -> ServerResult<Option<Envelope<Request>>> {
        // Read length prefix (4 bytes, big-endian)
        let mut len_buf = [0u8; 4];
        match tokio::time::timeout(self.timeout, self.stream.read_exact(&mut len_buf)).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None);
            }
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {
                return Err(ServerError::Protocol(ProtocolError::Timeout {
                    operation: "read request length".to_string(),
                }));
            }
        }

        let len = u32::from_be_bytes(len_buf) as usize;

        if len > MAX_MESSAGE_SIZE as usize {
            return Err(ServerError::Protocol(ProtocolError::MessageTooLarge {
                size: len as u32,
                max: MAX_MESSAGE_SIZE,
            }));
        }

        if len == 0 {
            return Err(ServerError::Protocol(ProtocolError::EmptyMessage));
        }

        // Read payload
        let mut payload = vec![0u8; len];
        match tokio::time::timeout(self.timeout, self.stream.read_exact(&mut payload)).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {
                return Err(ServerError::Protocol(ProtocolError::Timeout {
                    operation: "read request payload".to_string(),
                }));
            }
        }

        let envelope: Envelope<Request> =
            serde_json::from_slice(&payload).map_err(nextmeeting_protocol::ProtocolError::from)?;

        // Validate protocol version
        if !envelope.is_compatible() {
            warn!(
                version = %envelope.protocol_version,
                expected = %PROTOCOL_VERSION,
                "Incompatible protocol version"
            );
        }

        Ok(Some(envelope))
    }

    /// Writes a response envelope to the connection.
    pub async fn write_response(&mut self, envelope: &Envelope<Response>) -> ServerResult<()> {
        let json =
            serde_json::to_vec(envelope).map_err(nextmeeting_protocol::ProtocolError::from)?;

        let len = json.len() as u32;
        if len > MAX_MESSAGE_SIZE {
            return Err(ServerError::Protocol(ProtocolError::MessageTooLarge {
                size: len,
                max: MAX_MESSAGE_SIZE,
            }));
        }

        let mut buffer = Vec::with_capacity(4 + json.len());
        buffer.extend_from_slice(&len.to_be_bytes());
        buffer.extend_from_slice(&json);

        match tokio::time::timeout(self.timeout, self.stream.write_all(&buffer)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {
                return Err(ServerError::Protocol(ProtocolError::Timeout {
                    operation: "write response".to_string(),
                }));
            }
        }

        Ok(())
    }

    /// Sends a response for the given request.
    pub async fn respond(
        &mut self,
        request_id: impl Into<String>,
        response: Response,
    ) -> ServerResult<()> {
        let envelope = Envelope::response(request_id, response);
        self.write_response(&envelope).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;

    #[tokio::test]
    async fn socket_server_creates_socket_file() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

        let config = ServerConfig::new(&socket_path);
        let server = SocketServer::new(config).await.unwrap();

        assert!(socket_path.exists());
        drop(server);
        assert!(!socket_path.exists());
    }

    #[tokio::test]
    async fn socket_server_rejects_duplicate() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

        let config = ServerConfig::new(&socket_path).with_cleanup_stale_socket(false);
        let _server = SocketServer::new(config.clone()).await.unwrap();

        // Second server should fail
        let result = SocketServer::new(config).await;
        assert!(matches!(result, Err(ServerError::SocketInUse { .. })));
    }

    #[tokio::test]
    async fn socket_server_cleans_stale_socket() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

        // Create a stale socket file (not a real socket)
        std::fs::write(&socket_path, b"stale").unwrap();

        let config = ServerConfig::new(&socket_path).with_cleanup_stale_socket(true);
        let server = SocketServer::new(config).await.unwrap();

        assert!(socket_path.exists());
        drop(server);
    }

    #[tokio::test]
    async fn connection_roundtrip() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

        let config =
            ServerConfig::new(&socket_path).with_connection_timeout(Duration::from_secs(5));
        let server = SocketServer::new(config).await.unwrap();

        // Spawn a client task
        let socket_path_clone = socket_path.clone();
        let client_task = tokio::spawn(async move {
            let mut stream = tokio::net::UnixStream::connect(&socket_path_clone)
                .await
                .unwrap();

            // Send a ping request
            let request = Envelope::request("test-1", Request::Ping);
            let json = serde_json::to_vec(&request).unwrap();
            let len = (json.len() as u32).to_be_bytes();
            AsyncWriteExt::write_all(&mut stream, &len).await.unwrap();
            AsyncWriteExt::write_all(&mut stream, &json).await.unwrap();

            // Read response
            let mut len_buf = [0u8; 4];
            AsyncReadExt::read_exact(&mut stream, &mut len_buf)
                .await
                .unwrap();
            let len = u32::from_be_bytes(len_buf) as usize;

            let mut payload = vec![0u8; len];
            AsyncReadExt::read_exact(&mut stream, &mut payload)
                .await
                .unwrap();

            let response: Envelope<Response> = serde_json::from_slice(&payload).unwrap();
            assert_eq!(response.request_id, "test-1");
            assert_eq!(response.payload, Response::Pong);
        });

        // Accept and handle the connection
        let mut conn = server.accept().await.unwrap();
        let request = conn.read_request().await.unwrap().unwrap();
        assert_eq!(request.payload, Request::Ping);

        conn.respond(&request.request_id, Response::Pong)
            .await
            .unwrap();

        client_task.await.unwrap();
    }

    #[tokio::test]
    async fn connection_handles_client_disconnect() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

        let config = ServerConfig::new(&socket_path);
        let server = SocketServer::new(config).await.unwrap();

        // Connect and immediately disconnect
        let socket_path_clone = socket_path.clone();
        let handle = tokio::spawn(async move {
            let _stream: tokio::net::UnixStream =
                tokio::net::UnixStream::connect(&socket_path_clone)
                    .await
                    .unwrap();
            // Stream dropped, connection closed
        });

        let mut conn = server.accept().await.unwrap();

        // Wait for client to disconnect
        handle.await.unwrap();

        // Read should return None (clean EOF)
        let result = conn.read_request().await.unwrap();
        assert!(result.is_none());
    }
}
