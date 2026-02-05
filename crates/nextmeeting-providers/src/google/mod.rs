//! Google Calendar provider implementation.
//!
//! This module provides a [`GoogleProvider`] that can fetch events from
//! Google Calendar using the Google Calendar API.
//!
//! # Features
//!
//! - OAuth 2.0 PKCE authorization flow with loopback redirect
//! - Token persistence with secure storage
//! - Automatic token refresh
//! - ETag-based conditional fetching
//! - Rate limit handling with exponential backoff
//! - Recurring event expansion (server-side)
//!
//! # Authentication Flow
//!
//! 1. User provides their own OAuth client ID/secret (required by Google)
//! 2. Provider starts a local HTTP server on a random port
//! 3. Opens browser to Google's authorization page with PKCE challenge
//! 4. User grants permissions in the browser
//! 5. Google redirects to the loopback server with the authorization code
//! 6. Provider exchanges the code for access and refresh tokens
//! 7. Tokens are persisted for future use
//!
//! # Example
//!
//! ```ignore
//! use nextmeeting_providers::google::{GoogleProvider, GoogleConfig, OAuthCredentials};
//!
//! let credentials = OAuthCredentials::new(
//!     "your-client-id.apps.googleusercontent.com",
//!     "your-client-secret",
//! );
//!
//! let config = GoogleConfig::new(credentials)
//!     .with_domain("example.com");  // For Google Workspace
//!
//! let provider = GoogleProvider::new(config)?;
//!
//! if !provider.is_authenticated() {
//!     provider.authenticate().await?;
//! }
//!
//! let events = provider.fetch_events(FetchOptions::new()).await?;
//! ```

mod client;
mod config;
mod oauth;
mod provider;
mod tokens;

pub use client::CalendarListEntry;
pub use config::{GoogleConfig, OAuthCredentials};
pub use oauth::{OAuthClient, PkceFlow};
pub use provider::GoogleProvider;
pub use tokens::{TokenInfo, TokenStorage};
