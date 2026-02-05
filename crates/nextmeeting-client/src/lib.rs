//! CLI, socket client, output rendering, actions
//!
//! This crate provides the `nextmeeting` command-line interface.

pub mod cli;
pub mod commands;
pub mod config;
pub mod error;

pub use cli::Cli;
pub use error::{ClientError, ClientResult};
