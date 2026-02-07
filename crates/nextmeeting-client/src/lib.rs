//! CLI, socket client, output rendering, actions
//!
//! This crate provides the `nextmeeting` command-line interface.

pub mod actions;
pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod secret;
pub mod socket;

pub use cli::Cli;
pub use error::{ClientError, ClientResult};
pub use socket::SocketClient;
