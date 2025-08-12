from __future__ import annotations

import html
from typing import Any, Dict, List, Optional
from datetime import datetime
from dateutil.parser import isoparse  # type: ignore


def _ellipsis(text: str, max_len: int) -> str:
    if max_len <= 0 or len(text) <= max_len:
        return text
    return text[: max_len - 1] + "…"


def _when_text(item: Optional[Dict[str, Any]], *, time_format: str = "24h") -> str:
    if not item:
        return ""
    now = datetime.now()
    try:
        s = isoparse(item.get("start"))
        e = isoparse(item.get("end"))
    except Exception:
        return ""
    if s <= now <= e:
        minutes = int((e - now).total_seconds() // 60)
        return f"{minutes} min left"
    if s.date() != now.date():
        if (s.date() - now.date()).days == 1:
            base = "Tomorrow"
        else:
            base = s.strftime("%a %d")
        if time_format == "12h":
            return f"{base} at {s.strftime('%I:%M %p')}"
        return f"{base} at {s.strftime('%H:%M')}"
    if time_format == "12h":
        return s.strftime("%I:%M %p")
        return s.strftime("%H:%M")


def _build_fields(item: Dict[str, Any], *, time_format: str = "24h") -> Dict[str, Any]:
    # Parse dates for templating compatibility
    try:
        start_dt = isoparse(item.get("start"))
        end_dt = isoparse(item.get("end"))
    except Exception:
        start_dt = end_dt = None  # type: ignore[assignment]
    now = datetime.now()
    minutes_until = None
    if start_dt is not None:
        minutes_until = int(max(0, (start_dt - now).total_seconds()) // 60)
    return {
        "when": _when_text(item, time_format=time_format),
        "title": item.get("title") or "(no title)",
        "start_time": start_dt,
        "end_time": end_dt,
        "meet_url": item.get("meet_url"),
        "calendar_url": item.get("calendar_url"),
        "minutes_until": minutes_until,
        "is_all_day": bool(item.get("is_all_day")),
        "is_ongoing": bool(item.get("is_ongoing")),
    }


def format_waybar(
    next_item: Optional[Dict[str, Any]],
    items: List[Dict[str, Any]],
    *,
    privacy: bool = False,
    privacy_title: str = "Busy",
    max_title_length: int = 50,
    tooltip_limit: Optional[int] = None,
    notify_min: int = 5,
    time_format: str = "24h",
    format_str: Optional[str] = None,
    tooltip_format: Optional[str] = None,
) -> Dict[str, Any]:
    def _title(d: Dict[str, Any]) -> str:
        t = privacy_title if privacy else (d.get("title") or "(no title)")
        return html.escape(_ellipsis(t, max_title_length))

    text = ""
    css_class = ""
    if next_item:
        if format_str:
            flds = _build_fields(next_item, time_format=time_format)
            raw = format_str.format(**flds)
            text = html.escape(_ellipsis(raw, max_title_length))
        else:
            when = _when_text(next_item, time_format=time_format)
            text = f"{when} - {_title(next_item)}" if when else _title(next_item)
        css_class = "current" if next_item.get("is_ongoing") else "upcoming"
        # soon threshold
        try:
            now = datetime.now()
            s = isoparse(next_item.get("start"))
            if s > now and int((s - now).total_seconds() // 60) <= notify_min:
                css_class = "soon"
        except Exception:
            pass
    tooltip_lines: List[str] = []
    count = 0
    for d in items:
        if tooltip_limit is not None and count >= tooltip_limit:
            break
        if tooltip_format:
            flds = _build_fields(d, time_format=time_format)
            line = tooltip_format.format(**flds)
            tooltip_lines.append(html.escape(line))
        else:
            tooltip_lines.append(f"• {_title(d)}")
        count += 1
    tooltip = "\n".join(tooltip_lines)
    return {"text": text, "tooltip": tooltip, "class": css_class}


def format_polybar(
    next_item: Optional[Dict[str, Any]],
    *,
    privacy: bool = False,
    privacy_title: str = "Busy",
    max_title_length: int = 50,
    notify_min: int = 5,
    notify_min_color: str = "#FF5733",
    notify_min_color_fg: str = "#F4F1DE",
    time_format: str = "24h",
    format_str: Optional[str] = None,
) -> str:
    def colorize(num: int) -> str:
        return f"%{{B{notify_min_color}}}%{{F{notify_min_color_fg}}}{num}%{{F-}}%{{B-}}"

    if not next_item:
        return ""
    if format_str:
        flds = _build_fields(next_item, time_format=time_format)
        return format_str.format(**flds)
    title = privacy_title if privacy else (next_item.get("title") or "(no title)")
    title = _ellipsis(title, max_title_length)
    now = datetime.now()
    try:
        s = isoparse(next_item.get("start"))
        e = isoparse(next_item.get("end"))
    except Exception:
        return title
    if s <= now <= e:
        minutes = int((e - now).total_seconds() // 60)
        when = f"{minutes} left"
    else:
        minutes = int((s - now).total_seconds() // 60)
        if minutes <= notify_min and minutes >= 0:
            when = f"in {colorize(minutes)} min"
        elif s.date() != now.date():
            if (s.date() - now.date()).days == 1:
                base = "Tomorrow"
            else:
                base = s.strftime("%a %d")
            when = f"{base} at {s.strftime('%I:%M %p' if time_format == '12h' else '%H:%M')}"
        else:
            when = s.strftime("%I:%M %p" if time_format == "12h" else "%H:%M")
    return f"{when} - {title}"
