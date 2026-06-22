"""Tests for multi-calendar functionality in CalDAV integration."""

import datetime
from argparse import Namespace
from unittest.mock import MagicMock, Mock, patch

import pytest

pytest.importorskip("caldav")
pytest.importorskip("icalendar")

from nextmeeting.caldav import CalDavMeetingFetcher
from nextmeeting.cli import Meeting


class _StubCalendar:
    """Mock calendar object for testing."""

    def __init__(self, url: str, name: str = None):
        self.url = Mock()
        self.url.to_python = Mock(return_value=url)
        self.name = name or url.split("/")[-1]
        self._display_name = name

    def get_properties(self, props):
        if self._display_name:
            return {"{DAV:}displayname": self._display_name}
        return {}

    def date_search(self, start, end):
        """Return empty list by default."""
        return []


class _StubEvent:
    """Mock CalDAV event for testing."""

    def __init__(self, summary: str, start: str, end: str, calendar_name: str = None):
        # Create minimal iCalendar data
        ics = (
            f"BEGIN:VCALENDAR\n"
            f"VERSION:2.0\n"
            f"BEGIN:VEVENT\n"
            f"UID:test-{summary}\n"
            f"SUMMARY:{summary}\n"
            f"DTSTART:{start}\n"
            f"DTEND:{end}\n"
            f"END:VEVENT\n"
            f"END:VCALENDAR\n"
        ).encode("utf-8")
        self.data = ics


def test_resolve_calendars_empty_list_returns_all():
    """When no calendars specified, should fetch from all available."""
    fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)

    # Mock client with 3 available calendars
    mock_client = Mock()
    mock_principal = Mock()
    cal1 = _StubCalendar("https://example.com/cal1", "Calendar 1")
    cal2 = _StubCalendar("https://example.com/cal2", "Calendar 2")
    cal3 = _StubCalendar("https://example.com/cal3", "Calendar 3")
    mock_principal.calendars.return_value = [cal1, cal2, cal3]
    mock_client.principal.return_value = mock_principal

    # Args with empty calendar list
    args = Namespace(caldav_calendar=[], caldav_url="https://example.com/")

    resolved = fetcher._resolve_calendars(mock_client, args)

    # Should return all 3 calendars
    assert len(resolved) == 3
    assert resolved[0][1] == "Calendar 1"
    assert resolved[1][1] == "Calendar 2"
    assert resolved[2][1] == "Calendar 3"


def test_resolve_calendars_multiple_hints():
    """Should resolve each hint to a calendar object."""
    fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)

    mock_client = Mock()
    mock_principal = Mock()

    # Setup calendars
    cal1 = _StubCalendar("https://example.com/cal1", "Work")
    cal2 = _StubCalendar("https://example.com/cal2", "Personal")
    cal3 = _StubCalendar("https://example.com/cal3", "Other")
    mock_principal.calendars.return_value = [cal1, cal2, cal3]
    mock_client.principal.return_value = mock_principal

    # Mock calendar() method to return calendar by URL
    # When calendar hints are URLs, they bypass caldav_url and use the hint directly
    def mock_calendar_lookup(url):
        url_map = {
            "https://example.com/cal1": cal1,
            "https://example.com/cal2": cal2,
        }
        if url in url_map:
            return url_map[url]
        raise ValueError(f"Calendar not found: {url}")

    mock_client.calendar = Mock(side_effect=mock_calendar_lookup)

    # Args with multiple calendar hints (URLs)
    # Don't set caldav_url when using direct calendar URLs
    args = Namespace(
        caldav_calendar=["https://example.com/cal1", "https://example.com/cal2"],
        caldav_url=None,
    )

    resolved = fetcher._resolve_calendars(mock_client, args)

    # Should resolve both calendars
    assert len(resolved) == 2
    assert resolved[0][1] == "Work"
    assert resolved[1][1] == "Personal"


def test_resolve_calendars_deduplicates():
    """Same calendar resolved twice should only appear once."""
    fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)

    mock_client = Mock()
    cal1 = _StubCalendar("https://example.com/cal1", "Work")

    # Mock calendar() to always return same calendar
    mock_client.calendar = Mock(return_value=cal1)

    # Mock principal for fallback
    mock_principal = Mock()
    mock_principal.calendars.return_value = [cal1]
    mock_client.principal.return_value = mock_principal

    # Args with duplicate calendar hints (same URL different ways)
    args = Namespace(
        caldav_calendar=[
            "https://example.com/cal1",
            "https://example.com/cal1/",  # Same URL with trailing slash
        ],
        caldav_url="https://example.com/",
    )

    resolved = fetcher._resolve_calendars(mock_client, args)

    # Should only appear once (deduplication by URL)
    assert len(resolved) == 1
    assert resolved[0][1] == "Work"


def test_fetch_meetings_aggregates_calendars():
    """Events from multiple calendars should be merged and sorted."""
    fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)

    # Create mock calendars with events at different times
    cal1 = _StubCalendar("https://example.com/cal1", "Work")
    cal2 = _StubCalendar("https://example.com/cal2", "Personal")

    # Events from cal1 (starts in 2 hours and 4 hours)
    now = datetime.datetime.now(datetime.timezone.utc)
    event1_start = now + datetime.timedelta(hours=2)
    event1_end = event1_start + datetime.timedelta(hours=1)
    event3_start = now + datetime.timedelta(hours=4)
    event3_end = event3_start + datetime.timedelta(hours=1)

    # Event from cal2 (starts in 3 hours - should be in middle)
    event2_start = now + datetime.timedelta(hours=3)
    event2_end = event2_start + datetime.timedelta(hours=1)

    cal1_events = [
        _StubEvent(
            "Work Meeting 1",
            event1_start.strftime("%Y%m%dT%H%M%SZ"),
            event1_end.strftime("%Y%m%dT%H%M%SZ"),
        ),
        _StubEvent(
            "Work Meeting 2",
            event3_start.strftime("%Y%m%dT%H%M%SZ"),
            event3_end.strftime("%Y%m%dT%H%M%SZ"),
        ),
    ]
    cal2_events = [
        _StubEvent(
            "Personal Meeting",
            event2_start.strftime("%Y%m%dT%H%M%SZ"),
            event2_end.strftime("%Y%m%dT%H%M%SZ"),
        )
    ]

    cal1.date_search = Mock(return_value=cal1_events)
    cal2.date_search = Mock(return_value=cal2_events)

    # Mock client
    mock_client = Mock()
    mock_principal = Mock()
    mock_principal.calendars.return_value = [cal1, cal2]
    mock_client.principal.return_value = mock_principal

    args = Namespace(
        caldav_url="https://example.com/",
        caldav_calendar=[],
        caldav_lookbehind_hours=12,
        caldav_lookahead_hours=48,
        verbose=False,
        debug=False,
    )

    with patch("nextmeeting.caldav.DAVClient", return_value=mock_client):
        meetings = fetcher.fetch_meetings(args)

    # Should have 3 meetings total
    assert len(meetings) == 3

    # Should be sorted by start time
    assert meetings[0].title == "Work Meeting 1"
    assert meetings[1].title == "Personal Meeting"
    assert meetings[2].title == "Work Meeting 2"

    # Should have calendar names attached
    assert meetings[0].calendar_name == "Work"
    assert meetings[1].calendar_name == "Personal"
    assert meetings[2].calendar_name == "Work"


def test_fetch_meetings_partial_failure_continues():
    """If one calendar fails, should continue with others."""
    fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)

    # Create calendars where one will fail
    cal1 = _StubCalendar("https://example.com/cal1", "Work")
    cal2 = _StubCalendar("https://example.com/cal2", "Personal")

    # cal1 will raise an error
    cal1.date_search = Mock(side_effect=Exception("Calendar unavailable"))

    # cal2 will succeed
    now = datetime.datetime.now(datetime.timezone.utc)
    event_start = now + datetime.timedelta(hours=2)
    event_end = event_start + datetime.timedelta(hours=1)
    cal2_events = [
        _StubEvent(
            "Personal Meeting",
            event_start.strftime("%Y%m%dT%H%M%SZ"),
            event_end.strftime("%Y%m%dT%H%M%SZ"),
        )
    ]
    cal2.date_search = Mock(return_value=cal2_events)

    # Mock client
    mock_client = Mock()
    mock_principal = Mock()
    mock_principal.calendars.return_value = [cal1, cal2]
    mock_client.principal.return_value = mock_principal

    args = Namespace(
        caldav_url="https://example.com/",
        caldav_calendar=[],
        caldav_lookbehind_hours=12,
        caldav_lookahead_hours=48,
        verbose=False,
        debug=False,
    )

    with patch("nextmeeting.caldav.DAVClient", return_value=mock_client):
        # Should not raise exception even though cal1 failed
        meetings = fetcher.fetch_meetings(args)

    # Should still have meeting from cal2
    assert len(meetings) == 1
    assert meetings[0].title == "Personal Meeting"
    assert meetings[0].calendar_name == "Personal"


def test_calendar_name_in_format_template():
    """The {calendar_name} placeholder should work in format strings."""
    from nextmeeting.cli import MeetingFormatter

    # Create a meeting with calendar_name
    now = datetime.datetime.now()
    meeting = Meeting(
        title="Team Standup",
        start_time=now + datetime.timedelta(minutes=30),
        end_time=now + datetime.timedelta(minutes=60),
        calendar_url="https://example.com/cal1",
        meet_url="https://meet.example.com/abc",
        calendar_name="Work Calendar",
    )

    # Create args with custom format including calendar_name
    args = Namespace(
        format="{title} ({calendar_name}) at {when}",
        privacy=False,
        waybar=False,
        notify_min_before_events=5,
        notify_min_color="#FF0000",
        notify_min_color_foreground="#FFFFFF",
        cache_dir="/tmp",
        notify_icon="",
        notify_expiry=0,
        hour_separator=":",
        until_offset=60,
    )

    formatter = MeetingFormatter(args)
    formatted, _ = formatter.format_meeting(meeting, hyperlink=False)

    # Should include calendar name in output
    assert "Work Calendar" in formatted
    assert "Team Standup" in formatted


def test_config_array_not_accumulated_with_cli():
    """CLI --caldav-calendar should replace config, not append."""
    import tempfile
    import os
    import sys
    from pathlib import Path
    from unittest.mock import patch

    from nextmeeting.cli import parse_args

    # Create a config file with caldav_calendar set to "ConfigCal"
    with tempfile.NamedTemporaryFile(mode="w", suffix=".toml", delete=False) as f:
        f.write(
            """[nextmeeting]
caldav-url = "https://example.com/caldav"
caldav-calendar = ["ConfigCalendar1", "ConfigCalendar2"]
"""
        )
        config_path = f.name

    try:
        # Clear environment variables
        with patch.dict(os.environ, {}, clear=True):
            original_argv = sys.argv
            try:
                # Simulate CLI with --caldav-calendar flag
                sys.argv = [
                    "nextmeeting",
                    "--config",
                    config_path,
                    "--caldav-calendar",
                    "CLICalendar",
                ]
                args = parse_args()

                # CLI value should REPLACE config value, not append to it
                # Since argparse action='append' starts with None, the first CLI value
                # becomes a list with just that value
                assert args.caldav_calendar == ["CLICalendar"]
                assert "ConfigCalendar1" not in args.caldav_calendar
                assert "ConfigCalendar2" not in args.caldav_calendar

            finally:
                sys.argv = original_argv
    finally:
        os.unlink(config_path)


def test_resolve_calendars_all_fail():
    """If all calendars fail to resolve, should raise error."""
    fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)

    mock_client = Mock()
    mock_principal = Mock()
    mock_principal.calendars.return_value = []
    mock_client.principal.return_value = mock_principal

    # Mock calendar() to raise error
    mock_client.calendar = Mock(side_effect=Exception("Not found"))

    args = Namespace(
        caldav_calendar=["NonexistentCal"],
        caldav_url="https://example.com/",
        verbose=False,
        debug=False,
    )

    # Should raise RuntimeError when no calendars resolve
    with pytest.raises(RuntimeError, match="Failed to resolve any calendars"):
        fetcher._resolve_calendars(mock_client, args)


def test_fetch_meetings_all_calendars_fail():
    """If all calendars fail to fetch, should raise error."""
    fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)

    # Create calendars that will all fail
    cal1 = _StubCalendar("https://example.com/cal1", "Work")
    cal2 = _StubCalendar("https://example.com/cal2", "Personal")

    # Both will raise errors
    cal1.date_search = Mock(side_effect=Exception("Calendar 1 unavailable"))
    cal2.date_search = Mock(side_effect=Exception("Calendar 2 unavailable"))

    # Mock client
    mock_client = Mock()
    mock_principal = Mock()
    mock_principal.calendars.return_value = [cal1, cal2]
    mock_client.principal.return_value = mock_principal

    args = Namespace(
        caldav_url="https://example.com/",
        caldav_calendar=[],
        caldav_lookbehind_hours=12,
        caldav_lookahead_hours=48,
        verbose=False,
        debug=False,
    )

    with patch("nextmeeting.caldav.DAVClient", return_value=mock_client):
        # Should raise RuntimeError when all calendars fail
        with pytest.raises(RuntimeError, match="Failed to fetch from all"):
            fetcher.fetch_meetings(args)
