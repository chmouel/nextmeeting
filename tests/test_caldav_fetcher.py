import datetime
from argparse import Namespace

import dateutil.parser as dtparse
import pytest

pytest.importorskip("caldav")
pytest.importorskip("icalendar")

from nextmeeting.caldav import CalDavMeetingFetcher, _as_local_datetime
from nextmeeting.cli import Meeting


class _StubEvent:
    def __init__(self, data: bytes):
        self.data = data


def _expected_local(value: str) -> datetime.datetime:
    return dtparse.isoparse(value).astimezone().replace(tzinfo=None)


def test_meetings_from_event_with_url_and_all_day():
    ics = (
        "BEGIN:VCALENDAR\n"
        "VERSION:2.0\n"
        "BEGIN:VEVENT\n"
        "UID:demo-1\n"
        "SUMMARY:CalDAV Demo\n"
        "DTSTART:20240925T140000Z\n"
        "DTEND:20240925T150000Z\n"
        "URL:https://meet.example.com/demo\n"
        "END:VEVENT\n"
        "BEGIN:VEVENT\n"
        "UID:demo-2\n"
        "SUMMARY:All Day Sync\n"
        "DTSTART;VALUE=DATE:20240926\n"
        "DTEND;VALUE=DATE:20240927\n"
        "DESCRIPTION:Join via https://example.com/all\n"
        "END:VEVENT\n"
        "END:VCALENDAR\n"
    ).encode("utf-8")

    fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)
    args = Namespace(caldav_url="https://example.com/caldav")
    meetings = fetcher._meetings_from_event(_StubEvent(ics), args)

    assert len(meetings) == 2

    first = meetings[0]
    assert isinstance(first, Meeting)
    assert first.title == "CalDAV Demo"
    assert first.calendar_url == "https://meet.example.com/demo"
    assert first.meet_url == "https://meet.example.com/demo"
    assert first.start_time == _expected_local("2024-09-25T14:00:00+00:00")
    assert first.end_time == _expected_local("2024-09-25T15:00:00+00:00")

    second = meetings[1]
    assert second.is_all_day
    assert second.meet_url == "https://example.com/all"
    assert second.calendar_url == "https://example.com/caldav"


def test_as_local_datetime_with_date_and_timezone():
    aware = dtparse.isoparse("2024-01-01T12:30:00+02:00")
    naive = _as_local_datetime(aware)
    assert naive.tzinfo is None

    allday = _as_local_datetime(datetime.date(2024, 9, 1))
    assert allday == datetime.datetime(2024, 9, 1, 0, 0)


def test_zoom_link_in_description():
    """Test CalDAV event with Zoom link in description."""
    ics = (
        "BEGIN:VCALENDAR\n"
        "VERSION:2.0\n"
        "BEGIN:VEVENT\n"
        "UID:zoom-1\n"
        "SUMMARY:Zoom Team Sync\n"
        "DTSTART:20240925T140000Z\n"
        "DTEND:20240925T150000Z\n"
        "DESCRIPTION:Join via https://zoom.us/j/123456789?pwd=abc123\n"
        "END:VEVENT\n"
        "END:VCALENDAR\n"
    ).encode("utf-8")

    fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)
    args = Namespace(caldav_url="https://example.com/caldav")
    meetings = fetcher._meetings_from_event(_StubEvent(ics), args)

    assert len(meetings) == 1
    meeting = meetings[0]
    assert meeting.meet_url == "https://zoom.us/j/123456789?pwd=abc123"
    assert meeting.title == "Zoom Team Sync"


def test_google_meet_link_in_description():
    """Test CalDAV event with Google Meet link in description."""
    ics = (
        "BEGIN:VCALENDAR\n"
        "VERSION:2.0\n"
        "BEGIN:VEVENT\n"
        "UID:meet-1\n"
        "SUMMARY:Google Meet Standup\n"
        "DTSTART:20240925T100000Z\n"
        "DTEND:20240925T103000Z\n"
        "DESCRIPTION:Daily standup\\n\\nJoin: https://meet.google.com/abc-defg-hij\n"
        "END:VEVENT\n"
        "END:VCALENDAR\n"
    ).encode("utf-8")

    fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)
    args = Namespace(caldav_url="https://example.com/caldav")
    meetings = fetcher._meetings_from_event(_StubEvent(ics), args)

    assert len(meetings) == 1
    meeting = meetings[0]
    assert meeting.meet_url == "https://meet.google.com/abc-defg-hij"
    assert meeting.title == "Google Meet Standup"


def test_teams_link_in_location():
    """Test CalDAV event with Teams link in location field."""
    ics = (
        "BEGIN:VCALENDAR\n"
        "VERSION:2.0\n"
        "BEGIN:VEVENT\n"
        "UID:teams-1\n"
        "SUMMARY:Teams Planning\n"
        "DTSTART:20240925T150000Z\n"
        "DTEND:20240925T160000Z\n"
        "LOCATION:https://teams.microsoft.com/l/meetup-join/19%3ameeting_xyz\n"
        "END:VEVENT\n"
        "END:VCALENDAR\n"
    ).encode("utf-8")

    fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)
    args = Namespace(caldav_url="https://example.com/caldav")
    meetings = fetcher._meetings_from_event(_StubEvent(ics), args)

    assert len(meetings) == 1
    meeting = meetings[0]
    assert "teams.microsoft.com" in meeting.meet_url
    assert meeting.title == "Teams Planning"


def test_url_property_takes_precedence():
    """Test that URL property takes precedence over description links."""
    ics = (
        "BEGIN:VCALENDAR\n"
        "VERSION:2.0\n"
        "BEGIN:VEVENT\n"
        "UID:precedence-1\n"
        "SUMMARY:Mixed URLs\n"
        "DTSTART:20240925T140000Z\n"
        "DTEND:20240925T150000Z\n"
        "URL:https://primary.example.com/meeting\n"
        "DESCRIPTION:Also available at https://zoom.us/j/999999999\n"
        "END:VEVENT\n"
        "END:VCALENDAR\n"
    ).encode("utf-8")

    fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)
    args = Namespace(caldav_url="https://example.com/caldav")
    meetings = fetcher._meetings_from_event(_StubEvent(ics), args)

    assert len(meetings) == 1
    meeting = meetings[0]
    # URL property should take precedence
    assert meeting.meet_url == "https://primary.example.com/meeting"


def test_no_meeting_link():
    """Test CalDAV event with no meeting link."""
    ics = (
        "BEGIN:VCALENDAR\n"
        "VERSION:2.0\n"
        "BEGIN:VEVENT\n"
        "UID:no-link-1\n"
        "SUMMARY:In-Person Meeting\n"
        "DTSTART:20240925T140000Z\n"
        "DTEND:20240925T150000Z\n"
        "LOCATION:Conference Room A\n"
        "DESCRIPTION:Please arrive 5 minutes early\n"
        "END:VEVENT\n"
        "END:VCALENDAR\n"
    ).encode("utf-8")

    fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)
    args = Namespace(caldav_url="https://example.com/caldav")
    meetings = fetcher._meetings_from_event(_StubEvent(ics), args)

    assert len(meetings) == 1
    meeting = meetings[0]
    assert meeting.meet_url is None
    assert meeting.calendar_url == "https://example.com/caldav"
