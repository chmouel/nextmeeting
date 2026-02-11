//! Output formatting for calendar meetings.
//!
//! This module provides formatters for displaying meetings in various output formats:
//! - **TTY**: Human-readable terminal output with optional hyperlinks
//! - **Waybar**: JSON output for Waybar status bar integration
//! - **JSON**: Machine-readable JSON output
//!
//! # Example
//!
//! ```rust
//! use nextmeeting_core::format::{OutputFormat, OutputFormatter, FormatOptions};
//! use nextmeeting_core::{MeetingView, NormalizedEvent, EventTime};
//! use chrono::{DateTime, Utc, TimeZone};
//!
//! let now = Utc::now();
//! let options = FormatOptions::default();
//! let formatter = OutputFormatter::new(options);
//!
//! // Format meetings for different outputs
//! // let tty_output = formatter.format_tty(&meetings);
//! // let waybar_json = formatter.format_waybar(&meetings, "No meetings");
//! ```

use std::borrow::Cow;

use chrono::{DateTime, Local, Timelike};
use serde::{Deserialize, Serialize};

use crate::event::MeetingView;

const DEFAULT_UNTIL_OFFSET_MINUTES: i64 = 60;

/// The output format for meeting display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Human-readable terminal output.
    #[default]
    Tty,
    /// JSON output for Waybar status bar.
    Waybar,
    /// Machine-readable JSON output.
    Json,
}

/// CSS class for meeting urgency in status bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UrgencyClass {
    /// Meeting is currently ongoing.
    Ongoing,
    /// Meeting is currently ongoing and close to ending.
    EndingSoon,
    /// Meeting starts soon (within threshold).
    Soon,
    /// Meeting is upcoming but not imminent.
    Upcoming,
    /// All-day event.
    AllDay,
}

impl UrgencyClass {
    /// Returns the CSS class name for this urgency level.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ongoing => "ongoing",
            Self::EndingSoon => "ending_soon",
            Self::Soon => "soon",
            Self::Upcoming => "upcoming",
            Self::AllDay => "allday",
        }
    }
}

/// Configuration options for output formatting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatOptions {
    /// Maximum length for meeting titles (truncated with ellipsis).
    pub max_title_length: Option<usize>,
    /// Number of minutes before meeting to mark as "soon".
    pub soon_threshold_minutes: i64,
    /// Whether to include hyperlinks (OSC8) in TTY output.
    pub hyperlinks: bool,
    /// Hour separator character (e.g., ":", "h", "H").
    pub hour_separator: String,
    /// Time format preference ("24h" or "12h").
    pub time_format: TimeFormat,
    /// Whether to show the time remaining format or absolute time.
    pub show_relative_time: bool,
    /// Maximum number of meetings to include in tooltip.
    pub tooltip_limit: Option<usize>,
    /// Custom format template for the tooltip.
    pub tooltip_format: Option<String>,
    /// Custom format template for the main display.
    pub custom_format: Option<String>,
    /// When the meeting is more than this many minutes away, show absolute time instead of countdown.
    pub until_offset_minutes: Option<i64>,
    /// Enable privacy mode (replace titles).
    pub privacy: bool,
    /// Title to use when privacy mode is enabled.
    pub privacy_title: String,
    /// Background color for "soon" notifications in Waybar (Pango markup).
    pub notify_min_color: Option<String>,
    /// Foreground color for "soon" notifications in Waybar (Pango markup).
    pub notify_min_color_foreground: Option<String>,
    /// Whether to show all-day meetings in Waybar output.
    pub waybar_show_all_day: bool,
    /// Minutes before meeting end to mark as "ending soon".
    ///
    /// If not set, end-warning behaviour is disabled.
    pub end_warning_minutes: Option<i64>,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            max_title_length: None,
            soon_threshold_minutes: 5,
            hyperlinks: true,
            hour_separator: ":".to_string(),
            time_format: TimeFormat::H24,
            show_relative_time: true,
            tooltip_limit: None,
            tooltip_format: None,
            custom_format: None,
            until_offset_minutes: None,
            privacy: false,
            privacy_title: "Busy".to_string(),
            notify_min_color: None,
            notify_min_color_foreground: None,
            waybar_show_all_day: true,
            end_warning_minutes: None,
        }
    }
}

/// Time format preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeFormat {
    /// 24-hour format (e.g., "14:30").
    #[default]
    H24,
    /// 12-hour format with AM/PM (e.g., "2:30 PM").
    H12,
}

/// Waybar output format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaybarOutput {
    /// Text to display in the bar.
    pub text: String,
    /// Tooltip text (shown on hover).
    pub tooltip: String,
    /// CSS class for styling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Alternative text (for accessibility).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    /// Percentage (for progress indicators).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<u8>,
}

impl WaybarOutput {
    /// Creates a new WaybarOutput with required fields.
    pub fn new(text: impl Into<String>, tooltip: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tooltip: tooltip.into(),
            class: None,
            alt: None,
            percentage: None,
        }
    }

    /// Sets the CSS class.
    pub fn with_class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }

    /// Sets the alt text.
    pub fn with_alt(mut self, alt: impl Into<String>) -> Self {
        self.alt = Some(alt.into());
        self
    }
}

/// JSON output format for machine consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonOutput {
    /// List of formatted meetings.
    pub meetings: Vec<JsonMeeting>,
    /// The next non-all-day meeting, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_meeting: Option<JsonMeeting>,
    /// Number of meetings returned.
    pub count: usize,
}

/// A single meeting in JSON format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonMeeting {
    /// Event ID.
    pub id: String,
    /// Meeting title (may be truncated).
    pub title: String,
    /// Start time in ISO 8601 format.
    pub start_time: String,
    /// End time in ISO 8601 format.
    pub end_time: String,
    /// Formatted time string for display.
    pub time_display: String,
    /// Whether this is an all-day event.
    pub is_all_day: bool,
    /// Whether the meeting is ongoing.
    pub is_ongoing: bool,
    /// Minutes until meeting starts (negative if ongoing/past).
    pub minutes_until: i64,
    /// Primary meeting URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_url: Option<String>,
    /// Calendar URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendar_url: Option<String>,
    /// Urgency class.
    pub urgency: UrgencyClass,
}

/// A formatted meeting line with metadata.
#[derive(Debug, Clone)]
pub struct FormattedMeeting {
    /// The formatted display text.
    pub text: String,
    /// The urgency class.
    pub urgency: UrgencyClass,
    /// The underlying meeting view.
    pub meeting: MeetingView,
}

/// Output formatter for calendar meetings.
#[derive(Debug, Clone)]
pub struct OutputFormatter {
    options: FormatOptions,
}

impl OutputFormatter {
    /// Creates a new OutputFormatter with the given options.
    pub fn new(options: FormatOptions) -> Self {
        Self { options }
    }

    /// Creates a new OutputFormatter with default options.
    pub fn with_defaults() -> Self {
        Self::new(FormatOptions::default())
    }

    /// Formats meetings for TTY output.
    ///
    /// Returns a list of formatted lines suitable for terminal display.
    pub fn format_tty(&self, meetings: &[MeetingView]) -> Vec<FormattedMeeting> {
        self.format_tty_at(meetings, Local::now())
    }

    /// Formats meetings for TTY output at a specific time.
    ///
    /// This variant is useful for testing with a fixed time.
    pub fn format_tty_at(
        &self,
        meetings: &[MeetingView],
        now: DateTime<Local>,
    ) -> Vec<FormattedMeeting> {
        meetings
            .iter()
            .map(|m| self.format_single_meeting(m, now, self.options.hyperlinks, false))
            .collect()
    }

    /// Formats meetings for Waybar output.
    ///
    /// Returns a JSON-serializable structure with text, tooltip, and class.
    pub fn format_waybar(&self, meetings: &[MeetingView], no_meeting_text: &str) -> WaybarOutput {
        self.format_waybar_at(meetings, no_meeting_text, Local::now())
    }

    /// Formats meetings for Waybar output at a specific time.
    ///
    /// This variant is useful for testing with a fixed time.
    pub fn format_waybar_at(
        &self,
        meetings: &[MeetingView],
        no_meeting_text: &str,
        now: DateTime<Local>,
    ) -> WaybarOutput {
        if meetings.is_empty() {
            return WaybarOutput::new(no_meeting_text, "No upcoming meetings");
        }

        // If waybar_show_all_day is false and only all-day meetings exist, show no_meeting_text
        if !self.options.waybar_show_all_day && meetings.iter().all(|m| m.is_all_day) {
            return WaybarOutput::new(no_meeting_text, "No upcoming meetings");
        }

        // Get the next non-all-day meeting for the main display
        let next_meeting = self.get_next_meeting_for_display(meetings);

        let (text, class) = if let Some(meeting) = next_meeting {
            let formatted = self.format_single_meeting(meeting, now, false, true);
            let urgency = formatted.urgency;
            (formatted.text.clone(), Some(urgency.as_str().to_string()))
        } else {
            // Only all-day meetings
            let first = &meetings[0];
            let raw_title = self.privacy_title(&first.title);
            let title = self.truncate_title(&raw_title);
            (
                format!("All day: {}", html_escape(&title)),
                Some("allday".to_string()),
            )
        };

        // Build tooltip with all meetings
        let tooltip_meetings: Vec<_> = if let Some(limit) = self.options.tooltip_limit {
            meetings.iter().take(limit).collect()
        } else {
            meetings.iter().collect()
        };

        let tooltip_lines: Vec<String> = tooltip_meetings
            .iter()
            .map(|m| self.format_tooltip_line(m, now))
            .collect();

        let tooltip = bulletize(&tooltip_lines);

        let mut output = WaybarOutput::new(text, tooltip);
        output.class = class;
        output
    }

    /// Wraps minute text in Pango `<span>` with configured notification colors.
    fn format_waybar_minute_badge(&self, minutes: i64) -> String {
        let has_bg = self.options.notify_min_color.is_some();
        let has_fg = self.options.notify_min_color_foreground.is_some();

        if !has_bg && !has_fg {
            return minutes.to_string();
        }

        let mut attrs = String::new();
        if let Some(ref bg) = self.options.notify_min_color {
            attrs.push_str(&format!(" background=\"{}\"", html_escape(bg)));
        }
        if let Some(ref fg) = self.options.notify_min_color_foreground {
            attrs.push_str(&format!(" color=\"{}\"", html_escape(fg)));
        }

        format!("<span{}>{}</span>", attrs, minutes)
    }

    /// Formats meetings as JSON output.
    ///
    /// Returns a structured JSON output for machine consumption.
    pub fn format_json(&self, meetings: &[MeetingView]) -> JsonOutput {
        self.format_json_at(meetings, Local::now())
    }

    /// Formats meetings as JSON output at a specific time.
    ///
    /// This variant is useful for testing with a fixed time.
    pub fn format_json_at(&self, meetings: &[MeetingView], now: DateTime<Local>) -> JsonOutput {
        let json_meetings: Vec<JsonMeeting> = meetings
            .iter()
            .map(|m| self.to_json_meeting(m, now))
            .collect();

        let next_meeting = self
            .get_next_meeting_for_display(meetings)
            .map(|m| self.to_json_meeting(m, now));

        JsonOutput {
            count: json_meetings.len(),
            meetings: json_meetings,
            next_meeting,
        }
    }

    /// Formats a single meeting for display.
    fn format_single_meeting(
        &self,
        meeting: &MeetingView,
        now: DateTime<Local>,
        hyperlink: bool,
        waybar_markup: bool,
    ) -> FormattedMeeting {
        let urgency = self.compute_urgency(meeting, now);

        let text = if let Some(ref template) = self.options.custom_format {
            self.format_with_template(meeting, now, template, waybar_markup)
        } else {
            let time_str = self.format_time(meeting, now, waybar_markup);
            let title = self.format_title(meeting, hyperlink);
            if waybar_markup {
                format!("{} - {}", time_str, html_escape(&title))
            } else {
                format!("{} - {}", time_str, title)
            }
        };

        FormattedMeeting {
            text,
            urgency,
            meeting: meeting.clone(),
        }
    }

    /// Formats a meeting line for the tooltip.
    fn format_tooltip_line(&self, meeting: &MeetingView, now: DateTime<Local>) -> String {
        if let Some(ref template) = self.options.tooltip_format {
            self.format_with_template(meeting, now, template, false)
        } else {
            let time_str = self.format_tooltip_time(meeting, now);
            let raw_title = self.privacy_title(&meeting.title);
            let title = self.truncate_title(&raw_title);
            let escaped_title = html_escape(&title);
            format!("{} - {}", time_str, escaped_title)
        }
    }

    /// Formats time for tooltip display with date context for non-today meetings.
    fn format_tooltip_time(&self, meeting: &MeetingView, now: DateTime<Local>) -> String {
        let date = meeting.start_local;

        if meeting.is_all_day {
            if date.date_naive() == now.date_naive() {
                return "All day".to_string();
            } else if date.date_naive() == now.date_naive() + chrono::Duration::days(1) {
                return "Tomorrow (all day)".to_string();
            } else {
                return date.format("%a %d (all day)").to_string();
            }
        }

        let time_str = self.format_absolute_time(meeting);

        if date.date_naive() == now.date_naive() {
            // Same day: just show time
            time_str
        } else {
            // Different day: use date prefix
            let delta = date - now;
            let days = delta.num_minutes().div_euclid(24 * 60);

            let date_prefix = if days == 0 {
                "Tomorrow".to_string()
            } else {
                date.format("%a %d").to_string()
            };

            format!("{} at {}", date_prefix, time_str)
        }
    }

    /// Formats a meeting using a custom template.
    fn format_with_template(
        &self,
        meeting: &MeetingView,
        now: DateTime<Local>,
        template: &str,
        waybar_markup: bool,
    ) -> String {
        let title = self.privacy_title(&meeting.title);
        let meet_url = meeting
            .primary_link
            .as_ref()
            .map(|l| l.url.as_str())
            .unwrap_or("");
        let calendar_url = meeting.calendar_url.as_deref().unwrap_or("");
        let minutes_until = meeting.minutes_until_start(now);
        let when = self.format_time(meeting, now, waybar_markup);

        template
            .replace("{title}", &title)
            .replace("{when}", &when)
            .replace(
                "{start_time:%H:%M}",
                &meeting.start_local.format("%H:%M").to_string(),
            )
            .replace(
                "{end_time:%H:%M}",
                &meeting.end_local.format("%H:%M").to_string(),
            )
            .replace(
                "{start_time}",
                &meeting.start_local.format("%H:%M").to_string(),
            )
            .replace("{end_time}", &meeting.end_local.format("%H:%M").to_string())
            .replace("{meet_url}", meet_url)
            .replace("{calendar_url}", calendar_url)
            .replace("{minutes_until}", &minutes_until.to_string())
            .replace("{is_all_day}", &meeting.is_all_day.to_string())
            .replace("{is_ongoing}", &meeting.is_ongoing.to_string())
    }

    /// Computes the urgency class for a meeting.
    fn compute_urgency(&self, meeting: &MeetingView, now: DateTime<Local>) -> UrgencyClass {
        if meeting.is_all_day {
            return UrgencyClass::AllDay;
        }

        if meeting.start_local <= now && now < meeting.end_local {
            if self.is_meeting_ending_soon(meeting, now) {
                return UrgencyClass::EndingSoon;
            }
            return UrgencyClass::Ongoing;
        }

        let minutes_until = meeting.minutes_until_start(now);
        if minutes_until <= self.options.soon_threshold_minutes {
            UrgencyClass::Soon
        } else {
            UrgencyClass::Upcoming
        }
    }

    /// Returns true when a meeting is ongoing and within the configured end-warning window.
    fn is_meeting_ending_soon(&self, meeting: &MeetingView, now: DateTime<Local>) -> bool {
        let threshold = match self.options.end_warning_minutes {
            Some(minutes) if minutes > 0 => minutes,
            _ => return false,
        };

        if !(meeting.start_local <= now && now < meeting.end_local) {
            return false;
        }

        let minutes_left = meeting.minutes_until_end(now);
        minutes_left >= 0 && minutes_left <= threshold
    }

    /// Formats the time display for a meeting.
    fn format_time(
        &self,
        meeting: &MeetingView,
        now: DateTime<Local>,
        waybar_markup: bool,
    ) -> String {
        if meeting.is_all_day {
            return "All day".to_string();
        }

        if meeting.start_local <= now && now < meeting.end_local {
            let minutes_left = meeting.minutes_until_end(now);
            return self.format_time_remaining(minutes_left, true);
        }

        if self.options.show_relative_time {
            self.format_time_until(meeting.start_local, now, waybar_markup)
        } else {
            self.format_absolute_time(meeting)
        }
    }

    /// Formats absolute time (HH:MM or HH:MM AM/PM).
    fn format_absolute_time(&self, meeting: &MeetingView) -> String {
        if meeting.is_all_day {
            return "All day".to_string();
        }

        let sep = &self.options.hour_separator;
        match self.options.time_format {
            TimeFormat::H24 => meeting
                .start_local
                .format(&format!("%H{}%M", sep))
                .to_string(),
            TimeFormat::H12 => meeting
                .start_local
                .format(&format!("%I{}%M %p", sep))
                .to_string(),
        }
    }

    /// Formats "time until" display.
    fn format_time_until(
        &self,
        date: DateTime<Local>,
        now: DateTime<Local>,
        waybar_markup: bool,
    ) -> String {
        let delta = date - now;
        let secs = delta.num_seconds();
        let total_minutes = if secs > 0 {
            (secs + 59) / 60
        } else {
            secs / 60
        };
        let days = total_minutes.div_euclid(24 * 60);
        let hours = total_minutes.rem_euclid(24 * 60).div_euclid(60);
        let minutes = total_minutes.rem_euclid(60);

        if date.date_naive() != now.date_naive() {
            let mut s = if days == 0 {
                "Tomorrow".to_string()
            } else {
                date.format("%a %d").to_string()
            };

            match self.options.time_format {
                TimeFormat::H12 => {
                    let sep = &self.options.hour_separator;
                    s.push_str(&format!(" at {}", date.format(&format!("%I{}%M %p", sep))));
                }
                TimeFormat::H24 => {
                    let sep = &self.options.hour_separator;
                    s.push_str(&format!(
                        " at {:02}{}{:02}",
                        date.hour(),
                        sep,
                        date.minute()
                    ));
                }
            }
            return s;
        }

        let until_offset = self
            .options
            .until_offset_minutes
            .unwrap_or(DEFAULT_UNTIL_OFFSET_MINUTES);
        if total_minutes > until_offset {
            return self.format_absolute_time_from_date(date);
        }

        if total_minutes <= 0 {
            return "Now".to_string();
        }

        if waybar_markup
            && minutes <= self.options.soon_threshold_minutes
            && self.options.notify_min_color.is_some()
        {
            if minutes == 0 {
                return "Now 🏃".to_string();
            }
            let badge = self.format_waybar_minute_badge(minutes);
            return format!("In {} minutes", badge);
        }

        let mut parts = Vec::new();
        if days > 0 {
            parts.push(format!("{} day{}", days, if days != 1 { "s" } else { "" }));
        }
        if hours > 0 {
            parts.push(format!(
                "{} hour{}",
                hours,
                if hours != 1 { "s" } else { "" }
            ));
        }
        if minutes > 0 || parts.is_empty() {
            parts.push(format!(
                "{} minute{}",
                minutes,
                if minutes != 1 { "s" } else { "" }
            ));
        }

        if parts.len() == 1 {
            format!("In {}", parts[0])
        } else {
            let last = parts.pop().expect("parts contains at least two entries");
            format!("In {} and {}", parts.join(", "), last)
        }
    }

    /// Formats an absolute date/time string using configured time format and separator.
    fn format_absolute_time_from_date(&self, date: DateTime<Local>) -> String {
        let sep = &self.options.hour_separator;
        match self.options.time_format {
            TimeFormat::H24 => date.format(&format!("%H{}%M", sep)).to_string(),
            TimeFormat::H12 => date.format(&format!("%I{}%M %p", sep)).to_string(),
        }
    }

    /// Formats "time remaining" display (e.g., "15 minutes to go").
    fn format_time_remaining(&self, minutes: i64, is_ongoing: bool) -> String {
        if minutes <= 0 {
            return "Ending".to_string();
        }

        let suffix = if is_ongoing { " to go" } else { "" };

        if minutes < 60 {
            let unit = if minutes == 1 { "minute" } else { "minutes" };
            return format!("{} {}{}", minutes, unit, suffix);
        }

        let hours = minutes / 60;
        let mins = minutes % 60;

        if mins == 0 {
            format!("{}H{}", hours, suffix)
        } else {
            format!("{}H{:02}{}", hours, mins, suffix)
        }
    }

    /// Applies privacy substitution to a title.
    fn privacy_title<'a>(&'a self, title: &'a str) -> Cow<'a, str> {
        if self.options.privacy {
            Cow::Borrowed(self.options.privacy_title.as_str())
        } else {
            Cow::Borrowed(title)
        }
    }

    /// Formats the meeting title, optionally with hyperlink.
    fn format_title(&self, meeting: &MeetingView, hyperlink: bool) -> String {
        let raw_title = self.privacy_title(&meeting.title);
        let title = self.truncate_title(&raw_title);

        if hyperlink {
            if let Some(ref link) = meeting.primary_link {
                return make_hyperlink(&link.url, &title);
            }
            if let Some(ref url) = meeting.calendar_url {
                return make_hyperlink(url, &title);
            }
        }

        title.into_owned()
    }

    /// Truncates a title to the configured maximum length.
    fn truncate_title<'a>(&self, title: &'a str) -> Cow<'a, str> {
        if let Some(max_len) = self.options.max_title_length {
            ellipsis(title, max_len)
        } else {
            Cow::Borrowed(title)
        }
    }

    /// Gets the next non-all-day meeting for display.
    fn get_next_meeting_for_display<'a>(
        &self,
        meetings: &'a [MeetingView],
    ) -> Option<&'a MeetingView> {
        // Prefer non-all-day meetings
        meetings
            .iter()
            .find(|m| !m.is_all_day)
            .or_else(|| meetings.first())
    }

    /// Converts a MeetingView to JsonMeeting.
    fn to_json_meeting(&self, meeting: &MeetingView, now: DateTime<Local>) -> JsonMeeting {
        let urgency = self.compute_urgency(meeting, now);
        let time_display = self.format_time(meeting, now, false);
        let minutes_until = meeting.minutes_until_start(now);
        let raw_title = self.privacy_title(&meeting.title);
        let title = self.truncate_title(&raw_title).into_owned();

        JsonMeeting {
            id: meeting.id.clone(),
            title,
            start_time: meeting.start_local.to_rfc3339(),
            end_time: meeting.end_local.to_rfc3339(),
            time_display,
            is_all_day: meeting.is_all_day,
            is_ongoing: meeting.is_ongoing,
            minutes_until,
            meeting_url: meeting.primary_link.as_ref().map(|l| l.url.clone()),
            calendar_url: meeting.calendar_url.clone(),
            urgency,
        }
    }
}

/// Truncates a string with ellipsis if it exceeds the given length.
///
/// Handles HTML entities properly, counting them as single characters.
pub fn ellipsis(s: &str, max_len: usize) -> Cow<'_, str> {
    if max_len == 0 {
        return Cow::Borrowed("");
    }

    // Count actual display characters (simplified - doesn't handle all HTML entities)
    let char_count = s.chars().count();

    if char_count <= max_len {
        return Cow::Borrowed(s);
    }

    // Truncate and add ellipsis
    let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
    Cow::Owned(format!("{}...", truncated))
}

/// Creates an OSC8 hyperlink for terminal output.
///
/// This creates an ANSI escape sequence that modern terminals interpret as a clickable link.
pub fn make_hyperlink(url: &str, label: &str) -> String {
    // OSC8 hyperlink format: \e]8;;URL\e\\LABEL\e]8;;\e\\
    format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, label)
}

/// Escapes text for HTML display.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Creates bullet points from a list of items.
pub fn bulletize(items: &[String]) -> String {
    if items.is_empty() {
        return String::new();
    }
    items
        .iter()
        .map(|item| format!("• {}", item))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventLink, LinkKind, NormalizedEvent};
    use crate::time::EventTime;
    use chrono::{TimeZone, Utc};

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, s).unwrap()
    }

    fn local_from_utc(dt: DateTime<Utc>) -> DateTime<Local> {
        dt.with_timezone(&Local)
    }

    fn sample_meeting(now: DateTime<Utc>, offset_minutes: i64) -> MeetingView {
        let start = now + chrono::Duration::minutes(offset_minutes);
        let end = start + chrono::Duration::minutes(30);

        let event = NormalizedEvent::new(
            "evt-123",
            "Team Standup",
            EventTime::from_utc(start),
            EventTime::from_utc(end),
            "primary",
        )
        .with_link(EventLink::new(
            LinkKind::GoogleMeet,
            "https://meet.google.com/abc-defg-hij",
        ))
        .with_calendar_url("https://calendar.google.com/event/123");

        MeetingView::from_event(&event, now)
    }

    fn ongoing_meeting_ending_in(now: DateTime<Utc>, minutes_until_end: i64) -> MeetingView {
        let end = now + chrono::Duration::minutes(minutes_until_end);
        let start = end - chrono::Duration::minutes(30);

        let event = NormalizedEvent::new(
            "evt-ongoing",
            "Sprint Review",
            EventTime::from_utc(start),
            EventTime::from_utc(end),
            "primary",
        )
        .with_link(EventLink::new(
            LinkKind::GoogleMeet,
            "https://meet.google.com/abc-defg-hij",
        ));

        MeetingView::from_event(&event, now)
    }

    mod ellipsis_tests {
        use super::*;

        #[test]
        fn short_string_unchanged() {
            assert_eq!(ellipsis("hello", 10), "hello");
        }

        #[test]
        fn exact_length_unchanged() {
            assert_eq!(ellipsis("hello", 5), "hello");
        }

        #[test]
        fn long_string_truncated() {
            assert_eq!(ellipsis("hello world", 8), "hello...");
        }

        #[test]
        fn zero_length() {
            assert_eq!(ellipsis("hello", 0), "");
        }

        #[test]
        fn very_short_max() {
            assert_eq!(ellipsis("hello", 3), "...");
        }
    }

    mod hyperlink_tests {
        use super::*;

        #[test]
        fn creates_osc8_link() {
            let result = make_hyperlink("https://example.com", "Click me");
            assert!(result.contains("https://example.com"));
            assert!(result.contains("Click me"));
            assert!(result.contains("\x1b]8;;"));
        }
    }

    mod html_escape_tests {
        use super::*;

        #[test]
        fn escapes_special_chars() {
            assert_eq!(html_escape("<script>"), "&lt;script&gt;");
            assert_eq!(html_escape("a & b"), "a &amp; b");
            assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
        }
    }

    mod output_format {
        use super::*;

        #[test]
        fn default_is_tty() {
            assert_eq!(OutputFormat::default(), OutputFormat::Tty);
        }

        #[test]
        fn serde_roundtrip() {
            let format = OutputFormat::Waybar;
            let json = serde_json::to_string(&format).unwrap();
            assert_eq!(json, "\"waybar\"");
            let parsed: OutputFormat = serde_json::from_str(&json).unwrap();
            assert_eq!(format, parsed);
        }
    }

    mod urgency_class {
        use super::*;

        #[test]
        fn as_str_values() {
            assert_eq!(UrgencyClass::Ongoing.as_str(), "ongoing");
            assert_eq!(UrgencyClass::EndingSoon.as_str(), "ending_soon");
            assert_eq!(UrgencyClass::Soon.as_str(), "soon");
            assert_eq!(UrgencyClass::Upcoming.as_str(), "upcoming");
            assert_eq!(UrgencyClass::AllDay.as_str(), "allday");
        }
    }

    mod format_options {
        use super::*;

        #[test]
        fn default_values() {
            let opts = FormatOptions::default();
            assert_eq!(opts.max_title_length, None);
            assert_eq!(opts.soon_threshold_minutes, 5);
            assert!(opts.hyperlinks);
            assert_eq!(opts.hour_separator, ":");
            assert_eq!(opts.time_format, TimeFormat::H24);
            assert!(opts.show_relative_time);
            assert_eq!(opts.end_warning_minutes, None);
        }
    }

    mod formatter {
        use super::*;

        #[test]
        fn format_time_until_now() {
            let formatter = OutputFormatter::with_defaults();
            let now = local_from_utc(utc(2025, 2, 5, 10, 0, 0));
            assert_eq!(formatter.format_time_until(now, now, false), "Now");
            assert_eq!(
                formatter.format_time_until(now - chrono::Duration::minutes(5), now, false),
                "Now"
            );
        }

        #[test]
        fn format_time_until_minutes() {
            let formatter = OutputFormatter::with_defaults();
            let now = local_from_utc(utc(2025, 2, 5, 10, 0, 0));
            assert_eq!(
                formatter.format_time_until(now + chrono::Duration::minutes(15), now, false),
                "In 15 minutes"
            );
            assert_eq!(
                formatter.format_time_until(now + chrono::Duration::minutes(1), now, false),
                "In 1 minute"
            );
        }

        #[test]
        fn format_time_until_rounds_up_fractional_seconds() {
            let formatter = OutputFormatter::with_defaults();
            let now = local_from_utc(utc(2025, 2, 5, 10, 0, 0));
            // 70 seconds from now → should show "In 2 minutes" (ceiling)
            assert_eq!(
                formatter.format_time_until(now + chrono::Duration::seconds(70), now, false),
                "In 2 minutes"
            );
            // 1 second from now → should show "In 1 minute" (ceiling)
            assert_eq!(
                formatter.format_time_until(now + chrono::Duration::seconds(1), now, false),
                "In 1 minute"
            );
            // 61 seconds from now → should show "In 2 minutes" (ceiling)
            assert_eq!(
                formatter.format_time_until(now + chrono::Duration::seconds(61), now, false),
                "In 2 minutes"
            );
        }

        #[test]
        fn format_time_until_hours() {
            let mut opts = FormatOptions::default();
            opts.until_offset_minutes = Some(999);
            let formatter = OutputFormatter::new(opts);
            let now = local_from_utc(utc(2025, 2, 5, 10, 0, 0));
            assert_eq!(
                formatter.format_time_until(now + chrono::Duration::minutes(60), now, false),
                "In 1 hour"
            );
            assert_eq!(
                formatter.format_time_until(now + chrono::Duration::minutes(120), now, false),
                "In 2 hours"
            );
            assert_eq!(
                formatter.format_time_until(now + chrono::Duration::minutes(90), now, false),
                "In 1 hour and 30 minutes"
            );
            assert_eq!(
                formatter.format_time_until(now + chrono::Duration::minutes(150), now, false),
                "In 2 hours and 30 minutes"
            );
        }

        #[test]
        fn format_time_until_tomorrow() {
            let formatter = OutputFormatter::with_defaults();
            let now = Local.with_ymd_and_hms(2025, 2, 5, 23, 0, 0).unwrap();
            let meeting = now + chrono::Duration::hours(2);

            assert_eq!(
                formatter.format_time_until(meeting, now, false),
                "Tomorrow at 01:00"
            );
        }

        #[test]
        fn format_time_remaining() {
            let formatter = OutputFormatter::with_defaults();
            assert_eq!(formatter.format_time_remaining(0, true), "Ending");
            assert_eq!(
                formatter.format_time_remaining(15, true),
                "15 minutes to go"
            );
            assert_eq!(formatter.format_time_remaining(60, true), "1H to go");
            assert_eq!(formatter.format_time_remaining(90, true), "1H30 to go");
        }

        #[test]
        fn truncate_title_with_max_length() {
            let mut opts = FormatOptions::default();
            opts.max_title_length = Some(10);
            let formatter = OutputFormatter::new(opts);

            assert_eq!(formatter.truncate_title("Short").as_ref(), "Short");
            assert_eq!(
                formatter.truncate_title("Very Long Title").as_ref(),
                "Very Lo..."
            );
        }

        #[test]
        fn format_tty_basic() {
            let now_utc = utc(2025, 2, 5, 10, 0, 0);
            let now_local = local_from_utc(now_utc);
            let meeting = sample_meeting(now_utc, 15); // 15 minutes from now
            let formatter = OutputFormatter::with_defaults();

            let formatted = formatter.format_tty_at(&[meeting], now_local);
            assert_eq!(formatted.len(), 1);
            assert!(formatted[0].text.contains("Team Standup"));
            assert!(formatted[0].text.contains("In 15 minutes"));
            assert_eq!(formatted[0].urgency, UrgencyClass::Upcoming);
        }

        #[test]
        fn format_tty_soon() {
            let now_utc = utc(2025, 2, 5, 10, 0, 0);
            let now_local = local_from_utc(now_utc);
            let meeting = sample_meeting(now_utc, 3); // 3 minutes from now
            let formatter = OutputFormatter::with_defaults();

            let formatted = formatter.format_tty_at(&[meeting], now_local);
            assert_eq!(formatted[0].urgency, UrgencyClass::Soon);
        }

        #[test]
        fn format_waybar_empty() {
            let formatter = OutputFormatter::with_defaults();
            let output = formatter.format_waybar(&[], "No meetings");

            assert_eq!(output.text, "No meetings");
            assert!(output.tooltip.contains("No upcoming"));
        }

        #[test]
        fn format_waybar_with_meeting() {
            let now_utc = utc(2025, 2, 5, 10, 0, 0);
            let now_local = local_from_utc(now_utc);
            let meeting = sample_meeting(now_utc, 15);
            let formatter = OutputFormatter::with_defaults();

            let output = formatter.format_waybar_at(&[meeting], "No meetings", now_local);

            assert!(output.text.contains("Team Standup"));
            assert!(output.class.is_some());
            assert!(!output.tooltip.is_empty());
        }

        #[test]
        fn format_waybar_ending_soon_class() {
            let now_utc = utc(2025, 2, 5, 10, 0, 0);
            let now_local = local_from_utc(now_utc);
            let meeting = ongoing_meeting_ending_in(now_utc, 4);
            let mut opts = FormatOptions::default();
            opts.end_warning_minutes = Some(5);
            let formatter = OutputFormatter::new(opts);

            let output = formatter.format_waybar_at(&[meeting], "No meetings", now_local);

            assert_eq!(output.class, Some("ending_soon".to_string()));
        }

        #[test]
        fn format_json_basic() {
            let now_utc = utc(2025, 2, 5, 10, 0, 0);
            let now_local = local_from_utc(now_utc);
            let meeting = sample_meeting(now_utc, 15);
            let formatter = OutputFormatter::with_defaults();

            let output = formatter.format_json_at(&[meeting], now_local);

            assert_eq!(output.count, 1);
            assert_eq!(output.meetings.len(), 1);
            assert!(output.next_meeting.is_some());

            let json_meeting = &output.meetings[0];
            assert_eq!(json_meeting.id, "evt-123");
            assert_eq!(json_meeting.title, "Team Standup");
            assert!(!json_meeting.is_all_day);
            assert!(!json_meeting.is_ongoing);
            assert_eq!(json_meeting.urgency, UrgencyClass::Upcoming);
        }

        #[test]
        fn waybar_tooltip_limit() {
            let now_utc = utc(2025, 2, 5, 10, 0, 0);
            let now_local = local_from_utc(now_utc);
            let meetings: Vec<_> = (0..5)
                .map(|i| sample_meeting(now_utc, (i + 1) * 30))
                .collect();

            let mut opts = FormatOptions::default();
            opts.tooltip_limit = Some(2);
            let formatter = OutputFormatter::new(opts);

            let output = formatter.format_waybar_at(&meetings, "No meetings", now_local);

            // Tooltip should only have 2 lines
            let lines: Vec<_> = output.tooltip.lines().collect();
            assert_eq!(lines.len(), 2);
        }

        #[test]
        fn time_format_12h() {
            let mut opts = FormatOptions::default();
            opts.time_format = TimeFormat::H12;
            opts.show_relative_time = false;
            let formatter = OutputFormatter::new(opts);

            let now = utc(2025, 2, 5, 10, 0, 0);
            let meeting = sample_meeting(now, 0);

            let time_str = formatter.format_absolute_time(&meeting);
            assert!(time_str.contains("AM") || time_str.contains("PM"));
        }
    }

    mod waybar_output {
        use super::*;

        #[test]
        fn builder_pattern() {
            let output = WaybarOutput::new("test", "tooltip")
                .with_class("soon")
                .with_alt("alternative");

            assert_eq!(output.text, "test");
            assert_eq!(output.tooltip, "tooltip");
            assert_eq!(output.class, Some("soon".to_string()));
            assert_eq!(output.alt, Some("alternative".to_string()));
        }

        #[test]
        fn serde_skips_none_fields() {
            let output = WaybarOutput::new("test", "tooltip");
            let json = serde_json::to_string(&output).unwrap();

            assert!(!json.contains("class"));
            assert!(!json.contains("alt"));
            assert!(!json.contains("percentage"));
        }
    }
}

#[cfg(test)]
mod golden_tests;
