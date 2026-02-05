//! Core types: time, events, links, filters, formatting

pub mod event;
pub mod links;
pub mod time;

pub use event::{EventLink, LinkKind, MeetingView, NormalizedEvent};
pub use links::{detect_link, extract_links_from_text, LinkDetector};
pub use time::{EventTime, TimeWindow};
