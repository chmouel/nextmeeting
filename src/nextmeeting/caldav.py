"""CalDAV integration helpers for nextmeeting."""

from __future__ import annotations

import datetime
import re
from typing import Callable, Optional
import sys
import copy

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

        # Resolve multiple calendars
        calendars_with_names = self._resolve_calendars(client, args)

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

        # Fetch from all calendars
        all_meetings = []
        fetch_errors = []

        for calendar, display_name in calendars_with_names:
            try:
                raw_events = calendar.date_search(start, end)  # type: ignore[union-attr]
            except caldav_error.DAVError as exc:  # noqa: BLE001
                # Try retry logic for this calendar
                try:
                    raw_events = self._retry_date_search_for_calendar(
                        client, calendar, display_name, args, (start, end), exc
                    )
                except Exception as retry_exc:  # noqa: BLE001
                    fetch_errors.append(f"{display_name}: {retry_exc}")
                    continue
            except Exception as exc:  # noqa: BLE001
                fetch_errors.append(f"{display_name}: {exc}")
                continue

            # Parse events from this calendar
            for event in raw_events:
                try:
                    event_meetings = self._meetings_from_event(event, args)
                    # Tag each meeting with calendar display name
                    for meeting in event_meetings:
                        if hasattr(meeting, "calendar_name"):
                            meeting.calendar_name = display_name  # type: ignore[attr-defined]
                    all_meetings.extend(event_meetings)
                except Exception:  # noqa: BLE001
                    continue

        # Warn about fetch errors (if any calendars failed)
        if fetch_errors:
            print(
                f"Warning: Failed to fetch from {len(fetch_errors)} calendar(s)",
                file=sys.stderr,
            )
            if getattr(args, "verbose", False) or getattr(args, "debug", False):
                for error in fetch_errors:
                    print(f"  - {error}", file=sys.stderr)

        # If all calendars failed, raise error
        if not all_meetings and fetch_errors:
            raise RuntimeError(
                f"Failed to fetch from all {len(calendars_with_names)} calendars:\n"
                + "\n".join(f"  - {e}" for e in fetch_errors)
            )

        now = datetime.datetime.now()
        return sorted(
            [meeting for meeting in all_meetings if meeting.end_time > now],  # type: ignore[attr-defined]
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

    def _get_calendar_display_name(self, calendar) -> str:
        """Extract human-readable display name from calendar object."""
        # Try displayname property first (DAV:displayname)
        try:
            props = calendar.get_properties(["{DAV:}displayname"])  # type: ignore[arg-type]
            display_name = props.get("{DAV:}displayname")
            if display_name:
                return str(display_name)
        except Exception:  # noqa: BLE001
            pass

        # Fall back to name attribute
        name = getattr(calendar, "name", None)
        if name:
            return str(name)

        # Last resort: use URL path component
        url_obj = getattr(calendar, "url", None)
        if url_obj:
            url_str = str(
                url_obj.to_python() if hasattr(url_obj, "to_python") else url_obj
            )
            # Extract last path component
            path_parts = [p for p in url_str.split("/") if p]
            if path_parts:
                return path_parts[-1]

        return "Unknown Calendar"

    def _get_calendar_url(self, calendar) -> str:
        """Extract URL from calendar object for deduplication."""
        url_obj = getattr(calendar, "url", None)
        if url_obj:
            if hasattr(url_obj, "to_python"):
                return str(url_obj.to_python())
            return str(url_obj)
        return ""

    def _resolve_calendars(self, client: "DAVClient", args) -> list[tuple[object, str]]:
        """
        Resolve multiple calendars based on hints.

        Returns list of tuples: (calendar_object, display_name)
        where display_name is used to identify which calendar each meeting came from.
        """
        calendar_hints = getattr(args, "caldav_calendar", [])

        # Normalize to list (backward compatibility)
        if not isinstance(calendar_hints, list):
            calendar_hints = [calendar_hints] if calendar_hints else []

        # If no hints provided, return ALL available calendars
        # Note: This differs from previous behaviour which returned only the first calendar
        if not calendar_hints:
            try:
                principal = client.principal()
                all_calendars = principal.calendars()
                if not all_calendars:
                    raise RuntimeError("No calendars are available for this account")

                resolved = []
                for calendar in all_calendars:
                    display_name = self._get_calendar_display_name(calendar)
                    resolved.append((calendar, display_name))

                return resolved
            except Exception as exc:
                # Re-raise - if we can't get calendars, we should fail
                raise exc

        # Resolve each hint to a calendar
        resolved = []
        errors = []

        for hint in calendar_hints:
            try:
                # Create temporary args object with single hint
                temp_args = copy.copy(args)
                temp_args.caldav_calendar = hint

                calendar = self._resolve_calendar(client, temp_args)
                display_name = self._get_calendar_display_name(calendar)

                # Check for duplicates (same calendar resolved multiple times)
                calendar_urls = [self._get_calendar_url(cal) for cal, _ in resolved]
                current_url = self._get_calendar_url(calendar)
                if current_url not in calendar_urls:
                    resolved.append((calendar, display_name))

            except Exception as exc:  # noqa: BLE001
                errors.append(f"Calendar '{hint}': {exc}")
                continue

        # If no calendars resolved successfully, raise error with details
        if not resolved:
            if errors:
                error_msg = "Failed to resolve any calendars:\n" + "\n".join(
                    f"  - {e}" for e in errors
                )
            else:
                error_msg = "No calendar hints matched any available calendars"
            raise RuntimeError(error_msg)

        # Warn about partial failures (if verbose/debug enabled)
        if errors and (
            getattr(args, "verbose", False) or getattr(args, "debug", False)
        ):
            for error in errors:
                print(f"[WARNING] {error}", file=sys.stderr)

        return resolved

    def _retry_date_search_for_calendar(
        self,
        client: "DAVClient",
        calendar,
        display_name: str,
        args,
        window: tuple[datetime.datetime, datetime.datetime],
        original_exc: Exception,
    ) -> list[object]:
        """
        Retry date search for a specific calendar with sophisticated fallback logic.

        Attempts multiple URL variations and provides detailed diagnostic information
        to help users debug connection issues.
        """
        if not caldav_error:
            raise RuntimeError(
                f"Failed to query calendar '{display_name}': {original_exc}"
            ) from original_exc

        # Try to get the calendar URL for variation attempts
        calendar_url = self._get_calendar_url(calendar)
        if not calendar_url:
            # No URL available, just try once more with the calendar object
            try:
                return calendar.date_search(*window)
            except Exception:  # noqa: BLE001
                raise RuntimeError(
                    f"Failed to query calendar '{display_name}': {original_exc}"
                ) from original_exc

        # Build candidate URLs to try
        candidates: list[tuple[str, str]] = [("original", calendar_url)]

        # Try URL with/without trailing slash
        if calendar_url.endswith("/"):
            candidates.append(("trimmed-slash", calendar_url.rstrip("/")))
        else:
            candidates.append(("with-slash", f"{calendar_url}/"))

        # Try calendar hint as direct URL if it looks like a URL
        calendar_hints = getattr(args, "caldav_calendar", [])
        if not isinstance(calendar_hints, list):
            calendar_hints = [calendar_hints] if calendar_hints else []

        for hint in calendar_hints:
            hint_str = str(hint)
            if hint_str and re.match(r"https?://", hint_str):
                if all(hint_str != candidate for _, candidate in candidates):
                    candidates.append(("calendar-hint", hint_str))

        # Try each URL variation
        errors: list[str] = []
        for label, candidate_url in candidates:
            if not candidate_url:
                continue
            try:
                alt_calendar = client.calendar(url=candidate_url)
                return alt_calendar.date_search(*window)  # type: ignore[union-attr]
            except Exception as exc:  # noqa: BLE001
                errors.append(f"{label}: {candidate_url} -> {exc}")

        # All variations failed - gather available calendars for diagnostic output
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

        # Build detailed error message
        message_lines = [
            f"Failed to query calendar '{display_name}'; no working locations responded.",
            "Tried the following URLs:",
        ]

        if errors:
            # Show all errors in verbose mode, otherwise just the first
            if getattr(args, "verbose", False) or getattr(args, "debug", False):
                entries = errors
            else:
                entries = errors[:1]
            message_lines.extend(f"  - {entry}" for entry in entries)
        else:
            message_lines.append("  - (no alternate URLs attempted)")

        # Show available calendars to help with debugging
        if available:
            message_lines.append("Server advertised calendars:")
            message_lines.extend(f"  * {url}" for url in available)
        else:
            message_lines.append(
                "Consider pointing --caldav-url or --caldav-calendar to the full collection path, "
                "for example http(s)://HOST/dav/calendars/USER/CALENDAR/."
            )

        raise RuntimeError("\n".join(message_lines)) from original_exc

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
