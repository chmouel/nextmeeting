//! Golden tests for output formatting.
//!
//! These tests use insta for snapshot testing to ensure output format stability.
//! Run with `cargo insta review` to update snapshots after intentional changes.

use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};

use crate::event::{EventLink, LinkKind, MeetingView, NormalizedEvent};
use crate::format::{FormatOptions, OutputFormatter, TimeFormat};
use crate::time::EventTime;

/// Create a UTC datetime for testing.
fn utc(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, min, s).unwrap()
}

/// Create a date for all-day events.
#[allow(dead_code)]
fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

/// Convert UTC to local (for test assertions).
fn local_from_utc(dt: DateTime<Utc>) -> DateTime<Local> {
    dt.with_timezone(&Local)
}

/// Create a sample meeting starting at a given offset from `now`.
fn sample_meeting(now: DateTime<Utc>, offset_minutes: i64, title: &str) -> MeetingView {
    let start = now + chrono::Duration::minutes(offset_minutes);
    let end = start + chrono::Duration::minutes(30);

    let event = NormalizedEvent::new(
        format!("evt-{}", offset_minutes),
        title,
        EventTime::from_utc(start),
        EventTime::from_utc(end),
        "primary",
    )
    .with_link(EventLink::new(
        LinkKind::GoogleMeet,
        format!("https://meet.google.com/abc-{}", offset_minutes),
    ))
    .with_calendar_url(format!(
        "https://calendar.google.com/event/{}",
        offset_minutes
    ));

    MeetingView::from_event(&event, now)
}

/// Create an ongoing meeting (started 10 minutes ago).
fn ongoing_meeting(now: DateTime<Utc>, title: &str) -> MeetingView {
    let start = now - chrono::Duration::minutes(10);
    let end = now + chrono::Duration::minutes(20);

    let event = NormalizedEvent::new(
        "evt-ongoing",
        title,
        EventTime::from_utc(start),
        EventTime::from_utc(end),
        "primary",
    )
    .with_link(EventLink::new(
        LinkKind::Zoom,
        "https://zoom.us/j/123456789",
    ));

    MeetingView::from_event(&event, now)
}

/// Create an ongoing meeting that is about to end.
fn ongoing_meeting_ending_in(
    now: DateTime<Utc>,
    title: &str,
    minutes_until_end: i64,
) -> MeetingView {
    let end = now + chrono::Duration::minutes(minutes_until_end);
    let start = end - chrono::Duration::minutes(30);

    let event = NormalizedEvent::new(
        "evt-ending-soon",
        title,
        EventTime::from_utc(start),
        EventTime::from_utc(end),
        "primary",
    )
    .with_link(EventLink::new(
        LinkKind::Zoom,
        "https://zoom.us/j/123456789",
    ));

    MeetingView::from_event(&event, now)
}

/// Create an all-day event.
fn all_day_meeting(now: DateTime<Utc>, title: &str) -> MeetingView {
    let today = now.date_naive();
    let tomorrow = today + chrono::Duration::days(1);

    let event = NormalizedEvent::new(
        "evt-allday",
        title,
        EventTime::from_date(today),
        EventTime::from_date(tomorrow),
        "primary",
    );

    MeetingView::from_event(&event, now)
}

/// The reference time for all golden tests: 2025-02-05 10:00:00 UTC.
/// Using a fixed time ensures reproducible snapshots.
fn reference_time() -> DateTime<Utc> {
    utc(2025, 2, 5, 10, 0, 0)
}

// =============================================================================
// TTY Output Golden Tests
// =============================================================================

#[test]
fn golden_tty_empty() {
    let formatter = OutputFormatter::with_defaults();
    let now = local_from_utc(reference_time());
    let meetings: Vec<MeetingView> = vec![];

    let output = formatter.format_tty_at(&meetings, now);

    insta::assert_debug_snapshot!("tty_empty", output);
}

#[test]
fn golden_tty_single_upcoming() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 15, "Team Standup")];

    let mut options = FormatOptions::default();
    options.hyperlinks = false; // Disable hyperlinks for readable snapshots
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_single_upcoming", output);
}

#[test]
fn golden_tty_soon_meeting() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 3, "Urgent Sync")];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_soon_meeting", output);
}

#[test]
fn golden_tty_ongoing() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![ongoing_meeting(now_utc, "Sprint Review")];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_ongoing", output);
}

#[test]
fn golden_tty_all_day() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![all_day_meeting(now_utc, "Company Holiday")];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_all_day", output);
}

#[test]
fn golden_tty_multiple_meetings() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![
        sample_meeting(now_utc, 15, "Team Standup"),
        sample_meeting(now_utc, 60, "1:1 with Manager"),
        sample_meeting(now_utc, 120, "Design Review"),
    ];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_multiple_meetings", output);
}

#[test]
fn golden_tty_title_truncation() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(
        now_utc,
        15,
        "Very Long Meeting Title That Should Be Truncated",
    )];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    options.max_title_length = Some(20);
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_title_truncation", output);
}

#[test]
fn golden_tty_with_hyperlinks() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 15, "Team Standup")];

    let mut options = FormatOptions::default();
    options.hyperlinks = true;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    // The hyperlink contains OSC8 escape sequences, we just verify structure
    assert_eq!(output.len(), 1);
    assert!(output[0].text.contains("Team Standup"));
    assert!(output[0].text.contains("\x1b]8;;")); // OSC8 start
}

// =============================================================================
// Waybar Output Golden Tests
// =============================================================================

#[test]
fn golden_waybar_empty() {
    let formatter = OutputFormatter::with_defaults();
    let now = local_from_utc(reference_time());

    let output = formatter.format_waybar_at(&[], "No meetings", now);

    insta::assert_json_snapshot!("waybar_empty", output);
}

#[test]
fn golden_waybar_single_upcoming() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 15, "Team Standup")];

    let formatter = OutputFormatter::with_defaults();
    let output = formatter.format_waybar_at(&meetings, "No meetings", now_local);

    insta::assert_json_snapshot!("waybar_single_upcoming", output);
}

#[test]
fn golden_waybar_soon() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 3, "Urgent Call")];

    let formatter = OutputFormatter::with_defaults();
    let output = formatter.format_waybar_at(&meetings, "No meetings", now_local);

    insta::assert_json_snapshot!("waybar_soon", output);
}

#[test]
fn golden_waybar_ongoing() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![ongoing_meeting(now_utc, "Sprint Review")];

    let formatter = OutputFormatter::with_defaults();
    let output = formatter.format_waybar_at(&meetings, "No meetings", now_local);

    insta::assert_json_snapshot!("waybar_ongoing", output);
}

#[test]
fn golden_waybar_ending_soon() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![ongoing_meeting_ending_in(now_utc, "Sprint Review", 4)];

    let mut options = FormatOptions::default();
    options.end_warning_minutes = Some(5);
    let formatter = OutputFormatter::new(options);
    let output = formatter.format_waybar_at(&meetings, "No meetings", now_local);

    insta::assert_json_snapshot!("waybar_ending_soon", output);
}

#[test]
fn golden_waybar_all_day_only() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![all_day_meeting(now_utc, "Company Holiday")];

    let formatter = OutputFormatter::with_defaults();
    let output = formatter.format_waybar_at(&meetings, "No meetings", now_local);

    insta::assert_json_snapshot!("waybar_all_day_only", output);
}

#[test]
fn golden_waybar_mixed_meetings() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![
        all_day_meeting(now_utc, "Company Holiday"),
        sample_meeting(now_utc, 15, "Team Standup"),
        sample_meeting(now_utc, 60, "1:1 Meeting"),
    ];

    let formatter = OutputFormatter::with_defaults();
    let output = formatter.format_waybar_at(&meetings, "No meetings", now_local);

    insta::assert_json_snapshot!("waybar_mixed_meetings", output);
}

#[test]
fn golden_waybar_tooltip_limit() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![
        sample_meeting(now_utc, 15, "Meeting 1"),
        sample_meeting(now_utc, 45, "Meeting 2"),
        sample_meeting(now_utc, 75, "Meeting 3"),
        sample_meeting(now_utc, 105, "Meeting 4"),
        sample_meeting(now_utc, 135, "Meeting 5"),
    ];

    let mut options = FormatOptions::default();
    options.tooltip_limit = Some(3);
    let formatter = OutputFormatter::new(options);
    let output = formatter.format_waybar_at(&meetings, "No meetings", now_local);

    insta::assert_json_snapshot!("waybar_tooltip_limit", output);
}

// =============================================================================
// JSON Output Golden Tests
// =============================================================================

#[test]
fn golden_json_empty() {
    let formatter = OutputFormatter::with_defaults();
    let now = local_from_utc(reference_time());

    let output = formatter.format_json_at(&[], now);

    insta::assert_json_snapshot!("json_empty", output);
}

#[test]
fn golden_json_single_upcoming() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 15, "Team Standup")];

    let formatter = OutputFormatter::with_defaults();
    let output = formatter.format_json_at(&meetings, now_local);

    // Use redaction for time-sensitive fields to make snapshots stable
    insta::assert_json_snapshot!("json_single_upcoming", output, {
        ".meetings[].start_time" => "[start_time]",
        ".meetings[].end_time" => "[end_time]",
        ".next_meeting.start_time" => "[start_time]",
        ".next_meeting.end_time" => "[end_time]",
    });
}

#[test]
fn golden_json_ongoing() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![ongoing_meeting(now_utc, "Sprint Review")];

    let formatter = OutputFormatter::with_defaults();
    let output = formatter.format_json_at(&meetings, now_local);

    insta::assert_json_snapshot!("json_ongoing", output, {
        ".meetings[].start_time" => "[start_time]",
        ".meetings[].end_time" => "[end_time]",
        ".next_meeting.start_time" => "[start_time]",
        ".next_meeting.end_time" => "[end_time]",
    });
}

#[test]
fn golden_json_multiple() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![
        sample_meeting(now_utc, 15, "Team Standup"),
        sample_meeting(now_utc, 60, "1:1 Meeting"),
        all_day_meeting(now_utc, "Holiday"),
    ];

    let formatter = OutputFormatter::with_defaults();
    let output = formatter.format_json_at(&meetings, now_local);

    insta::assert_json_snapshot!("json_multiple", output, {
        ".meetings[].start_time" => "[start_time]",
        ".meetings[].end_time" => "[end_time]",
        ".next_meeting.start_time" => "[start_time]",
        ".next_meeting.end_time" => "[end_time]",
    });
}

// =============================================================================
// Time Format Variations
// =============================================================================

#[test]
fn golden_time_format_12h() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 15, "Afternoon Meeting")];

    let mut options = FormatOptions::default();
    options.time_format = TimeFormat::H12;
    options.show_relative_time = false;
    options.hyperlinks = false;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_time_format_12h", output);
}

#[test]
fn golden_absolute_time() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 15, "Team Standup")];

    let mut options = FormatOptions::default();
    options.show_relative_time = false;
    options.hyperlinks = false;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_absolute_time", output);
}

#[test]
fn golden_custom_hour_separator() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 15, "Team Standup")];

    let mut options = FormatOptions::default();
    options.show_relative_time = false;
    options.hour_separator = "h".to_string();
    options.hyperlinks = false;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_custom_separator", output);
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn golden_meeting_starting_now() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 0, "Starting Now")];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_starting_now", output);
}

#[test]
fn golden_meeting_hours_away() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 150, "Later Today")]; // 2h 30m

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_hours_away", output);
}

#[test]
fn golden_meeting_exactly_one_hour() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 60, "In One Hour")];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_one_hour", output);
}

#[test]
fn golden_special_characters_in_title() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);

    let start = now_utc + chrono::Duration::minutes(15);
    let end = start + chrono::Duration::minutes(30);

    let event = NormalizedEvent::new(
        "evt-special",
        "Meeting: Q&A <Session> \"Test\"",
        EventTime::from_utc(start),
        EventTime::from_utc(end),
        "primary",
    );

    let meetings = vec![MeetingView::from_event(&event, now_utc)];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_special_characters", output);
}

// =============================================================================
// Custom Format Template Tests
// =============================================================================

#[test]
fn golden_custom_format_template() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 15, "Team Standup")];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    options.custom_format = Some("{when} | {title}".to_string());
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_custom_format", output);
}

#[test]
fn golden_custom_format_all_placeholders() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 15, "Team Standup")];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    options.custom_format = Some(
        "{title} @ {start_time} - {end_time} ({minutes_until}m) all_day={is_all_day} ongoing={is_ongoing}"
            .to_string(),
    );
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_custom_format_all_placeholders", output);
}

// =============================================================================
// Privacy Mode Tests
// =============================================================================

#[test]
fn golden_privacy_mode_tty() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 15, "Secret Meeting")];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    options.privacy = true;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_privacy_mode", output);
}

#[test]
fn golden_privacy_custom_title() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 15, "Secret Meeting")];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    options.privacy = true;
    options.privacy_title = "Meeting".to_string();
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_privacy_custom_title", output);
}

#[test]
fn golden_privacy_waybar() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 15, "Secret Meeting")];

    let mut options = FormatOptions::default();
    options.privacy = true;
    options.privacy_title = "Busy".to_string();
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_waybar_at(&meetings, "No meetings", now_local);

    insta::assert_json_snapshot!("waybar_privacy", output);
}

// =============================================================================
// Waybar Color Markup Tests
// =============================================================================

#[test]
fn golden_waybar_soon_with_colors() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 3, "Urgent Call")]; // 3 min = soon

    let mut options = FormatOptions::default();
    options.notify_min_color = Some("#ff0000".to_string());
    options.notify_min_color_foreground = Some("#ffffff".to_string());
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_waybar_at(&meetings, "No meetings", now_local);

    insta::assert_json_snapshot!("waybar_soon_with_colors", output);
}

#[test]
fn golden_waybar_hide_all_day() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![all_day_meeting(now_utc, "Holiday")];

    let mut options = FormatOptions::default();
    options.waybar_show_all_day = false;
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_waybar_at(&meetings, "No meetings", now_local);

    insta::assert_json_snapshot!("waybar_hide_all_day", output);
}

// =============================================================================
// Until Offset Tests
// =============================================================================

#[test]
fn golden_until_offset_within_threshold() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 15, "Team Standup")];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    options.until_offset_minutes = Some(30); // 30 min threshold, meeting is 15 min away -> show countdown
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_until_offset_within", output);
}

#[test]
fn golden_until_offset_beyond_threshold() {
    let now_utc = reference_time();
    let now_local = local_from_utc(now_utc);
    let meetings = vec![sample_meeting(now_utc, 90, "Later Meeting")];

    let mut options = FormatOptions::default();
    options.hyperlinks = false;
    options.until_offset_minutes = Some(30); // 30 min threshold, meeting is 90 min away -> show absolute time
    let formatter = OutputFormatter::new(options);

    let output = formatter.format_tty_at(&meetings, now_local);

    insta::assert_debug_snapshot!("tty_until_offset_beyond", output);
}
