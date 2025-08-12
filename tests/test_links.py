import datetime
from argparse import Namespace

from nextmeeting.cli import Meeting, OutputFormatter, sanitize_meeting_link


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
        include_calendar=[],
        exclude_calendar=[],
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


def test_outlook_safelink_cleanup_and_zoom_normalization():
    # Outlook SafeLink wrapping a zoom join link
    wrapped = (
        "https://nam12.safelinks.protection.outlook.com/ap/t-xyz/?url="
        + "https%3A%2F%2Fzoom.us%2Fjoin%3Fconfno%3D123456789%26pwd%3DabcDEF"
        + "&data=ignored"
    )
    s = sanitize_meeting_link(wrapped)
    assert s is not None
    assert s.service == "zoom"
    assert s.meeting_id == "123456789"
    assert s.passcode == "abcDEF"
    assert s.url.startswith("https://zoom.us/j/123456789")


def test_google_meet_normalization_and_filters():
    now = datetime.datetime.now()
    m1 = Meeting(
        title="Meet",
        start_time=now + datetime.timedelta(minutes=40),
        end_time=now + datetime.timedelta(minutes=70),
        calendar_url="https://calendar",
        meet_url="https://meet.google.com/abc-defg-hij?hs=123",
    )
    m2 = Meeting(
        title="Zoom",
        start_time=now + datetime.timedelta(minutes=10),
        end_time=now + datetime.timedelta(minutes=40),
        calendar_url="https://calendar",
        meet_url="https://zoom.us/j/123?pwd=xxx",
    )

    # Only-within-window 30 mins should include m2 and drop m1
    out = OutputFormatter(_args(within_mins=30, only_with_link=False))
    formatted, _ = out.format_meetings([m1, m2])
    assert any("Zoom" in line for line in formatted)
    assert all("Meet" not in line for line in formatted)

    # Only-with-link should keep both since both have links
    out2 = OutputFormatter(_args(within_mins=None, only_with_link=True))
    formatted2, _ = out2.format_meetings([m1, m2])
    expected = 2
    assert len(formatted2) == expected
