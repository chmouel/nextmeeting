//! Core types: time, events, links, filters, formatting

pub mod event;
pub mod format;
pub mod links;
pub mod time;

pub use event::{EventLink, LinkKind, MeetingView, NormalizedEvent};
pub use format::{
    bulletize, ellipsis, html_escape, make_hyperlink, FormatOptions, FormattedMeeting, JsonMeeting,
    JsonOutput, OutputFormat, OutputFormatter, TimeFormat, UrgencyClass, WaybarOutput,
};
pub use links::{detect_link, extract_links_from_text, LinkDetector};
pub use time::{EventTime, TimeWindow};
