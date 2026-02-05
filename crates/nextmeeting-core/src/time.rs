//! Time types for calendar events.
//!
//! This module provides [`EventTime`] for representing event start/end times
//! (which may be either a specific datetime or an all-day date), and
//! [`TimeWindow`] for defining query ranges.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Represents the time of a calendar event.
///
/// Calendar events can have two types of times:
/// - **DateTime**: A specific point in time (with timezone, stored as UTC)
/// - **AllDay**: A date without a specific time (all-day events)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum EventTime {
    /// A specific datetime, stored in UTC.
    DateTime(DateTime<Utc>),
    /// An all-day event date (no specific time).
    AllDay(NaiveDate),
}

impl EventTime {
    /// Creates a new `EventTime::DateTime` from a UTC datetime.
    pub fn from_utc(dt: DateTime<Utc>) -> Self {
        Self::DateTime(dt)
    }

    /// Creates a new `EventTime::DateTime` from a datetime in any timezone.
    pub fn from_local<Tz: TimeZone>(dt: DateTime<Tz>) -> Self {
        Self::DateTime(dt.with_timezone(&Utc))
    }

    /// Creates a new `EventTime::AllDay` from a date.
    pub fn from_date(date: NaiveDate) -> Self {
        Self::AllDay(date)
    }

    /// Returns `true` if this is an all-day event time.
    pub fn is_all_day(&self) -> bool {
        matches!(self, Self::AllDay(_))
    }

    /// Returns `true` if this is a specific datetime.
    pub fn is_datetime(&self) -> bool {
        matches!(self, Self::DateTime(_))
    }

    /// Returns the datetime if this is a `DateTime` variant.
    pub fn as_datetime(&self) -> Option<&DateTime<Utc>> {
        match self {
            Self::DateTime(dt) => Some(dt),
            Self::AllDay(_) => None,
        }
    }

    /// Returns the date if this is an `AllDay` variant.
    pub fn as_date(&self) -> Option<&NaiveDate> {
        match self {
            Self::AllDay(d) => Some(d),
            Self::DateTime(_) => None,
        }
    }

    /// Converts to a UTC datetime for comparison purposes.
    ///
    /// For all-day events, returns midnight UTC on that date.
    pub fn to_utc_datetime(&self) -> DateTime<Utc> {
        match self {
            Self::DateTime(dt) => *dt,
            Self::AllDay(date) => date.and_hms_opt(0, 0, 0).expect("valid time").and_utc(),
        }
    }

    /// Returns the date portion of this event time.
    pub fn date(&self) -> NaiveDate {
        match self {
            Self::DateTime(dt) => dt.date_naive(),
            Self::AllDay(date) => *date,
        }
    }

    /// Checks if this event time falls on the given date in the specified timezone.
    pub fn is_on_date<Tz: TimeZone>(&self, date: NaiveDate, tz: &Tz) -> bool {
        match self {
            Self::DateTime(dt) => dt.with_timezone(tz).date_naive() == date,
            Self::AllDay(d) => *d == date,
        }
    }

    /// Checks if this event time is before another event time.
    ///
    /// All-day events are compared at midnight UTC.
    pub fn is_before(&self, other: &EventTime) -> bool {
        self.to_utc_datetime() < other.to_utc_datetime()
    }

    /// Checks if this event time is after another event time.
    ///
    /// All-day events are compared at midnight UTC.
    pub fn is_after(&self, other: &EventTime) -> bool {
        self.to_utc_datetime() > other.to_utc_datetime()
    }

    /// Checks if this event time is before a given UTC datetime.
    pub fn is_before_utc(&self, dt: DateTime<Utc>) -> bool {
        self.to_utc_datetime() < dt
    }

    /// Checks if this event time is after a given UTC datetime.
    pub fn is_after_utc(&self, dt: DateTime<Utc>) -> bool {
        self.to_utc_datetime() > dt
    }
}

impl PartialOrd for EventTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EventTime {
    fn cmp(&self, other: &Self) -> Ordering {
        self.to_utc_datetime().cmp(&other.to_utc_datetime())
    }
}

/// A time window for querying calendar events.
///
/// Represents a half-open interval `[start, end)` in UTC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Start of the window (inclusive).
    pub start: DateTime<Utc>,
    /// End of the window (exclusive).
    pub end: DateTime<Utc>,
}

impl TimeWindow {
    /// Creates a new time window.
    ///
    /// # Panics
    ///
    /// Panics if `start` is after `end`.
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        assert!(start <= end, "TimeWindow start must be <= end");
        Self { start, end }
    }

    /// Creates a time window from a start time and duration.
    pub fn from_duration(start: DateTime<Utc>, duration: Duration) -> Self {
        Self::new(start, start + duration)
    }

    /// Creates a time window for a single day in the given timezone.
    pub fn for_date<Tz: TimeZone>(date: NaiveDate, tz: &Tz) -> Self {
        let start = tz
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("valid time"))
            .single()
            .expect("unambiguous local time")
            .with_timezone(&Utc);
        let end = tz
            .from_local_datetime(
                &date
                    .succ_opt()
                    .expect("valid successor date")
                    .and_hms_opt(0, 0, 0)
                    .expect("valid time"),
            )
            .single()
            .expect("unambiguous local time")
            .with_timezone(&Utc);
        Self { start, end }
    }

    /// Creates a time window for "today" starting from now until end of day.
    pub fn today_remaining<Tz: TimeZone>(now: DateTime<Utc>, tz: &Tz) -> Self {
        let local_now = now.with_timezone(tz);
        let today = local_now.date_naive();
        let end_of_day = tz
            .from_local_datetime(
                &today
                    .succ_opt()
                    .expect("valid successor date")
                    .and_hms_opt(0, 0, 0)
                    .expect("valid time"),
            )
            .single()
            .expect("unambiguous local time")
            .with_timezone(&Utc);
        Self {
            start: now,
            end: end_of_day,
        }
    }

    /// Creates a time window starting from now extending the given duration.
    pub fn from_now(now: DateTime<Utc>, duration: Duration) -> Self {
        Self::new(now, now + duration)
    }

    /// Returns the duration of this time window.
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }

    /// Checks if a datetime falls within this window.
    ///
    /// Uses half-open interval semantics: `[start, end)`.
    pub fn contains(&self, dt: DateTime<Utc>) -> bool {
        self.start <= dt && dt < self.end
    }

    /// Checks if an event time falls within this window.
    ///
    /// For all-day events, checks if midnight UTC falls within the window.
    pub fn contains_event_time(&self, et: &EventTime) -> bool {
        self.contains(et.to_utc_datetime())
    }

    /// Checks if an event with given start and end times overlaps with this window.
    ///
    /// An event overlaps if it starts before the window ends AND ends after the window starts.
    pub fn overlaps_event(&self, event_start: &EventTime, event_end: &EventTime) -> bool {
        let start = event_start.to_utc_datetime();
        let end = event_end.to_utc_datetime();
        start < self.end && end > self.start
    }

    /// Extends the window by the given duration on both ends.
    pub fn extend(&self, duration: Duration) -> Self {
        Self {
            start: self.start - duration,
            end: self.end + duration,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, s).unwrap()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    mod event_time {
        use super::*;

        #[test]
        fn datetime_creation() {
            let dt = utc(2025, 2, 5, 10, 30, 0);
            let et = EventTime::from_utc(dt);
            assert!(et.is_datetime());
            assert!(!et.is_all_day());
            assert_eq!(et.as_datetime(), Some(&dt));
            assert_eq!(et.as_date(), None);
        }

        #[test]
        fn allday_creation() {
            let d = date(2025, 2, 5);
            let et = EventTime::from_date(d);
            assert!(et.is_all_day());
            assert!(!et.is_datetime());
            assert_eq!(et.as_date(), Some(&d));
            assert_eq!(et.as_datetime(), None);
        }

        #[test]
        fn to_utc_datetime() {
            let dt = utc(2025, 2, 5, 10, 30, 0);
            let et_dt = EventTime::from_utc(dt);
            assert_eq!(et_dt.to_utc_datetime(), dt);

            let d = date(2025, 2, 5);
            let et_ad = EventTime::from_date(d);
            assert_eq!(et_ad.to_utc_datetime(), utc(2025, 2, 5, 0, 0, 0));
        }

        #[test]
        fn date_extraction() {
            let dt = utc(2025, 2, 5, 23, 59, 0);
            let et = EventTime::from_utc(dt);
            assert_eq!(et.date(), date(2025, 2, 5));

            let d = date(2025, 3, 15);
            let et = EventTime::from_date(d);
            assert_eq!(et.date(), d);
        }

        #[test]
        fn ordering() {
            let et1 = EventTime::from_utc(utc(2025, 2, 5, 10, 0, 0));
            let et2 = EventTime::from_utc(utc(2025, 2, 5, 11, 0, 0));
            let et3 = EventTime::from_date(date(2025, 2, 5));

            assert!(et3 < et1); // midnight < 10:00
            assert!(et1 < et2); // 10:00 < 11:00
            assert!(et1.is_before(&et2));
            assert!(et2.is_after(&et1));
        }

        #[test]
        fn serde_roundtrip() {
            let et_dt = EventTime::from_utc(utc(2025, 2, 5, 10, 30, 0));
            let json = serde_json::to_string(&et_dt).unwrap();
            let parsed: EventTime = serde_json::from_str(&json).unwrap();
            assert_eq!(et_dt, parsed);

            let et_ad = EventTime::from_date(date(2025, 2, 5));
            let json = serde_json::to_string(&et_ad).unwrap();
            let parsed: EventTime = serde_json::from_str(&json).unwrap();
            assert_eq!(et_ad, parsed);
        }
    }

    mod time_window {
        use super::*;

        #[test]
        fn creation() {
            let start = utc(2025, 2, 5, 9, 0, 0);
            let end = utc(2025, 2, 5, 17, 0, 0);
            let window = TimeWindow::new(start, end);
            assert_eq!(window.start, start);
            assert_eq!(window.end, end);
            assert_eq!(window.duration(), Duration::hours(8));
        }

        #[test]
        #[should_panic(expected = "start must be <= end")]
        fn invalid_window() {
            let start = utc(2025, 2, 5, 17, 0, 0);
            let end = utc(2025, 2, 5, 9, 0, 0);
            TimeWindow::new(start, end);
        }

        #[test]
        fn contains_datetime() {
            let window = TimeWindow::new(utc(2025, 2, 5, 9, 0, 0), utc(2025, 2, 5, 17, 0, 0));

            // Inside
            assert!(window.contains(utc(2025, 2, 5, 10, 0, 0)));
            assert!(window.contains(utc(2025, 2, 5, 16, 59, 59)));

            // Boundaries
            assert!(window.contains(utc(2025, 2, 5, 9, 0, 0))); // start inclusive
            assert!(!window.contains(utc(2025, 2, 5, 17, 0, 0))); // end exclusive

            // Outside
            assert!(!window.contains(utc(2025, 2, 5, 8, 59, 59)));
            assert!(!window.contains(utc(2025, 2, 5, 17, 0, 1)));
        }

        #[test]
        fn overlaps_event() {
            let window = TimeWindow::new(utc(2025, 2, 5, 9, 0, 0), utc(2025, 2, 5, 17, 0, 0));

            // Event fully inside window
            let start = EventTime::from_utc(utc(2025, 2, 5, 10, 0, 0));
            let end = EventTime::from_utc(utc(2025, 2, 5, 11, 0, 0));
            assert!(window.overlaps_event(&start, &end));

            // Event starts before, ends inside
            let start = EventTime::from_utc(utc(2025, 2, 5, 8, 0, 0));
            let end = EventTime::from_utc(utc(2025, 2, 5, 10, 0, 0));
            assert!(window.overlaps_event(&start, &end));

            // Event starts inside, ends after
            let start = EventTime::from_utc(utc(2025, 2, 5, 16, 0, 0));
            let end = EventTime::from_utc(utc(2025, 2, 5, 18, 0, 0));
            assert!(window.overlaps_event(&start, &end));

            // Event completely contains window
            let start = EventTime::from_utc(utc(2025, 2, 5, 8, 0, 0));
            let end = EventTime::from_utc(utc(2025, 2, 5, 18, 0, 0));
            assert!(window.overlaps_event(&start, &end));

            // Event ends at window start (no overlap)
            let start = EventTime::from_utc(utc(2025, 2, 5, 8, 0, 0));
            let end = EventTime::from_utc(utc(2025, 2, 5, 9, 0, 0));
            assert!(!window.overlaps_event(&start, &end));

            // Event starts at window end (no overlap)
            let start = EventTime::from_utc(utc(2025, 2, 5, 17, 0, 0));
            let end = EventTime::from_utc(utc(2025, 2, 5, 18, 0, 0));
            assert!(!window.overlaps_event(&start, &end));
        }

        #[test]
        fn for_date() {
            let window = TimeWindow::for_date(date(2025, 2, 5), &Utc);
            assert_eq!(window.start, utc(2025, 2, 5, 0, 0, 0));
            assert_eq!(window.end, utc(2025, 2, 6, 0, 0, 0));
            assert_eq!(window.duration(), Duration::hours(24));
        }

        #[test]
        fn from_duration() {
            let start = utc(2025, 2, 5, 10, 0, 0);
            let window = TimeWindow::from_duration(start, Duration::hours(2));
            assert_eq!(window.start, start);
            assert_eq!(window.end, utc(2025, 2, 5, 12, 0, 0));
        }

        #[test]
        fn extend() {
            let window = TimeWindow::new(utc(2025, 2, 5, 10, 0, 0), utc(2025, 2, 5, 12, 0, 0));
            let extended = window.extend(Duration::hours(1));
            assert_eq!(extended.start, utc(2025, 2, 5, 9, 0, 0));
            assert_eq!(extended.end, utc(2025, 2, 5, 13, 0, 0));
        }

        #[test]
        fn serde_roundtrip() {
            let window = TimeWindow::new(utc(2025, 2, 5, 9, 0, 0), utc(2025, 2, 5, 17, 0, 0));
            let json = serde_json::to_string(&window).unwrap();
            let parsed: TimeWindow = serde_json::from_str(&json).unwrap();
            assert_eq!(window, parsed);
        }
    }
}
