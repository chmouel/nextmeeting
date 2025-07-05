import datetime
import sys
from argparse import Namespace
from pathlib import Path

# pylint: disable=import-error
import pytest

sys.path.insert(0, str(Path(__file__).parent.parent / "src"))
# pylint: disable=wrong-import-position
from nextmeeting import cli

ELLIPSIS_LENGTH = 20


# Helper: create a fake Meeting
def make_meeting(
    title="Test Meeting",
    start_time=None,
    end_time=None,
    calendar_url=None,
    meet_url=None,
):
    now = datetime.datetime.now()
    if start_time is None:
        start_time = now + datetime.timedelta(minutes=10)
    if end_time is None:
        end_time = now + datetime.timedelta(minutes=70)
    if calendar_url is None:
        calendar_url = "https://calendar.google.com/test"
    if meet_url is None:
        meet_url = "https://meet.google.com/test"
    return cli.Meeting(
        title=title,
        start_time=start_time,
        end_time=end_time,
        calendar_url=calendar_url,
        meet_url=meet_url,
    )


FAKE_MEETING = make_meeting()
FAKE_MEETING_ONGOING = make_meeting(
    title="Ongoing Meeting",
    start_time=datetime.datetime.now() - datetime.timedelta(minutes=10),
    end_time=datetime.datetime.now() + datetime.timedelta(minutes=20),
    calendar_url="https://calendar.google.com/ongoing",
    meet_url="https://meet.google.com/ongoing",
)
FAKE_MEETING_PAST = make_meeting(
    title="Past Meeting",
    start_time=datetime.datetime.now() - datetime.timedelta(hours=2),
    end_time=datetime.datetime.now() - datetime.timedelta(hours=1, minutes=30),
    calendar_url="https://calendar.google.com/past",
    meet_url="https://meet.google.com/past",
)


# Test Meeting parsing from TSV
@pytest.mark.parametrize(
    ("tsv", "title"),
    [
        (
            "2025-07-04 10:00 2025-07-04 11:00 https://calendar.google.com/test https://meet.google.com/test Test Meeting",
            "Test Meeting",
        ),
    ],
)
def test_meeting_from_tsv(tsv, title):
    match = cli.REG_TSV.match(tsv)
    assert match is not None
    meeting = cli.Meeting.from_match(match)
    assert meeting.title == title
    assert meeting.calendar_url.startswith("https://calendar.google.com/")


# Test time-to-next-meeting logic
def test_time_until_start_and_end():
    m = FAKE_MEETING
    assert m.time_until_start.total_seconds() > 0
    assert m.time_until_end.total_seconds() > 0
    m2 = FAKE_MEETING_ONGOING
    assert m2.time_until_start.total_seconds() < 0
    assert m2.time_until_end.total_seconds() > 0


# Test is_ongoing property
def test_is_ongoing():
    assert FAKE_MEETING_ONGOING.is_ongoing
    assert not FAKE_MEETING.is_ongoing
    assert not FAKE_MEETING_PAST.is_ongoing


# Test ret_events output for upcoming and ongoing meetings
def test_ret_events_upcoming_and_ongoing():
    args = Namespace(
        today_only=False,
        waybar=False,
        skip_all_day_meeting=False,
        google_domain=None,
        notify_min_before_events=5,
        notify_min_color="#FF5733",
        notify_min_color_foreground="#F4F1DE",
        waybar_show_all_day_meeting=False,
    )
    rets, _ = cli.ret_events([FAKE_MEETING, FAKE_MEETING_ONGOING], args)
    assert any("to go" in r for r in rets) or any("In" in r for r in rets)


# Test notification logic (mock notify-send)
def test_notify(monkeypatch, tmp_path):
    args = Namespace(
        cache_dir=tmp_path,
        notify_expiry=0,
        notify_icon="/tmp/icon.svg",
        notify_min_before_events=5,
        debug=False,
    )
    monkeypatch.setattr(cli, "NOTIFY_PROGRAM", "/bin/true")
    called = {}

    def fake_call(_):
        called["called"] = True
        return 0

    monkeypatch.setattr(cli.subprocess, "call", fake_call)
    cli.notify(
        "Test",
        datetime.datetime.now(),
        datetime.datetime.now() + datetime.timedelta(minutes=1),
        args,
    )
    assert called.get("called")


# Test MeetingFetcher abstraction
def test_meetingfetcher_fetch_meetings(monkeypatch):
    fetcher = cli.MeetingFetcher()
    dummy_args = Namespace(
        gcalcli_cmdline="echo '2025-07-04 10:00 2025-07-04 11:00 https://calendar.google.com/test https://meet.google.com/test Test Meeting'",
        debug=False,
    )
    monkeypatch.setattr(cli, "process_lines", lambda _: [FAKE_MEETING])
    meetings = fetcher.fetch_meetings(dummy_args)
    assert meetings
    assert meetings[0].title == "Test Meeting"


# Test all-day meeting detection and skipping
def test_all_day_meeting_detection_and_skip():
    all_day = make_meeting(
        title="All Day Meeting",
        start_time=datetime.datetime.now().replace(
            hour=0, minute=0, second=0, microsecond=0
        ),
        end_time=(datetime.datetime.now() + datetime.timedelta(days=1)).replace(
            hour=0, minute=0, second=0, microsecond=0
        ),
    )
    assert all_day.is_all_day
    args = Namespace(
        today_only=False,
        waybar=False,
        skip_all_day_meeting=True,
        google_domain=None,
        notify_min_before_events=5,
        notify_min_color="#FF5733",
        notify_min_color_foreground="#F4F1DE",
        waybar_show_all_day_meeting=False,
    )
    rets, _ = cli.ret_events([all_day, FAKE_MEETING], args)
    # Only FAKE_MEETING should be present
    assert all("All Day" not in r for r in rets)


# Test ellipsis function
def test_ellipsis():
    s = "<b>Hello</b> World!" + "x" * 60
    result = cli.ellipsis(s, ELLIPSIS_LENGTH)
    assert result.endswith("...")
    assert "<b>" not in result
    assert len(result) <= ELLIPSIS_LENGTH
    s2 = "Short string"
    assert cli.ellipsis(s2, 50) == s2


# Test pretty_date for today, tomorrow, negative deltas
def test_pretty_date_cases():
    args = Namespace(
        notify_min_color="#FF5733", notify_min_color_foreground="#F4F1DE", waybar=True
    )
    now = datetime.datetime.now()
    # Today, in 10 minutes
    delta = cli.dtrel.relativedelta(minutes=10)
    s = cli.pretty_date(delta, now + datetime.timedelta(minutes=10), args)
    assert "In" in s
    # Tomorrow
    tomorrow = now + datetime.timedelta(days=1)
    delta_tomorrow = cli.dtrel.relativedelta(days=1)
    s2 = cli.pretty_date(delta_tomorrow, tomorrow, args)
    assert "Tomorrow" in s2 or tomorrow.strftime("%a %d") in s2
    # Negative delta (past)
    neg = cli.dtrel.relativedelta(minutes=-5)
    s3 = cli.pretty_date(neg, now - datetime.timedelta(minutes=5), args)
    assert s3 == "Now"


# Test notify cache prevents duplicate notifications
def test_notify_cache(tmp_path, monkeypatch):
    args = Namespace(
        cache_dir=tmp_path,
        notify_expiry=0,
        notify_icon="/tmp/icon.svg",
        notify_min_before_events=5,
        debug=False,
    )
    monkeypatch.setattr(cli, "NOTIFY_PROGRAM", "/bin/true")
    called = {}

    def fake_call(_):
        called["count"] = called.get("count", 0) + 1
        return 0

    monkeypatch.setattr(cli.subprocess, "call", fake_call)
    start = datetime.datetime.now() + datetime.timedelta(minutes=1)
    end = start + datetime.timedelta(minutes=30)
    cli.notify("Test", start, end, args)
    cli.notify("Test", start, end, args)
    assert called["count"] == 1  # Only one notification should be sent


# Test get_next_non_all_day_meeting and get_next_meeting
def test_get_next_helpers():
    # Make all_day meeting longer than 24 hours to ensure is_all_day is True
    all_day = make_meeting(
        title="All Day Meeting",
        start_time=datetime.datetime.now().replace(
            hour=0, minute=0, second=0, microsecond=0
        ),
        end_time=(
            datetime.datetime.now() + datetime.timedelta(days=1, hours=1)
        ).replace(hour=1, minute=0, second=0, microsecond=0),
    )
    meetings = [all_day, FAKE_MEETING]
    rets = ["all day", "normal"]
    result = cli.get_next_non_all_day_meeting(meetings, rets, 24)
    assert result == "normal"
    result2 = cli.get_next_meeting(meetings, skip_all_day=True)
    assert result2 == FAKE_MEETING
    result3 = cli.get_next_meeting(meetings, skip_all_day=False)
    assert result3 == all_day


# Test CLI output for waybar and non-waybar (mocking json.dump and print)
def test_cli_output_waybar(monkeypatch, tmp_path):
    # Patch fetcher to return a known meeting
    # pylint: disable=too-few-public-methods
    class DummyFetcher:
        def fetch_meetings(self, _):
            return [FAKE_MEETING]

    monkeypatch.setattr(cli, "MeetingFetcher", DummyFetcher)
    # Patch json.dump and sys.stdout
    out = {}

    def fake_json_dump(obj, _):
        out["obj"] = obj

    monkeypatch.setattr(cli.json, "dump", fake_json_dump)
    monkeypatch.setattr(
        cli,
        "parse_args",
        lambda: Namespace(
            waybar=True,
            waybar_show_all_day_meeting=True,
            calendar=None,
            gcalcli_cmdline=cli.GCALCLI_CMDLINE,
            cache_dir=tmp_path,
            max_title_length=50,
            notify_icon="/tmp/icon.svg",
            notify_expiry=0,
            skip_all_day_meeting=False,
            today_only=False,
            google_domain=None,
            notify_min_before_events=5,
            notify_min_color="#FF5733",
            notify_min_color_foreground="#F4F1DE",
            open_meet_url=False,
            debug=False,
        ),
    )
    cli.main()
    assert out["obj"]["text"]


def test_cli_output_non_waybar(monkeypatch, tmp_path, capsys):
    # pylint: disable=too-few-public-methods
    class DummyFetcher:
        def fetch_meetings(self, _):
            return [FAKE_MEETING]

    monkeypatch.setattr(cli, "MeetingFetcher", DummyFetcher)
    monkeypatch.setattr(
        cli,
        "parse_args",
        lambda: Namespace(
            waybar=False,
            waybar_show_all_day_meeting=False,
            calendar=None,
            gcalcli_cmdline=cli.GCALCLI_CMDLINE,
            cache_dir=tmp_path,
            max_title_length=50,
            notify_icon="/tmp/icon.svg",
            notify_expiry=0,
            skip_all_day_meeting=False,
            today_only=False,
            google_domain=None,
            notify_min_before_events=5,
            notify_min_color="#FF5733",
            notify_min_color_foreground="#F4F1DE",
            open_meet_url=False,
            debug=False,
        ),
    )
    cli.main()
    captured = capsys.readouterr()
    assert "to go" in captured.out or "In" in captured.out
