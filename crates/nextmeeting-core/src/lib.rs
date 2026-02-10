//! Core types: time, events, links, filters, formatting

pub mod event;
pub mod format;
pub mod links;
pub mod time;
pub mod tracing;

pub use event::{Attendee, EventLink, LinkKind, MeetingView, NormalizedEvent, ResponseStatus};
pub use format::{
    FormatOptions, FormattedMeeting, JsonMeeting, JsonOutput, OutputFormat, OutputFormatter,
    TimeFormat, UrgencyClass, WaybarOutput, bulletize, ellipsis, html_escape, make_hyperlink,
};
pub use links::{LinkDetector, detect_link, extract_links_from_text};
pub use time::{EventTime, TimeWindow};
pub use tracing::{TracingConfig, TracingError, TracingOutputFormat, init_tracing};
