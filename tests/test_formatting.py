import datetime as dt

from nextmeeting.formatting import format_waybar, format_polybar


def _item(title: str, start: dt.datetime, end: dt.datetime, *, meet: str | None = None):
    return {
        "title": title,
        "start": start.isoformat(),
        "end": end.isoformat(),
        "calendar_url": "https://cal",
        "meet_url": meet,
        "is_all_day": False,
        "is_ongoing": False,
    }


def test_waybar_basic_and_templates():
    now = dt.datetime.now().replace(second=0, microsecond=0)
    next_item = _item(
        "Standup", now + dt.timedelta(minutes=10), now + dt.timedelta(minutes=40)
    )
    items = [
        next_item,
        _item("Retro", now + dt.timedelta(minutes=60), now + dt.timedelta(minutes=90)),
    ]

    # Basic formatting
    out = format_waybar(
        next_item,
        items,
        privacy=False,
        tooltip_limit=2,
        notify_min=5,
        time_format="24h",
    )
    assert "Standup" in out["text"]
    assert "Retro" in out["tooltip"]

    # soon class when within notify_min
    soon_item = _item(
        "Soon", now + dt.timedelta(minutes=3), now + dt.timedelta(minutes=33)
    )
    out2 = format_waybar(soon_item, items, privacy=False, tooltip_limit=1, notify_min=5)
    assert out2["class"] == "soon"

    # Template formatting
    out3 = format_waybar(
        next_item,
        items,
        privacy=False,
        tooltip_limit=1,
        time_format="24h",
        format_str="{when} • {title}",
        tooltip_format="{start_time:%H:%M}-{end_time:%H:%M} · {title}",
    )
    assert "• Standup" in out3["text"]
    assert ":" in out3["tooltip"]  # time in tooltip


def test_polybar_basic_and_template():
    now = dt.datetime.now().replace(second=0, microsecond=0)
    next_item = _item(
        "Demo", now + dt.timedelta(minutes=15), now + dt.timedelta(minutes=45)
    )

    txt = format_polybar(next_item, privacy=False, notify_min=5, time_format="24h")
    assert "Demo" in txt

    # Template override should return exactly rendered template
    txt2 = format_polybar(next_item, format_str="{title}")
    assert txt2 == "Demo"
