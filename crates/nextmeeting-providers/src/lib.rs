//! CalendarProvider trait and implementations.
//!
//! This crate provides the abstraction layer for calendar backends:
//!
//! - [`CalendarProvider`] - The core trait that all calendar backends implement
//! - [`RawEvent`] - Provider-agnostic raw event data
//! - [`normalize_event`] - Pipeline to convert raw events to normalized form
//! - [`ProviderError`] - Error types for provider operations
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐    ┌─────────────────┐
//! │  Google API     │    │  CalDAV Server  │
//! └────────┬────────┘    └────────┬────────┘
//!          │                      │
//!          ▼                      ▼
//! ┌─────────────────┐    ┌─────────────────┐
//! │ GoogleProvider  │    │ CalDavProvider  │
//! └────────┬────────┘    └────────┬────────┘
//!          │                      │
//!          │   CalendarProvider   │
//!          └──────────┬───────────┘
//!                     │
//!                     ▼
//!              ┌─────────────┐
//!              │  RawEvent   │
//!              └──────┬──────┘
//!                     │
//!                     ▼ normalize_event()
//!              ┌──────────────────┐
//!              │ NormalizedEvent  │
//!              └──────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use nextmeeting_providers::{CalendarProvider, FetchOptions, normalize_event};
//!
//! async fn fetch_meetings(provider: &dyn CalendarProvider) -> Vec<NormalizedEvent> {
//!     let result = provider.fetch_events(FetchOptions::new()).await?;
//!     result.events.iter().map(normalize_event).collect()
//! }
//! ```

#[cfg(feature = "caldav")]
pub mod caldav;
pub mod error;
pub mod normalize;
pub mod provider;
pub mod raw_event;

// Re-export main types at crate root
pub use error::{ProviderError, ProviderErrorCode, ProviderResult};
pub use normalize::{normalize_event, normalize_events};
pub use provider::{
    BoxFuture, CalendarInfo, CalendarProvider, ErrorProvider, FetchOptions, FetchResult,
    ProviderStatus,
};
pub use raw_event::{
    RawAttendee, RawConferenceData, RawEntryPoint, RawEvent, RawEventTime, ResponseStatus,
};
