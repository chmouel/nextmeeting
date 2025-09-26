"""CalDAV integration helpers for nextmeeting."""

from __future__ import annotations

import datetime
import re
from typing import Callable, Optional

try:
    from caldav import DAVClient  # type: ignore[import-not-found]
    from caldav.lib import error as caldav_error  # type: ignore[import-not-found]
except ImportError:  # noqa: BLE001
    DAVClient = None  # type: ignore[assignment]
    caldav_error = None  # type: ignore[assignment]

try:
    from icalendar import Calendar  # type: ignore[import-not-found]
except ImportError:  # noqa: BLE001
    Calendar = None  # type: ignore[assignment]

CALDAV_DEFAULT_LOOKAHEAD_HOURS = 48
CALDAV_DEFAULT_LOOKBEHIND_HOURS = 12

URL_RE = re.compile(r"https?://\S+")


class CalDavMeetingFetcher:
    """Fetch meetings from a CalDAV-compatible calendar."""

    def __init__(self, meeting_factory: Callable[..., object]):
        self.meeting_factory = meeting_factory
        if DAVClient is None or Calendar is None:
            raise RuntimeError(
                "CalDAV support requires the 'caldav' and 'icalendar' packages. Install via 'uv add caldav'."
            )

    def fetch_meetings(self, args) -> list[object]:
        if not args.caldav_url:
            raise RuntimeError("--caldav-url is required when using the CalDAV flags")

        client_kwargs = {
            "url": args.caldav_url,
            "username": getattr(args, "caldav_username", None),
            "password": getattr(args, "caldav_password", None),
        }
        if getattr(args, "caldav_disable_tls_verify", False):
            client_kwargs["ssl_verify_cert"] = False

        client = DAVClient(
            **{key: value for key, value in client_kwargs.items() if value is not None}
        )
        calendar = self._resolve_calendar(client, args)

        lookbehind = getattr(
            args, "caldav_lookbehind_hours", CALDAV_DEFAULT_LOOKBEHIND_HOURS
        )
        lookahead = getattr(
            args, "caldav_lookahead_hours", CALDAV_DEFAULT_LOOKAHEAD_HOURS
        )

        start = datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(
            hours=lookbehind
        )
        end = start + datetime.timedelta(hours=lookbehind + lookahead)

        try:
            raw_events = calendar.date_search(start, end)  # type: ignore[union-attr]
        except caldav_error.DAVError as exc:  # noqa: BLE001
            raw_events = self._retry_date_search(client, args, (start, end), exc)

        meetings: list[object] = []
        for event in raw_events:
            try:
                meetings.extend(self._meetings_from_event(event, args))
            except Exception:  # noqa: BLE001
                continue

        now = datetime.datetime.now()
        return sorted(
            [meeting for meeting in meetings if meeting.end_time > now],  # type: ignore[attr-defined]
            key=lambda meeting: meeting.start_time,  # type: ignore[attr-defined]
        )

    def _resolve_calendar(self, client: "DAVClient", args):
        calendar_hint = getattr(args, "caldav_calendar", None)
        direct_url = getattr(args, "caldav_url", None)

        if direct_url:
            try:
                return client.calendar(url=direct_url)
            except Exception as exc:  # noqa: BLE001
                raise RuntimeError(
                    f"Failed to reach calendar URL '{direct_url}': {exc}"
                ) from exc

        if calendar_hint and re.match(r"https?://", str(calendar_hint)):
            return client.calendar(url=calendar_hint)

        try:
            principal = client.principal()
        except Exception as exc:  # noqa: BLE001
            raise RuntimeError(
                "Unable to auto-detect calendars; provide --caldav-calendar with a full collection URL."
            ) from exc
        calendars = principal.calendars()
        if not calendars:
            raise RuntimeError("No calendars are available for this account")

        if not calendar_hint:
            return calendars[0]

        for cal in calendars:
            identifiers = {
                getattr(cal, "name", None),
                getattr(cal, "id", None),
                getattr(getattr(cal, "url", None), "to_python", lambda: None)(),
            }
            identifiers = {str(item) for item in identifiers if item}
            if any(str(calendar_hint) in identifier for identifier in identifiers):
                return cal

            try:
                props = cal.get_properties(["{DAV:}displayname"])  # type: ignore[arg-type]
                display_name = props.get("{DAV:}displayname")
                if display_name and str(calendar_hint) in str(display_name):
                    return cal
            except Exception:  # noqa: BLE001
                continue

        raise RuntimeError(f"Unable to find a calendar matching '{calendar_hint}'")

    def _meetings_from_event(self, event: object, args) -> list[object]:
        data = getattr(event, "data", None)
        if not data:
            return []
        if isinstance(data, bytes):
            raw = data
        else:
            raw = str(data).encode("utf-8")

        calendar = Calendar.from_ical(raw)
        meetings: list[object] = []
        for component in calendar.walk():
            if component.name != "VEVENT":
                continue
            meeting = self._meeting_from_component(component, args)
            if meeting:
                meetings.append(meeting)
        return meetings

    def _meeting_from_component(self, component, args):
        summary = str(component.get("summary", ""))

        start_raw = component.get("dtstart")
        end_raw = component.get("dtend")
        duration_raw = component.get("duration")

        start_dt = _as_local_datetime(
            start_raw.dt if hasattr(start_raw, "dt") else start_raw
        )
        if end_raw:
            end_dt = _as_local_datetime(
                end_raw.dt if hasattr(end_raw, "dt") else end_raw
            )
        else:
            duration = (
                duration_raw.dt
                if duration_raw and hasattr(duration_raw, "dt")
                else duration_raw
            )
            if isinstance(duration, datetime.timedelta):
                end_dt = start_dt + duration
            elif duration:
                end_dt = start_dt + datetime.timedelta(seconds=int(duration))
            else:
                end_dt = start_dt + datetime.timedelta(hours=1)

        url = component.get("url")
        calendar_url = str(url) if url else getattr(args, "caldav_url", "")
        meet_url = self._extract_meeting_url(component)

        return self.meeting_factory(
            title=summary or "(No title)",
            start_time=start_dt,
            end_time=end_dt,
            calendar_url=calendar_url,
            meet_url=meet_url,
        )

    def _extract_meeting_url(self, component) -> Optional[str]:
        if component.get("url"):
            return str(component.get("url"))
        for key in ("description", "location"):
            content = component.get(key)
            if not content:
                continue
            if hasattr(content, "to_ical"):
                text = content.to_ical().decode()
            else:
                text = str(content)
            if match := URL_RE.search(text):
                return match.group(0)
        return None

    def _retry_date_search(
        self,
        client: "DAVClient",
        args,
        window: tuple[datetime.datetime, datetime.datetime],
        original_exc: Exception,
    ) -> list[object]:
        if not caldav_error or not getattr(args, "caldav_url", None):
            raise RuntimeError(
                f"Failed to query calendar endpoint: {original_exc}"
            ) from original_exc

        original_url = str(args.caldav_url)
        candidates: list[tuple[str, str]] = [("original", original_url)]
        if original_url.endswith("/"):
            candidates.append(("trimmed-slash", original_url.rstrip("/")))
        else:
            candidates.append(("with-slash", f"{original_url}/"))

        if getattr(args, "caldav_calendar", None):
            hint = str(args.caldav_calendar)
            if hint and all(hint != candidate for _, candidate in candidates):
                candidates.append(("calendar-hint", hint))

        errors: list[str] = []
        for label, candidate in candidates:
            if not candidate:
                continue
            try:
                alt_calendar = client.calendar(url=candidate)
                return alt_calendar.date_search(*window)  # type: ignore[union-attr]
            except Exception as exc:  # noqa: BLE001
                errors.append(f"{label}: {candidate} -> {exc}")

        available: list[str] = []
        try:
            principal = client.principal()
            for cal in principal.calendars():
                url_obj = getattr(cal, "url", None)
                if url_obj and hasattr(url_obj, "to_python"):
                    available.append(str(url_obj.to_python()))
                elif isinstance(url_obj, str):
                    available.append(url_obj)
        except Exception:  # noqa: BLE001
            available = []

        message_lines = [
            "Failed to query calendar endpoint; no working locations responded.",
            "Tried the following URLs:",
        ]
        if errors:
            if getattr(args, "verbose", False) or getattr(args, "debug", False):
                entries = errors
            else:
                entries = errors[:1]
            message_lines.extend(f"  - {entry}" for entry in entries)
        else:
            message_lines.append("  - (no alternate URLs attempted)")
        if available:
            message_lines.append("Server advertised calendars:")
            message_lines.extend(f"  * {url}" for url in available)
        else:
            message_lines.append(
                "Consider pointing --caldav-url or --caldav-calendar to the full collection path, "
                "for example http(s)://HOST/dav/calendars/USER/CALENDAR/."
            )

        raise RuntimeError("\n".join(message_lines)) from original_exc


def _as_local_datetime(value: Optional[object]) -> datetime.datetime:
    if value is None:
        raise ValueError("Missing datetime value in calendar event")
    if isinstance(value, datetime.datetime):
        if value.tzinfo:
            return value.astimezone().replace(tzinfo=None)
        return value
    if isinstance(value, datetime.date):
        return datetime.datetime.combine(value, datetime.time.min)
    raise TypeError(f"Unsupported datetime type: {type(value)!r}")
