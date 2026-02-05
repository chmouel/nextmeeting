//! Core types: time, events, links, filters, formatting

pub mod event;
pub mod time;

pub use event::{EventLink, LinkKind, MeetingView, NormalizedEvent};
pub use time::{EventTime, TimeWindow};
