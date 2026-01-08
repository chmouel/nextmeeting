#!/usr/bin/env python3
# Author: Chmouel Boudjnah <chmouel@chmouel.com>
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may
# not use this file except in compliance with the License. You may obtain
# a copy of the License at
#
#      http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations
# under the License.
# pylint: disable=too-many-lines disable=too-many-locals disable=too-many-statements disable=line-too-long

import argparse
import datetime
import hashlib
import html
import json
import os
import pathlib
import re
import shlex
import shutil
import subprocess
import sys
import traceback
import urllib.parse as urlparse
import webbrowser
from dataclasses import dataclass
from datetime import timedelta
from pathlib import Path
from typing import Optional, Sequence

import dateutil.parser as dtparse
import dateutil.relativedelta as dtrel

from .caldav import (
    CALDAV_DEFAULT_LOOKAHEAD_HOURS,
    CALDAV_DEFAULT_LOOKBEHIND_HOURS,
    CalDavMeetingFetcher,
)

try:  # Python 3.11+
    import tomllib  # type: ignore[import-not-found]
except Exception:  # noqa: BLE001
    tomllib = None  # type: ignore[assignment]

# Constants
REG_TSV = re.compile(
    r"(?P<startdate>(\d{4})-(\d{2})-(\d{2}))\s*?"
    r"(?P<starthour>(\d{2}:\d{2}))\s*"
    r"(?P<enddate>(\d{4})-(\d{2})-(\d{2}))\s*?"
    r"(?P<endhour>(\d{2}:\d{2}))\s*"
    r"(?P<calendar_url>(https://\S+))\s*"
    r"(?P<meet_url>(https://\S*)?)?\s*"
    r"(?P<title>.*)$"
)

DEFAULT_CALENDAR = os.environ.get("GCALCLI_DEFAULT_CALENDAR", "")
GCALCLI_CMDLINE = (
    "gcalcli --nocolor agenda today --nodeclined --details=end --details=url --tsv"
)
TITLE_ELIPSIS_LENGTH = 50
MAX_CACHED_ENTRIES = 30
NOTIFY_MIN_BEFORE_EVENTS = 5
NOTIFY_MIN_COLOR = "#FF5733"
NOTIFY_MIN_COLOR_FOREGROUND = "#F4F1DE"
CACHE_DIR = pathlib.Path(os.path.expanduser("~/.cache/nextmeeting"))
NOTIFY_PROGRAM = shutil.which("notify-send") or ""
NOTIFY_ICON = "/usr/share/icons/hicolor/scalable/apps/org.gnome.Calendar.svg"
GOOGLE_CALENDAR_PUBLIC_URL = "www.google.com/calendar"
ALL_DAYS_MEETING_HOURS = 24
AGENDA_MINUTE_TOLERANCE = 2
HOUR_SEPARATOR = "H"
UNTIL_OFFSET = 60  # minutes before event to start showing "until" info
NO_MEETING_TEXT = "No meeting"
NO_MEETING_ICON = "🏖️"


@dataclass
class Meeting:
    title: str
    start_time: datetime.datetime
    end_time: datetime.datetime
    calendar_url: str
    meet_url: Optional[str] = None
    calendar_name: Optional[str] = None

    @property
    def is_all_day(self) -> bool:
        return (
            self.end_time - self.start_time
        ).total_seconds() / 3600 >= ALL_DAYS_MEETING_HOURS

    @property
    def is_ongoing(self) -> bool:
        now = datetime.datetime.now()
        return self.start_time <= now <= self.end_time

    @property
    def time_until_start(self) -> timedelta:
        return self.start_time - datetime.datetime.now()

    @property
    def time_until_end(self) -> timedelta:
        return self.end_time - datetime.datetime.now()

    @classmethod
    def from_match(cls, match: re.Match) -> "Meeting":
        start_time = dtparse.parse(f"{match['startdate']} {match['starthour']}")
        end_time = dtparse.parse(f"{match['enddate']} {match['endhour']}")
        return cls(
            title=match["title"],
            start_time=start_time,
            end_time=end_time,
            calendar_url=match["calendar_url"],
            meet_url=match["meet_url"] if match["meet_url"] else None,
        )


# pylint: disable=too-few-public-methods
class MeetingFormatter:
    def __init__(self, args: argparse.Namespace):
        self.args = args
        self.today = datetime.datetime.now()

    def format_meeting(
        self,
        meeting: Meeting,
        hyperlink: bool = False,  # pylint: disable=line-too-long
    ) -> tuple[str, str]:  # pylint: disable=line-too-long
        """Format a single meeting and return (formatted_string, css_class)."""
        fields, css = self._compute_fields(meeting, hyperlink)
        # Template support (plain text fields)
        template = getattr(self.args, "format", None)
        if template:
            formatted = template.format(
                when=fields["when"],
                title=fields["title"],
                start_time=fields["start_time"],
                end_time=fields["end_time"],
                meet_url=fields.get("meet_url"),
                calendar_url=fields.get("calendar_url"),
                calendar_name=fields.get("calendar_name"),
                minutes_until=fields.get("minutes_until"),
                is_all_day=fields.get("is_all_day"),
                is_ongoing=fields.get("is_ongoing"),
            )
        else:
            formatted = f"{fields['when']} - {fields['title']}"
        return formatted, css

    def _format_title(self, meeting: Meeting, hyperlink: bool) -> str:
        title = meeting.title
        if getattr(self.args, "privacy", False):
            title = getattr(self.args, "privacy_title", "Busy")
        if self.args.waybar:
            title = html.escape(title)
        if hyperlink and meeting.meet_url:
            title = make_hyperlink(meeting.meet_url, title)
        return title

    def _format_ongoing_meeting(
        self, meeting: Meeting, title: str
    ) -> tuple[str, str, str]:
        timetofinish = dtrel.relativedelta(meeting.end_time, self.today)
        if timetofinish.hours == 0:
            time_str = f"{timetofinish.minutes} minutes"
        else:
            time_str = f"{timetofinish.hours}H{timetofinish.minutes}"

        thetime = f"{time_str} to go"
        return thetime, title, "current"

    def _format_upcoming_meeting(
        self, meeting: Meeting, title: str
    ) -> tuple[str, str, str]:
        timeuntilstarting = dtrel.relativedelta(meeting.start_time, self.today)
        css_class = ""

        # Check if notification is needed
        if (
            not timeuntilstarting.days
            and not timeuntilstarting.hours
            and 0 <= timeuntilstarting.minutes <= self.args.notify_min_before_events
        ):
            css_class = "soon"
            notify(title, meeting.start_time, meeting.end_time, self.args)

        thetime = self._format_time_until(timeuntilstarting, meeting.start_time)
        # Additional notification offsets
        offsets: list[int] = []
        if getattr(self.args, "notify_offsets", None):
            # Support comma-separated values and repeated flags
            raw: list[str] = []
            for item in self.args.notify_offsets:
                raw.extend(str(item).split(","))
            try:
                offsets = [int(x) for x in raw if str(x).strip()]
            except ValueError:
                offsets = []
        if (
            not timeuntilstarting.days
            and not timeuntilstarting.hours
            and timeuntilstarting.minutes in offsets
            and timeuntilstarting.minutes >= 0
        ):
            notify(title, meeting.start_time, meeting.end_time, self.args)
        return thetime, title, css_class

    def _compute_fields(self, meeting: Meeting, hyperlink: bool) -> tuple[dict, str]:
        title = self._format_title(meeting, hyperlink)
        if meeting.is_ongoing:
            when_text, title_text, css = self._format_ongoing_meeting(meeting, title)
        else:
            when_text, title_text, css = self._format_upcoming_meeting(meeting, title)

        fields: dict = {
            "when": when_text,
            "title": title_text,
            "start_time": meeting.start_time,
            "end_time": meeting.end_time,
            "calendar_url": meeting.calendar_url,
            "calendar_name": getattr(meeting, "calendar_name", None),
            "meet_url": meeting.meet_url,
            "is_all_day": meeting.is_all_day,
            "is_ongoing": meeting.is_ongoing,
            "minutes_until": int(
                max(0, (meeting.start_time - datetime.datetime.now()).total_seconds())
                // 60
            ),
        }
        return fields, css

    def _format_time_until(
        self, deltad: dtrel.relativedelta, date: datetime.datetime
    ) -> str:
        total_minutes = deltad.days * 24 * 60 + deltad.hours * 60 + deltad.minutes

        time_format_pref = getattr(self.args, "time_format", "24h")

        if date.day != self.today.day:
            if deltad.days == 0:
                s = "Tomorrow"
            else:
                s = f"{date.strftime('%a %d')}"
            if time_format_pref == "12h":
                separator = getattr(self.args, "hour_separator", ":")
                time_fmt = f"%I{separator}%M %p"
                s += f" at {date.strftime(time_fmt)}"
            else:
                separator = getattr(self.args, "hour_separator", "h")
                s += f" at {date.hour:02d}{separator}{date.minute:02d}"
        # elif deltad.hours != 0:
        elif total_minutes > getattr(self.args, "until_offset", UNTIL_OFFSET):
            if time_format_pref == "12h":
                separator = getattr(self.args, "hour_separator", ":")
                time_fmt = f"%I{separator}%M %p"
                s = date.strftime(time_fmt)
            else:
                separator = getattr(self.args, "hour_separator", "H")
                time_fmt = f"%H{separator}%M"
                s = date.strftime(time_fmt)
        elif deltad.days < 0 or deltad.hours < 0 or deltad.minutes < 0:
            s = "Now"
        elif (
            deltad.minutes <= NOTIFY_MIN_BEFORE_EVENTS
            and self.args.notify_min_color
            and self.args.waybar
        ):
            number = (
                f'<span background="{self.args.notify_min_color}" '
                f'color="{self.args.notify_min_color_foreground}">'
                f"{deltad.minutes}</span>"
            )
            s = f"In {number} minutes"
        else:
            parts = []
            if deltad.days:
                parts.append(f"{deltad.days} day{'s' if deltad.days != 1 else ''}")
            if deltad.hours:
                parts.append(f"{deltad.hours} hour{'s' if deltad.hours != 1 else ''}")
            if deltad.minutes or not parts:
                parts.append(
                    f"{deltad.minutes} minute{'s' if deltad.minutes != 1 else ''}"
                )

            if len(parts) == 1:
                s = f"In {parts[0]}"
            else:
                s = f"In {', '.join(parts[:-1])} and {parts[-1]}"
        return s


class MeetingFetcher:
    def __init__(self, gcalcli_cmdline: str = GCALCLI_CMDLINE):
        self.gcalcli_cmdline = gcalcli_cmdline

    def fetch_meetings(self, args: argparse.Namespace) -> list[Meeting]:
        cmdline = getattr(args, "gcalcli_cmdline", self.gcalcli_cmdline)
        debug(f"Executing gcalcli command: {cmdline}", args)
        cache_file = Path(args.cache_dir) / "events.tsv"
        ttl_min = getattr(args, "cache_events_ttl", 0) or 0
        now = datetime.datetime.now().timestamp()

        # Use cache if valid
        if ttl_min > 0 and cache_file.exists():
            try:
                mtime = cache_file.stat().st_mtime
                if now - mtime <= ttl_min * 60:
                    with cache_file.open() as f:
                        return self._process_lines(f.read().splitlines())
            except Exception:  # noqa: BLE001
                pass

        try:
            result = subprocess.run(
                cmdline,
                shell=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=True,
            )
            # Write cache if enabled
            if ttl_min > 0:
                try:
                    with cache_file.open("w") as f:
                        f.write(result.stdout)
                except Exception:  # noqa: BLE001
                    pass
            return self._process_lines(result.stdout.splitlines())
        except subprocess.CalledProcessError as e:
            self._handle_gcalcli_error(e, cmdline, args)
            return []

    def _handle_gcalcli_error(
        self,
        error: subprocess.CalledProcessError,
        cmdline: str,
        args: argparse.Namespace,
    ):
        try:
            calendar_list_result = subprocess.run(
                "gcalcli list",
                shell=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
            calendar_list = calendar_list_result.stdout.strip()
        except Exception:
            calendar_list = "Unable to retrieve calendar list"

        debug(error.stderr, args)
        raise RuntimeError(
            f"gcalcli command failed with exit code {error.returncode}, command: {cmdline}\n"
            f"Calendar available:\n{calendar_list}\n"
            f"Try nextmeeting --calendar=CALENDAR option to target the right calendar.\n"
            f"Use --debug to see the full error message."
        )

    def _process_lines(self, lines: Sequence[str]) -> list[Meeting]:
        meetings = []
        now = datetime.datetime.now()

        for line in lines:
            _line = line.strip()
            if match := REG_TSV.match(_line):
                meeting = Meeting.from_match(match)
                if meeting.end_time > now:
                    meetings.append(meeting)

        return meetings


@dataclass
class SanitizedLink:
    service: Optional[str]
    url: str
    meeting_id: Optional[str] = None
    passcode: Optional[str] = None


OUTLOOK_SAFELINK_RE = re.compile(
    r"https://[^\s]+\.safelinks\.protection\.outlook\.com/[^\s]+url=([^\s&]+)",
    re.IGNORECASE,
)


def _cleanup_outlook_safelinks(text: str) -> str:
    match = OUTLOOK_SAFELINK_RE.search(text)
    if not match:
        return text
    try:
        encoded = match.group(1)
        decoded = urlparse.unquote(encoded)
        return decoded
    except Exception:  # noqa: BLE001
        return text


def _normalize_zoom(u: str) -> SanitizedLink:
    parsed = urlparse.urlsplit(u)
    query = dict(urlparse.parse_qsl(parsed.query))
    host = parsed.netloc.lower()
    is_zoomgov = host.endswith("zoomgov.com")
    service = "zoomgov" if is_zoomgov else "zoom"

    meeting_id = None
    passcode = query.get("pwd") or query.get("passcode")

    # Convert /join?confno= to /j/<id>
    path = parsed.path
    if "/join" in path and "confno" in query:
        meeting_id = query.get("confno")
        path = f"/j/{meeting_id}"
        query.pop("confno", None)
    # Extract /j/<id>
    parts = [p for p in path.split("/") if p]
    if parts and parts[0] in {"j", "wc", "w"}:
        meeting_id = next(iter(parts[1:]), None)

    new_query = urlparse.urlencode(
        {k: v for k, v in query.items() if k in {"pwd", "passcode"}}
    )
    normalized = urlparse.urlunsplit(
        (parsed.scheme, parsed.netloc, path, new_query, "")
    )
    return SanitizedLink(
        service=service, url=normalized, meeting_id=meeting_id, passcode=passcode
    )


def _normalize_meet(u: str) -> SanitizedLink:
    # https://meet.google.com/abc-defg-hij
    parsed = urlparse.urlsplit(u)
    service = "google_meet"
    parts = [p for p in parsed.path.split("/") if p]
    meeting_id = parts[0] if parts else None
    normalized = urlparse.urlunsplit(
        (parsed.scheme, parsed.netloc, parsed.path, "", "")
    )
    return SanitizedLink(service=service, url=normalized, meeting_id=meeting_id)


def _normalize_teams(u: str) -> SanitizedLink:
    # Keep as-is; teams links are long and signed
    return SanitizedLink(service="teams", url=u)


def _normalize_jitsi(u: str) -> SanitizedLink:
    parsed = urlparse.urlsplit(u)
    parts = [p for p in parsed.path.split("/") if p]
    meeting_id = parts[0] if parts else None
    normalized = urlparse.urlunsplit(
        (parsed.scheme, parsed.netloc, parsed.path, "", "")
    )
    return SanitizedLink(service="jitsi", url=normalized, meeting_id=meeting_id)


def sanitize_meeting_link(raw_url: Optional[str]) -> Optional[SanitizedLink]:
    """Return normalized meeting link and extracted details.

    - Unwrap Outlook SafeLinks
    - Normalize Zoom /join?confno= to /j/<id> and keep pwd
    - Strip tracking params for Meet/Jitsi
    """
    if not raw_url:
        return None
    url = _cleanup_outlook_safelinks(raw_url)
    low = url.lower()
    result: SanitizedLink
    try:
        if "zoom.us/" in low or low.startswith("zoommtg://") or "zoomgov.com/" in low:
            result = _normalize_zoom(url)
        elif "meet.google.com" in low:
            result = _normalize_meet(url)
        elif "teams.microsoft.com" in low:
            result = _normalize_teams(url)
        elif "meet.jit.si" in low:
            result = _normalize_jitsi(url)
        else:
            result = SanitizedLink(service=None, url=url)
    except Exception:  # noqa: BLE001
        result = SanitizedLink(service=None, url=url)
    return result


class NotificationManager:
    def __init__(self, args: argparse.Namespace):
        self.args = args
        self.cache_path = Path(args.cache_dir) / "cache.json"
        self.snooze_path = Path(args.cache_dir) / "snooze_until"

    def notify_if_needed(
        self, title: str, start_date: datetime.datetime, end_date: datetime.datetime
    ):
        if not NOTIFY_PROGRAM:
            return
        if self._is_snoozed():
            return

        uuid = self._generate_uuid(title, start_date, end_date)
        if self._is_already_notified(uuid):
            return

        self._mark_as_notified(uuid)
        self._send_notification(title, start_date, end_date)

    def _generate_uuid(
        self, title: str, start_date: datetime.datetime, end_date: datetime.datetime
    ) -> str:
        content = f"{title}{start_date}{end_date}".encode("utf-8")
        return hashlib.md5(content).hexdigest()

    def _is_already_notified(self, uuid: str) -> bool:
        if not self.cache_path.exists():
            return False

        try:
            with self.cache_path.open() as f:
                cached = json.load(f)
            return uuid in cached
        except (json.JSONDecodeError, IOError):
            return False

    def _mark_as_notified(self, uuid: str):
        cached = []
        if self.cache_path.exists():
            try:
                with self.cache_path.open() as f:
                    cached = json.load(f)
            except (json.JSONDecodeError, IOError):
                cached = []

        cached.append(uuid)
        if len(cached) > MAX_CACHED_ENTRIES:
            cached = cached[-MAX_CACHED_ENTRIES:]

        with self.cache_path.open("w") as f:
            json.dump(cached, f)

    def _send_notification(
        self, title: str, start_date: datetime.datetime, end_date: datetime.datetime
    ):
        cmd = [
            NOTIFY_PROGRAM,
            "-i",
            os.path.expanduser(self.args.notify_icon),
            title,
            f"Start: {start_date.strftime('%H:%M')} End: {end_date.strftime('%H:%M')}",
        ]

        # Urgency level
        if getattr(self.args, "notify_urgency", None):
            cmd.extend(["-u", self.args.notify_urgency])

        if self.args.notify_expiry > 0:
            cmd.extend(["-t", str(self.args.notify_expiry * 60 * 1000)])
        elif self.args.notify_expiry < 0:
            cmd.extend(["-t", str(NOTIFY_MIN_BEFORE_EVENTS * 60 * 1000)])

        subprocess.call(cmd)

    def _is_snoozed(self) -> bool:
        try:
            if not self.snooze_path.exists():
                return False
            until = float(self.snooze_path.read_text().strip())
            return datetime.datetime.now().timestamp() < until
        except Exception:  # noqa: BLE001
            return False

    def set_snooze(self, minutes: int):
        until = datetime.datetime.now().timestamp() + minutes * 60
        try:
            self.snooze_path.write_text(str(until))
        except Exception:  # noqa: BLE001
            pass

    def notify_morning_agenda_if_needed(self, meetings: list[Meeting]):
        target = getattr(self.args, "morning_agenda", None)
        if not target or not NOTIFY_PROGRAM or self._is_snoozed():
            return
        try:
            hour, minute = [int(x) for x in str(target).split(":")[:2]]
        except Exception:  # noqa: BLE001
            return
        now = datetime.datetime.now()
        if not (
            now.hour == hour and abs(now.minute - minute) <= AGENDA_MINUTE_TOLERANCE
        ):
            return
        # ensure only once per day using cache uuid
        today_key = f"agenda-{now.strftime('%Y%m%d')}"
        try:
            with self.cache_path.open() as f:
                cached = json.load(f)
        except Exception:  # noqa: BLE001
            cached = []
        if today_key in cached:
            return
        cached.append(today_key)
        with self.cache_path.open("w") as f:
            json.dump(cached, f)
        # Build body from today's meetings only
        today_meetings = [m for m in meetings if m.start_time.date() == now.date()]
        if not today_meetings:
            return
        lines = [f"{m.start_time.strftime('%H:%M')} {m.title}" for m in today_meetings]
        body = "\n".join(lines)
        cmd = [
            NOTIFY_PROGRAM,
            "-i",
            os.path.expanduser(self.args.notify_icon),
            "Today's meetings",
            body,
        ]
        if getattr(self.args, "notify_urgency", None):
            cmd.extend(["-u", self.args.notify_urgency])
        subprocess.call(cmd)


class OutputFormatter:
    def __init__(self, args: argparse.Namespace):
        self.args = args
        self.formatter = MeetingFormatter(args)

    def format_meetings(self, meetings: list[Meeting]) -> tuple[list[str], str]:
        """Format meetings for output."""
        results = []
        css_class = ""

        for meeting in meetings:
            if self._should_skip_meeting(meeting):
                continue

            formatted_meeting, meeting_css = self.formatter.format_meeting(
                meeting, hyperlink=not self.args.waybar
            )
            results.append(formatted_meeting)

            if meeting_css and not css_class:  # Use first non-empty CSS class
                css_class = meeting_css

        return results, css_class

    def _should_skip_meeting(self, meeting: Meeting) -> bool:
        today = datetime.datetime.now()
        skip = False

        if (
            self.args.today_only
            and meeting.start_time.date() != today.date()
            and not meeting.is_ongoing
        ):
            skip = True

        if not skip and self.args.skip_all_day_meeting and meeting.is_all_day:
            skip = True

        if not skip:
            title_lc = meeting.title.lower()
            if self.args.include_title and not any(
                term.lower() in title_lc for term in self.args.include_title
            ):
                skip = True
            if self.args.exclude_title and any(
                term.lower() in title_lc for term in self.args.exclude_title
            ):
                skip = True

        if not skip:
            cal_url_lc = meeting.calendar_url.lower()
            include_cal = getattr(self.args, "include_calendar", []) or []
            exclude_cal = getattr(self.args, "exclude_calendar", []) or []
            if include_cal and not any(
                term.lower() in cal_url_lc for term in include_cal
            ):
                skip = True
            if exclude_cal and any(term.lower() in cal_url_lc for term in exclude_cal):
                skip = True

        if not skip and getattr(self.args, "work_hours", None):
            try:
                start_s, end_s = str(self.args.work_hours).split("-")
                try:
                    hour, minute = map(int, start_s.split(":")[:2])
                    wh_start = datetime.time(hour, minute)
                except (ValueError, TypeError) as exc:
                    raise ValueError(
                        f"Invalid time format for start_s: {repr(start_s)}. Expected format: 'HH:MM' (e.g., '09:00')."
                    ) from exc
                try:
                    hour, minute = map(int, end_s.split(":")[:2])
                    wh_end = datetime.time(hour, minute)
                except (ValueError, TypeError) as exc:
                    raise ValueError(
                        f"Invalid time format for end_s: '{end_s}'. Expected 'HH:MM'."
                    ) from exc
                st = meeting.start_time.time()
                if not meeting.is_ongoing and not wh_start <= st <= wh_end:
                    skip = True
            except Exception:  # noqa: BLE001
                pass

        # Only-within-window filter
        if not skip and getattr(self.args, "within_mins", None):
            try:
                mins = int(self.args.within_mins)
                if mins >= 0:
                    delta = (meeting.start_time - today).total_seconds() / 60.0
                    if not meeting.is_ongoing and delta > mins:
                        skip = True
            except Exception:  # noqa: BLE001
                pass

        # Only events that have a meeting link
        if not skip and getattr(self.args, "only_with_link", False):
            link = sanitize_meeting_link(meeting.meet_url)
            if not link or not link.url:
                skip = True

        return skip

    def format_for_waybar(self, meetings: list[Meeting]) -> dict:
        """Format meetings for waybar output."""
        no_meeting_text = getattr(self.args, "no_meeting_text", NO_MEETING_TEXT)
        if not meetings:
            return {"text": f"{no_meeting_text} {NO_MEETING_ICON}"}

        formatted_meetings, css_class = self.format_meetings(meetings)
        if not formatted_meetings:
            return {"text": f"{no_meeting_text} {NO_MEETING_ICON}"}

        # Get the next meeting to display
        next_meeting = self._get_next_meeting_for_display(meetings, formatted_meetings)

        # Tooltip formatting: allow a separate template
        tooltip_lines = formatted_meetings
        if getattr(self.args, "tooltip_format", None):
            tooltip_lines = []
            for meeting in meetings:
                if self._should_skip_meeting(meeting):
                    continue
                fields, _ = self.formatter._compute_fields(  # pylint: disable=protected-access
                    meeting, hyperlink=False
                )
                tooltip_lines.append(
                    self.args.tooltip_format.format(
                        when=fields["when"],
                        title=fields["title"],
                        start_time=fields["start_time"],
                        end_time=fields["end_time"],
                        meet_url=fields.get("meet_url"),
                        calendar_url=fields.get("calendar_url"),
                        calendar_name=fields.get("calendar_name"),
                        minutes_until=fields.get("minutes_until"),
                        is_all_day=fields.get("is_all_day"),
                        is_ongoing=fields.get("is_ongoing"),
                    )
                )

        # Apply limit to tooltip if requested
        if getattr(self.args, "limit", None):
            tooltip_lines = tooltip_lines[: self.args.limit]

        result = {
            "text": ellipsis(next_meeting, self.args.max_title_length),
            "tooltip": bulletize(tooltip_lines),
        }

        if css_class:
            result["class"] = css_class

        return result

    def format_for_polybar(self, meetings: list[Meeting]) -> str:
        """Format a single-line string for Polybar."""
        no_meeting_text = getattr(self.args, "no_meeting_text", NO_MEETING_TEXT)
        if not meetings:
            return f"{no_meeting_text}"

        formatted_meetings, _ = self.format_meetings(meetings)
        if not formatted_meetings:
            return f"{no_meeting_text}"

        # Reuse the same selection logic as Waybar
        next_meeting = self._get_next_meeting_for_display(meetings, formatted_meetings)
        return ellipsis(next_meeting, self.args.max_title_length)

    def _get_next_meeting_for_display(
        self, meetings: list[Meeting], formatted_meetings: list[str]
    ) -> str:
        """Get the next meeting to display in waybar."""
        if self.args.waybar_show_all_day_meeting:
            return formatted_meetings[0]

        # Find next non-all-day meeting
        for idx, meeting in enumerate(meetings):
            if not meeting.is_all_day:
                return formatted_meetings[idx]

        # If only all-day meetings, return the first one
        return formatted_meetings[0]


# Utility functions
def ellipsis(string: str, length: int) -> str:
    clean_string = re.sub(r"<[^>]*>", "", string)
    return (
        clean_string[: length - 3] + "..."
        if len(clean_string) > length
        else clean_string
    )


def debug(msg: str, args: argparse.Namespace):
    if args.debug:
        print(f"[DEBUG] {msg}", file=sys.stderr)


def make_hyperlink(uri: str, label: str = "") -> str:
    if label is None:
        label = uri
    return f"\033]8;;{uri}\033\\{label}\033]8;;\033\\"


def replace_domain_url(domain: str, url: str) -> str:
    return url.replace(GOOGLE_CALENDAR_PUBLIC_URL, f"calendar.google.com/a/{domain}")


def build_calendar_day_url(
    date: datetime.datetime, domain: Optional[str] = None
) -> str:
    y, m, d = date.year, date.month, date.day
    base = f"https://calendar.google.com/calendar/u/0/r/day/{y}/{m}/{d}"
    if domain:
        base = f"https://calendar.google.com/a/{domain}/r/day/{y}/{m}/{d}"
    return base


def bulletize(items: list[str]) -> str:
    return "• " + "\n• ".join(items)


def notify(
    title: str,
    start_date: datetime.datetime,
    end_date: datetime.datetime,
    args: argparse.Namespace,
):
    """Legacy notification function for backward compatibility."""
    notification_manager = NotificationManager(args)
    notification_manager.notify_if_needed(title, start_date, end_date)


def get_next_meeting(meetings: list[Meeting], skip_all_day: bool) -> Optional[Meeting]:
    if not meetings:
        return None
    if not skip_all_day:
        return meetings[0]
    return next((m for m in meetings if not m.is_all_day), None)


def open_url(url: str, args: Optional[argparse.Namespace] = None):
    # Allow routing to a specific command (e.g., browser profile)
    if args and getattr(args, "open_with", None):
        try:
            cmd = shlex.split(str(args.open_with)) + [url]
            with subprocess.Popen(
                cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
            ):
                pass
            return
        except Exception:  # noqa: BLE001
            pass
    webbrowser.open_new_tab(url)


def copy_to_clipboard(text: str) -> bool:
    """Copy text to clipboard using common tools. Returns True on success."""
    candidates = []
    if shutil.which("wl-copy"):
        candidates.append(["wl-copy"])
    if shutil.which("xclip"):
        candidates.append(["xclip", "-selection", "clipboard"])
    if shutil.which("pbcopy"):
        candidates.append(["pbcopy"])
    for cmd in candidates:
        try:
            res = subprocess.run(
                cmd,
                input=text.encode("utf-8"),
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if res.returncode == 0:
                return True
        except Exception:  # noqa: BLE001 - best effort
            continue
    return False


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Next meeting scheduler")

    # Core options
    parser.add_argument(
        "--gcalcli-cmdline", default=GCALCLI_CMDLINE, help="gcalcli command line used"
    )
    parser.add_argument("--debug", action="store_true", help="Enable debug mode")
    parser.add_argument("--calendar", default=DEFAULT_CALENDAR, help="Calendar to use")
    parser.add_argument(
        "-v", "--verbose", action="store_true", help="Print detailed error tracebacks"
    )

    # CalDAV options
    parser.add_argument(
        "--caldav-url",
        help="CalDAV server URL (e.g., https://localhost:5232/user/calendar/)",
    )
    parser.add_argument("--caldav-username", help="CalDAV username")
    parser.add_argument("--caldav-password", help="CalDAV password")
    parser.add_argument(
        "--caldav-calendar",
        action="append",
        help="CalDAV calendar name or full URL (repeatable, defaults to all available if none provided)",
    )
    parser.add_argument(
        "--caldav-lookahead-hours",
        type=int,
        default=CALDAV_DEFAULT_LOOKAHEAD_HOURS,
        help="Hours ahead of now to include CalDAV events",
    )
    parser.add_argument(
        "--caldav-lookbehind-hours",
        type=int,
        default=CALDAV_DEFAULT_LOOKBEHIND_HOURS,
        help="Hours before now to include CalDAV events (captures ongoing meetings)",
    )
    parser.add_argument(
        "--caldav-disable-tls-verify",
        action="store_true",
        help="Disable TLS certificate verification for CalDAV requests",
    )

    # Display options
    parser.add_argument("--waybar", action="store_true", help="Output JSON for waybar")
    parser.add_argument(
        "--polybar", action="store_true", help="Output text for Polybar"
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output JSON (same structure as --waybar)",
    )
    parser.add_argument(
        "--waybar-show-all-day-meeting",
        action="store_true",
        help="Show all-day meetings in waybar",
    )
    parser.add_argument(
        "--max-title-length",
        type=int,
        default=TITLE_ELIPSIS_LENGTH,
        help="Maximum title length",
    )
    parser.add_argument(
        "--today-only", action="store_true", help="Show only today's meetings"
    )
    parser.add_argument(
        "--time-format",
        choices=["24h", "12h"],
        default="24h",
        help="Time display format for absolute times",
    )
    parser.add_argument(
        "--privacy",
        action="store_true",
        help="Redact meeting titles (display as Busy)",
    )
    parser.add_argument(
        "--privacy-title",
        default="Busy",
        help="Replacement title when --privacy is enabled",
    )
    parser.add_argument(
        "--limit",
        type=int,
        help="Limit the number of meetings shown in lists/tooltips",
    )
    parser.add_argument(
        "--format",
        help=(
            "Custom line template, placeholders: {when}, {title}, {start_time}, {end_time}, "
            "{meet_url}, {calendar_url}, {calendar_name}, {minutes_until}, {is_all_day}, {is_ongoing}"
        ),
    )
    parser.add_argument(
        "--tooltip-format",
        help=("Custom tooltip template (Waybar), placeholders same as --format"),
    )
    parser.add_argument(
        "--work-hours",
        help="Only show meetings whose start time is within HH:MM-HH:MM",
    )
    parser.add_argument(
        "--within-mins",
        type=int,
        help="Only show meetings starting within N minutes (ongoing always shown)",
    )
    parser.add_argument(
        "--only-with-link",
        action="store_true",
        help="Show only events that have a meeting link",
    )
    parser.add_argument(
        "--hour-separator",
        default=HOUR_SEPARATOR,
        help=f"Hour separator in time display (default '{HOUR_SEPARATOR}')",
    )
    parser.add_argument(
        "--until-offset",
        type=int,
        default=UNTIL_OFFSET,
        help=f"How many minutes before event before we start showing until information (default '{UNTIL_OFFSET}')",
    )
    parser.add_argument(
        "--no-meeting-text",
        default=NO_MEETING_TEXT,
        help="Text to display when there is no meeting",
    )

    # Meeting filtering
    parser.add_argument(
        "--skip-all-day-meeting",
        "-S",
        action="store_true",
        help="Skip all-day meetings",
    )
    parser.add_argument(
        "--include-title",
        action="append",
        default=[],
        help="Only include meetings whose title contains any of these terms (repeatable)",
    )
    parser.add_argument(
        "--exclude-title",
        action="append",
        default=[],
        help="Exclude meetings whose title contains any of these terms (repeatable)",
    )
    parser.add_argument(
        "--include-calendar",
        action="append",
        default=[],
        help="Only include meetings whose calendar URL contains any of these terms (repeatable)",
    )
    parser.add_argument(
        "--exclude-calendar",
        action="append",
        default=[],
        help="Exclude meetings whose calendar URL contains any of these terms (repeatable)",
    )
    parser.add_argument(
        "--all-day-meeting-hours",
        type=int,
        default=ALL_DAYS_MEETING_HOURS,
        help="Hours that constitute an all-day meeting",
    )

    # Notification options
    parser.add_argument(
        "--notify-min-before-events",
        type=int,
        default=NOTIFY_MIN_BEFORE_EVENTS,
        help="Minutes before event to notify",
    )
    parser.add_argument(
        "--notify-expiry", type=int, default=0, help="Notification expiry in minutes"
    )
    parser.add_argument("--notify-icon", default=NOTIFY_ICON, help="Notification icon")
    parser.add_argument(
        "--notify-offsets",
        action="append",
        default=[],
        help="Additional minute offsets to notify before start (repeatable or comma-separated)",
    )
    parser.add_argument(
        "--notify-urgency",
        choices=["low", "normal", "critical"],
        help="Set notification urgency level",
    )
    parser.add_argument(
        "--snooze",
        type=int,
        help="Snooze notifications for N minutes and exit",
    )
    parser.add_argument(
        "--morning-agenda",
        help="Send a once-per-day agenda notification at HH:MM",
    )
    parser.add_argument(
        "--notify-min-color",
        default=NOTIFY_MIN_COLOR,
        help="Color for urgent notifications",
    )
    parser.add_argument(
        "--notify-min-color-foreground",
        default=NOTIFY_MIN_COLOR_FOREGROUND,
        help="Foreground color for urgent notifications",
    )

    # URL options
    parser.add_argument("--open-meet-url", action="store_true", help="Open meeting URL")
    parser.add_argument(
        "--copy-meeting-url",
        action="store_true",
        help="Copy next meeting URL to clipboard",
    )
    parser.add_argument(
        "--copy-meeting-id",
        action="store_true",
        help="Copy next meeting ID to clipboard (if detected)",
    )
    parser.add_argument(
        "--copy-meeting-passcode",
        action="store_true",
        help="Copy next meeting passcode to clipboard (if detected)",
    )
    parser.add_argument(
        "--google-domain",
        default=os.environ.get("NEXTMEETING_GOOGLE_DOMAIN"),
        help="Google domain for calendar URLs",
    )
    parser.add_argument(
        "--open-calendar-day",
        action="store_true",
        help="Open Google Calendar day view of the next meeting",
    )
    parser.add_argument(
        "--open-with",
        help="Open links with a specific command (e.g., 'firefox -P Work')",
    )
    parser.add_argument(
        "--open-link-from-clipboard",
        action="store_true",
        help="Detect a meeting link in clipboard and open it",
    )
    parser.add_argument(
        "--create",
        choices=["meet", "zoom", "teams", "gcal"],
        help="Quick-create a meeting in the chosen service",
    )
    parser.add_argument(
        "--create-url",
        help="Custom URL for --create (overrides service URL)",
    )

    # Cache options
    parser.add_argument(
        "--cache-dir", type=Path, default=CACHE_DIR, help="Cache directory location"
    )
    parser.add_argument(
        "--cache-events-ttl",
        type=int,
        default=0,
        help="Cache gcalcli events for N minutes (0 disables)",
    )

    # Config
    parser.add_argument(
        "--config",
        type=Path,
        help="Path to a TOML config file (defaults to ~/.config/nextmeeting/config.toml if present)",
    )

    return parser


def _load_config(path: Path) -> dict:
    if not path.exists() or not path.is_file():
        return {}
    if not tomllib:
        return {}
    try:
        with path.open("rb") as f:
            data = tomllib.load(f)
        # Allow a top-level [nextmeeting] table or flat keys
        if "nextmeeting" in data and isinstance(data["nextmeeting"], dict):
            config_data = data["nextmeeting"]
        else:
            config_data = data

        # Normalize keys: convert hyphens to underscores to match argparse behavior
        # This allows both caldav-url and caldav_url in config files
        normalized_config = {
            key.replace("-", "_"): value for key, value in config_data.items()
        }

        return normalized_config
    except Exception:  # noqa: BLE001
        return {}


def parse_args() -> argparse.Namespace:
    parser = _build_parser()
    # First pass to discover --config or default file
    preliminary, _ = parser.parse_known_args()
    config_path = preliminary.config or Path(
        os.path.expanduser("~/.config/nextmeeting/config.toml")
    )
    cfg = _load_config(config_path)
    if cfg:
        # For action='append' arguments: if CLI provided them, exclude from config
        # This ensures CLI replaces config instead of appending to it
        if preliminary.caldav_calendar is not None and "caldav_calendar" in cfg:
            cfg = {k: v for k, v in cfg.items() if k != "caldav_calendar"}

        # Set defaults from config for any matching keys
        parser.set_defaults(**cfg)
    args = parser.parse_args()

    # Normalize caldav_calendar: if None (no config, no CLI), set to empty list
    if args.caldav_calendar is None:
        args.caldav_calendar = []

    return args


def main():
    args = parse_args()
    try:
        return _run(args)
    except Exception as exc:  # noqa: BLE001
        if getattr(args, "verbose", False) or getattr(args, "debug", False):
            traceback.print_exc()
        else:
            print(str(exc), file=sys.stderr)
        return 1


def _run(args: argparse.Namespace):
    args.cache_dir.mkdir(parents=True, exist_ok=True)

    # Handle snooze action
    if args.snooze and args.snooze > 0:
        NotificationManager(args).set_snooze(args.snooze)
        print(f"Snoozed notifications for {args.snooze} minutes")
        return

    use_caldav = bool(getattr(args, "caldav_url", None))

    if use_caldav and args.calendar and not args.caldav_calendar:
        args.caldav_calendar = [args.calendar]  # Wrap in list

    if args.calendar and not use_caldav:
        args.gcalcli_cmdline = f"{args.gcalcli_cmdline} --calendar {args.calendar}"

    # Fetch meetings
    if use_caldav:
        fetcher = CalDavMeetingFetcher(meeting_factory=Meeting)
    else:
        fetcher = MeetingFetcher()
    meetings = fetcher.fetch_meetings(args)

    # Morning agenda (best-effort, once per day)
    try:
        NotificationManager(args).notify_morning_agenda_if_needed(meetings)
    except Exception:
        pass

    if not meetings:
        no_meeting_text = getattr(args, "no_meeting_text", NO_MEETING_TEXT)
        output = (
            {"text": f"{no_meeting_text} {NO_MEETING_ICON}"}
            if (args.waybar or args.json)
            else f"{no_meeting_text}"
        )
        if args.polybar and not (args.waybar or args.json):
            print(f"{no_meeting_text}")
        elif args.waybar or args.json:
            json.dump(output, sys.stdout)
        else:
            print(output)
        return

    # Handle URL-related actions
    if _handle_url_actions(args, meetings):
        return

    # Format and output meetings
    formatter = OutputFormatter(args)

    if args.polybar and not (args.waybar or args.json):
        result = formatter.format_for_polybar(meetings)
        print(result)
    elif args.waybar or args.json:
        result = formatter.format_for_waybar(meetings)
        json.dump(result, sys.stdout)
    else:
        formatted_meetings, _ = formatter.format_meetings(meetings)
        if args.limit:
            formatted_meetings = formatted_meetings[: args.limit]
        if not formatted_meetings:
            debug(
                "No meetings detected. Try --calendar to specify another calendar", args
            )
            no_meeting_text = getattr(args, "no_meeting_text", NO_MEETING_TEXT)
            print(f"{no_meeting_text}")
        else:
            print(bulletize(formatted_meetings))


def _handle_url_actions(args: argparse.Namespace, meetings: list[Meeting]) -> bool:
    """Handle open/copy actions; return True if action was taken and program should exit."""
    # Quick-create action
    if args.create:
        create_urls = {
            "meet": "https://meet.google.com/new",
            "zoom": "https://zoom.us/start",
            "teams": "https://teams.microsoft.com/l/meeting/new?subject=",
            "gcal": "https://calendar.google.com/calendar/u/0/r/eventedit",
        }
        target = args.create_url or create_urls.get(args.create)
        if target:
            open_url(target, args)
            return True

    # Open link from clipboard
    if args.open_link_from_clipboard:
        clip = _read_clipboard()
        if clip:
            link = sanitize_meeting_link(clip)
            if link:
                open_url(link.url, args)
                return True
        return False

    if (
        args.open_meet_url
        or args.copy_meeting_url
        or args.copy_meeting_id
        or args.copy_meeting_passcode
        or args.open_calendar_day
    ):
        meeting = get_next_meeting(meetings, args.skip_all_day_meeting)
        if meeting:
            sanitized = sanitize_meeting_link(meeting.meet_url) or SanitizedLink(
                service=None, url=meeting.meet_url or meeting.calendar_url
            )
            url = sanitized.url or meeting.calendar_url
            if args.google_domain:
                url = replace_domain_url(args.google_domain, url)
            if args.open_meet_url:
                open_url(url, args)
            if args.open_calendar_day:
                day_url = build_calendar_day_url(meeting.start_time, args.google_domain)
                open_url(day_url, args)
            if args.copy_meeting_url:
                if not copy_to_clipboard(url):
                    print(url)
            if args.copy_meeting_id:
                value = sanitized.meeting_id or ""
                if not copy_to_clipboard(value):
                    print(value)
            if args.copy_meeting_passcode:
                value = sanitized.passcode or ""
                if not copy_to_clipboard(value):
                    print(value)
        return True
    return False


def _read_clipboard() -> str | None:
    """Return clipboard text using common tools; None on failure."""
    candidates = []
    if shutil.which("wl-paste"):
        candidates.append(["wl-paste", "-n"])
    if shutil.which("xclip"):
        candidates.append(["xclip", "-selection", "clipboard", "-o"])
    if shutil.which("pbpaste"):
        candidates.append(["pbpaste"])
    for cmd in candidates:
        try:
            res = subprocess.run(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                text=True,
                check=False,
            )
            if res.returncode == 0:
                text = (res.stdout or "").strip()
                if text:
                    return text
        except Exception:  # noqa: BLE001
            continue
    return None


if __name__ == "__main__":
    sys.exit(main())
