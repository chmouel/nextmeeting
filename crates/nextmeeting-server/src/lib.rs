//! Daemon: scheduler, cache, notifications.
//!
//! This crate provides the nextmeeting server daemon that handles:
//! - Unix socket IPC for client communication
//! - Calendar event caching with TTL
//! - Background scheduling for calendar sync
//! - Desktop notifications for upcoming meetings
//!
//! # Example
//!
//! ```rust,no_run
//! use nextmeeting_server::{ServerConfig, SocketServer};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = ServerConfig::default();
//!     let server = SocketServer::new(config).await?;
//!     
//!     // Handle connections...
//!     Ok(())
//! }
//! ```

mod cache;
mod config;
mod error;
mod handler;
mod notify;
mod pidfile;
mod scheduler;
mod signals;
mod socket;

pub use cache::{CacheEntry, EventCache};
pub use config::{ServerConfig, default_socket_path};
pub use error::{ServerError, ServerResult};
pub use handler::{
    RequestHandler, ServerState, SharedState, make_connection_handler, new_shared_state,
};
pub use notify::{
    NotifyConfig, NotifyEngine, NotifyState, SharedNotifyState, new_notify_state,
    notification_hash,
};
pub use pidfile::{PidFile, default_pid_path};
pub use scheduler::{
    Scheduler, SchedulerCommand, SchedulerConfig, SchedulerHandle, SchedulerState,
    SharedSchedulerState, new_scheduler_state,
};
pub use signals::{ReloadSignal, ShutdownHandle, ShutdownSignal, Signal, SignalHandler};
pub use socket::{Connection, SocketServer};
