//! CalDAV calendar provider implementation.
//!
//! This module provides a [`CalDavProvider`] that can fetch events from
//! CalDAV-compatible calendar servers.
//!
//! # Features
//!
//! - HTTP Digest and Basic authentication
//! - PROPFIND for calendar discovery
//! - REPORT with time-range expansion for recurring events
//! - ICS/iCalendar parsing
//! - TLS configuration (can be disabled for testing)
//!
//! # Example
//!
//! ```ignore
//! use nextmeeting_providers::caldav::{CalDavProvider, CalDavConfig};
//!
//! let config = CalDavConfig::new("https://caldav.example.com/calendars/user/")
//!     .with_credentials("user", "password");
//!
//! let provider = CalDavProvider::new(config)?;
//! let events = provider.fetch_events(FetchOptions::new()).await?;
//! ```

mod auth;
mod client;
mod config;
mod ics;
mod provider;
mod xml;

pub use config::CalDavConfig;
pub use provider::CalDavProvider;

// Re-export for testing
#[cfg(test)]
pub(crate) use auth::DigestAuth;
