//! Unix signal handling for the server.
//!
//! This module provides signal handling for graceful shutdown and configuration
//! reload using Unix signals:
//! - SIGTERM/SIGINT: Graceful shutdown
//! - SIGHUP: Configuration reload

use std::sync::Arc;

use tokio::sync::watch;
use tracing::{debug, info};

/// Signal types that the server handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// Shutdown signal (SIGTERM, SIGINT).
    Shutdown,
    /// Reload configuration signal (SIGHUP).
    Reload,
}

/// Signal handler that manages Unix signal processing.
pub struct SignalHandler {
    /// Channel to signal shutdown.
    shutdown_tx: Arc<watch::Sender<bool>>,
    /// Channel to receive shutdown signal.
    shutdown_rx: watch::Receiver<bool>,
    /// Channel to signal reload.
    reload_tx: Arc<watch::Sender<bool>>,
    /// Channel to receive reload signal.
    reload_rx: watch::Receiver<bool>,
}

impl Default for SignalHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalHandler {
    /// Creates a new signal handler.
    pub fn new() -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (reload_tx, reload_rx) = watch::channel(false);

        Self {
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            reload_tx: Arc::new(reload_tx),
            reload_rx,
        }
    }

    /// Spawns the signal listener task.
    ///
    /// This should be called once at server startup to start listening for signals.
    #[cfg(unix)]
    pub fn spawn_listener(&self) {
        let shutdown_tx = self.shutdown_tx.clone();
        let reload_tx = self.reload_tx.clone();

        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};

            let mut sigterm = signal(SignalKind::terminate())
                .expect("Failed to install SIGTERM handler");
            let mut sigint = signal(SignalKind::interrupt())
                .expect("Failed to install SIGINT handler");
            let mut sighup = signal(SignalKind::hangup())
                .expect("Failed to install SIGHUP handler");

            loop {
                tokio::select! {
                    _ = sigterm.recv() => {
                        info!("Received SIGTERM, initiating shutdown");
                        let _ = shutdown_tx.send(true);
                        break;
                    }
                    _ = sigint.recv() => {
                        info!("Received SIGINT, initiating shutdown");
                        let _ = shutdown_tx.send(true);
                        break;
                    }
                    _ = sighup.recv() => {
                        info!("Received SIGHUP, triggering reload");
                        let _ = reload_tx.send(true);
                        // Reset the reload flag after a short delay
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        let _ = reload_tx.send(false);
                    }
                }
            }

            debug!("Signal listener stopped");
        });
    }

    /// Non-Unix implementation (no-op).
    #[cfg(not(unix))]
    pub fn spawn_listener(&self) {
        let shutdown_tx = self.shutdown_tx.clone();

        tokio::spawn(async move {
            // On non-Unix, just handle Ctrl+C
            if let Ok(()) = tokio::signal::ctrl_c().await {
                info!("Received Ctrl+C, initiating shutdown");
                let _ = shutdown_tx.send(true);
            }
        });
    }

    /// Returns a future that completes when a shutdown signal is received.
    pub fn shutdown(&self) -> ShutdownSignal {
        ShutdownSignal {
            rx: self.shutdown_rx.clone(),
        }
    }

    /// Returns a future that completes when a reload signal is received.
    pub fn reload(&self) -> ReloadSignal {
        ReloadSignal {
            rx: self.reload_rx.clone(),
        }
    }

    /// Returns true if shutdown has been signaled.
    pub fn is_shutdown(&self) -> bool {
        *self.shutdown_rx.borrow()
    }

    /// Returns true if reload has been signaled.
    pub fn is_reload(&self) -> bool {
        *self.reload_rx.borrow()
    }

    /// Programmatically triggers a shutdown.
    pub fn trigger_shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Programmatically triggers a reload.
    pub fn trigger_reload(&self) {
        let _ = self.reload_tx.send(true);
    }

    /// Creates a shutdown handle that can be passed to other components.
    pub fn shutdown_handle(&self) -> ShutdownHandle {
        ShutdownHandle {
            tx: self.shutdown_tx.clone(),
            rx: self.shutdown_rx.clone(),
        }
    }
}

/// A signal that completes when shutdown is signaled.
pub struct ShutdownSignal {
    rx: watch::Receiver<bool>,
}

impl ShutdownSignal {
    /// Waits for the shutdown signal.
    pub async fn wait(mut self) {
        loop {
            if *self.rx.borrow() {
                return;
            }
            if self.rx.changed().await.is_err() {
                return;
            }
            if *self.rx.borrow() {
                return;
            }
        }
    }
}

/// A signal that completes when reload is signaled.
pub struct ReloadSignal {
    rx: watch::Receiver<bool>,
}

impl ReloadSignal {
    /// Waits for the reload signal.
    pub async fn wait(mut self) {
        loop {
            if *self.rx.borrow() {
                return;
            }
            if self.rx.changed().await.is_err() {
                return;
            }
            if *self.rx.borrow() {
                return;
            }
        }
    }
}

/// A handle for triggering or checking shutdown status.
#[derive(Clone)]
pub struct ShutdownHandle {
    tx: Arc<watch::Sender<bool>>,
    rx: watch::Receiver<bool>,
}

impl ShutdownHandle {
    /// Triggers a shutdown.
    pub fn trigger(&self) {
        let _ = self.tx.send(true);
    }

    /// Returns true if shutdown has been triggered.
    pub fn is_shutdown(&self) -> bool {
        *self.rx.borrow()
    }

    /// Returns a future that completes when shutdown is triggered.
    pub fn wait(&self) -> ShutdownSignal {
        ShutdownSignal {
            rx: self.rx.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn signal_handler_shutdown() {
        let handler = SignalHandler::new();
        
        assert!(!handler.is_shutdown());
        
        // Trigger shutdown programmatically
        handler.trigger_shutdown();
        
        assert!(handler.is_shutdown());
    }

    #[tokio::test]
    async fn signal_handler_reload() {
        let handler = SignalHandler::new();
        
        assert!(!handler.is_reload());
        
        // Trigger reload programmatically
        handler.trigger_reload();
        
        assert!(handler.is_reload());
    }

    #[tokio::test]
    async fn shutdown_signal_wait() {
        let handler = SignalHandler::new();
        let shutdown = handler.shutdown();

        // Spawn a task to trigger shutdown after a delay
        let tx = handler.shutdown_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _ = tx.send(true);
        });

        // Wait for shutdown signal with timeout
        let result = tokio::time::timeout(Duration::from_millis(100), shutdown.wait()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn shutdown_handle() {
        let handler = SignalHandler::new();
        let handle = handler.shutdown_handle();

        assert!(!handle.is_shutdown());

        // Trigger from handle
        handle.trigger();

        assert!(handle.is_shutdown());
        assert!(handler.is_shutdown());
    }

    #[tokio::test]
    async fn shutdown_handle_wait() {
        let handler = SignalHandler::new();
        let handle = handler.shutdown_handle();

        // Spawn a task to wait for shutdown
        let wait_handle = handle.clone();
        let wait_task = tokio::spawn(async move {
            wait_handle.wait().wait().await;
            true
        });

        // Small delay then trigger
        tokio::time::sleep(Duration::from_millis(10)).await;
        handle.trigger();

        let result = tokio::time::timeout(Duration::from_millis(100), wait_task).await;
        assert!(result.is_ok());
        assert!(result.unwrap().unwrap());
    }
}
