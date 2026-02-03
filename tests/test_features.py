import datetime
from argparse import Namespace

from nextmeeting.cli import Meeting, MeetingFormatter, OutputFormatter, ellipsis


def _args(**overrides):
    base = dict(
        waybar=False,
        json=False,
        polybar=False,
        waybar_show_all_day_meeting=False,
        max_title_length=50,
        today_only=False,
        skip_all_day_meeting=False,
        include_title=[],
        exclude_title=[],
        work_hours=None,
        notify_min_before_events=5,
        notify_min_color="#FF5733",
        notify_min_color_foreground="#F4F1DE",
        google_domain=None,
        format=None,
        tooltip_format=None,
        limit=None,
        debug=False,
        cache_dir=".",
        notify_icon="",
        notify_expiry=0,
        notify_offsets=[],
        privacy=False,
        privacy_title="Busy",
    )
    base.update(overrides)
    return Namespace(**base)


def _meeting(title: str, start: datetime.datetime, end: datetime.datetime) -> Meeting:
    return Meeting(
        title=title,
        start_time=start,
        end_time=end,
        calendar_url="https://www.google.com/calendar/event?eid=abc",
        meet_url="https://meet.google.com/xyz",
    )


def test_title_filters_include_exclude():
    now = datetime.datetime.now()
    m1 = _meeting(
        "Daily standup",
        now + datetime.timedelta(minutes=30),
        now + datetime.timedelta(minutes=60),
    )
    m2 = _meeting(
        "Random chat",
        now + datetime.timedelta(minutes=30),
        now + datetime.timedelta(minutes=60),
    )
    args = _args(include_title=["standup"])
    out = OutputFormatter(args)
    formatted, _ = out.format_meetings([m1, m2])
    assert any("standup" in s.lower() for s in formatted)
    assert all("random chat" not in s.lower() for s in formatted)


def test_privacy_mode_replaces_title():
    now = datetime.datetime.now()
    m = _meeting(
        "Very Secret Meeting",
        now + datetime.timedelta(minutes=10),
        now + datetime.timedelta(minutes=40),
    )
    args = _args(privacy=True, privacy_title="Busy")
    fmt = MeetingFormatter(args)
    text, _ = fmt.format_meeting(m)
    assert "Busy" in text
    assert "Very Secret Meeting" not in text


def test_format_template_applies():
    now = datetime.datetime.now()
    m = _meeting(
        "Demo",
        now + datetime.timedelta(minutes=10),
        now + datetime.timedelta(minutes=40),
    )
    args = _args(format="{title} @ {start_time:%H:%M}")
    fmt = MeetingFormatter(args)
    text, _ = fmt.format_meeting(m)
    assert "Demo @" in text


def test_work_hours_filters_outside():
    today = datetime.datetime.now().replace(hour=8, minute=0, second=0, microsecond=0)
    m = _meeting("Early", today, today + datetime.timedelta(hours=1))
    args = _args(work_hours="09:00-18:00")
    out = OutputFormatter(args)
    formatted, _ = out.format_meetings([m])
    assert formatted == []


def test_waybar_tooltip_limit():
    now = datetime.datetime.now()
    meetings = [
        _meeting(
            f"m{i}",
            now + datetime.timedelta(minutes=10 + i),
            now + datetime.timedelta(minutes=40 + i),
        )
        for i in range(5)
    ]
    args = _args(waybar=True, limit=2)
    out = OutputFormatter(args)
    result = out.format_for_waybar(meetings)
    assert result["tooltip"].count("\n") == 1  # 2 bullets -> 1 newline


def test_ellipsis_no_truncation_needed():
    assert ellipsis("short", 10) == "short"


def test_ellipsis_basic_truncation():
    result = ellipsis("this is a long string", 10)
    assert result == "this is..."
    assert len(result) == 10


def test_ellipsis_html_entity_not_broken():
    # &amp; should count as 1 display char and not be split
    result = ellipsis("Perf&amp;Scale meeting", 15)
    assert "&amp;" in result or result.endswith("...")
    # Ensure no broken entity like &amp without semicolon
    assert "&amp" not in result or "&amp;" in result


def test_ellipsis_html_entity_counts_as_one_char():
    # "Test&amp;Go" has display length 9 (Test&Go + 2 chars for entity displayed as 1)
    # With length=12, should not truncate
    result = ellipsis("Test&amp;Go", 12)
    assert result == "Test&amp;Go"


def test_ellipsis_numeric_entity_not_broken():
    # &#x27; is apostrophe, should count as 1 display char
    result = ellipsis("What&#x27;s New in Tech", 15)
    # Should not have broken entity
    assert "&#x27" not in result or "&#x27;" in result


def test_ellipsis_strips_html_tags():
    result = ellipsis("<span>Hello</span> World", 15)
    assert "<span>" not in result
    assert "Hello World" in result or "Hello Wor..." in result
