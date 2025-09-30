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
