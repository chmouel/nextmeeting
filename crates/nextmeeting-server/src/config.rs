//! Server configuration.

use std::path::PathBuf;
use std::time::Duration;

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Path to the Unix socket.
    pub socket_path: PathBuf,

    /// Connection timeout.
    pub connection_timeout: Duration,

    /// Maximum concurrent connections.
    pub max_connections: usize,

    /// Whether to remove stale socket on startup.
    pub cleanup_stale_socket: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
            connection_timeout: Duration::from_secs(30),
            max_connections: 100,
            cleanup_stale_socket: true,
        }
    }
}

impl ServerConfig {
    /// Creates a new server configuration with the given socket path.
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            ..Default::default()
        }
    }

    /// Builder: set connection timeout.
    pub fn with_connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// Builder: set max connections.
    pub fn with_max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }

    /// Builder: set cleanup stale socket.
    pub fn with_cleanup_stale_socket(mut self, cleanup: bool) -> Self {
        self.cleanup_stale_socket = cleanup;
        self
    }
}

/// Returns the default socket path.
///
/// Uses `$XDG_RUNTIME_DIR/nextmeeting.sock` if available,
/// otherwise falls back to `/tmp/nextmeeting-$UID.sock`.
pub fn default_socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("nextmeeting.sock")
    } else {
        // Fallback to /tmp with UID
        #[cfg(unix)]
        let uid = unsafe { libc::getuid() };
        #[cfg(not(unix))]
        let uid = 0;
        PathBuf::from(format!("/tmp/nextmeeting-{}.sock", uid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = ServerConfig::default();
        assert!(config.socket_path.to_string_lossy().contains("nextmeeting"));
        assert_eq!(config.connection_timeout, Duration::from_secs(30));
        assert_eq!(config.max_connections, 100);
        assert!(config.cleanup_stale_socket);
    }

    #[test]
    fn custom_config() {
        let config = ServerConfig::new("/custom/path.sock")
            .with_connection_timeout(Duration::from_secs(60))
            .with_max_connections(50)
            .with_cleanup_stale_socket(false);

        assert_eq!(config.socket_path, PathBuf::from("/custom/path.sock"));
        assert_eq!(config.connection_timeout, Duration::from_secs(60));
        assert_eq!(config.max_connections, 50);
        assert!(!config.cleanup_stale_socket);
    }

    #[test]
    fn default_socket_path_format() {
        let path = default_socket_path();
        let path_str = path.to_string_lossy();
        // Should either be in XDG_RUNTIME_DIR or /tmp
        assert!(path_str.contains("nextmeeting"));
        assert!(path_str.ends_with(".sock"));
    }
}
