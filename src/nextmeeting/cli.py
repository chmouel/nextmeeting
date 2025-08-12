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

import argparse
import datetime
import hashlib
import html
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import webbrowser
from dataclasses import dataclass
from datetime import timedelta
from pathlib import Path
from typing import Optional, Sequence

import dateutil.parser as dtparse
import dateutil.relativedelta as dtrel

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


@dataclass
class Meeting:
    title: str
    start_time: datetime.datetime
    end_time: datetime.datetime
    calendar_url: str
    meet_url: Optional[str] = None

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
        self, meeting: Meeting, hyperlink: bool = False
    ) -> tuple[str, str]:
        """Format a single meeting and return (formatted_string, css_class)."""
        title = self._format_title(meeting, hyperlink)

        if meeting.is_ongoing:
            return self._format_ongoing_meeting(meeting, title, hyperlink)
        return self._format_upcoming_meeting(meeting, title, hyperlink)

    def _format_title(self, meeting: Meeting, hyperlink: bool) -> str:
        title = meeting.title
        if self.args.waybar:
            title = html.escape(title)
        if hyperlink and meeting.meet_url:
            title = make_hyperlink(meeting.meet_url, title)
        return title

    def _format_ongoing_meeting(
        self, meeting: Meeting, title: str, hyperlink: bool
    ) -> tuple[str, str]:
        timetofinish = dtrel.relativedelta(meeting.end_time, self.today)
        if timetofinish.hours == 0:
            time_str = f"{timetofinish.minutes} minutes"
        else:
            time_str = f"{timetofinish.hours}H{timetofinish.minutes}"

        thetime = f"{time_str} to go"
        if hyperlink:
            thetime = f"{thetime: <17}"
            thetime = make_hyperlink(meeting.calendar_url, thetime)

        return f"{thetime} - {title}", "current"

    def _format_upcoming_meeting(
        self, meeting: Meeting, title: str, hyperlink: bool
    ) -> tuple[str, str]:
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
        if hyperlink:
            thetime = f"{thetime: <17}"
            url = (
                replace_domain_url(self.args.google_domain, meeting.calendar_url)
                if self.args.google_domain
                else meeting.calendar_url
            )
            thetime = make_hyperlink(url, thetime)

        return f"{thetime} - {title}", css_class

    def _format_time_until(
        self, deltad: dtrel.relativedelta, date: datetime.datetime
    ) -> str:
        if date.day != self.today.day:
            if deltad.days == 0:
                s = "Tomorrow"
            else:
                s = f"{date.strftime('%a %d')}"
            s += f" at {date.hour:02d}h{date.minute:02d}"
        elif deltad.hours != 0:
            s = date.strftime("%HH%M")
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
            s = f"In {deltad.minutes} minutes"
        return s


class MeetingFetcher:
    def __init__(self, gcalcli_cmdline: str = GCALCLI_CMDLINE):
        self.gcalcli_cmdline = gcalcli_cmdline

    def fetch_meetings(self, args: argparse.Namespace) -> list[Meeting]:
        cmdline = getattr(args, "gcalcli_cmdline", self.gcalcli_cmdline)
        debug(f"Executing gcalcli command: {cmdline}", args)

        try:
            result = subprocess.run(
                cmdline,
                shell=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=True,
            )
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


class NotificationManager:
    def __init__(self, args: argparse.Namespace):
        self.args = args
        self.cache_path = Path(args.cache_dir) / "cache.json"

    def notify_if_needed(
        self, title: str, start_date: datetime.datetime, end_date: datetime.datetime
    ):
        if not NOTIFY_PROGRAM:
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

        if self.args.notify_expiry > 0:
            cmd.extend(["-t", str(self.args.notify_expiry * 60 * 1000)])
        elif self.args.notify_expiry < 0:
            cmd.extend(["-t", str(NOTIFY_MIN_BEFORE_EVENTS * 60 * 1000)])

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

        # Skip if --today-only is set and meeting is not today (unless ongoing)
        if (
            self.args.today_only
            and meeting.start_time.date() != today.date()
            and not meeting.is_ongoing
        ):
            return True

        # Skip all-day meetings if requested
        if self.args.skip_all_day_meeting and meeting.is_all_day:
            return True

        # Title include/exclude filters
        title_lc = meeting.title.lower()
        if self.args.include_title:
            if not any(term.lower() in title_lc for term in self.args.include_title):
                return True
        if self.args.exclude_title:
            if any(term.lower() in title_lc for term in self.args.exclude_title):
                return True

        return False

    def format_for_waybar(self, meetings: list[Meeting]) -> dict:
        """Format meetings for waybar output."""
        if not meetings:
            return {"text": "No meeting 🏖️"}

        formatted_meetings, css_class = self.format_meetings(meetings)
        if not formatted_meetings:
            return {"text": "No meeting 🏖️"}

        # Get the next meeting to display
        next_meeting = self._get_next_meeting_for_display(meetings, formatted_meetings)

        result = {
            "text": ellipsis(next_meeting, self.args.max_title_length),
            "tooltip": bulletize(formatted_meetings),
        }

        if css_class:
            result["class"] = css_class

        return result

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


def open_url(url: str):
    webbrowser.open_new_tab(url)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Next meeting scheduler")

    # Core options
    parser.add_argument(
        "--gcalcli-cmdline", default=GCALCLI_CMDLINE, help="gcalcli command line used"
    )
    parser.add_argument("--debug", action="store_true", help="Enable debug mode")
    parser.add_argument("--calendar", default=DEFAULT_CALENDAR, help="Calendar to use")

    # Display options
    parser.add_argument("--waybar", action="store_true", help="Output JSON for waybar")
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
        "--google-domain",
        default=os.environ.get("NEXTMEETING_GOOGLE_DOMAIN"),
        help="Google domain for calendar URLs",
    )

    # Cache options
    parser.add_argument(
        "--cache-dir", type=Path, default=CACHE_DIR, help="Cache directory location"
    )

    return parser.parse_args()


def main():
    args = parse_args()
    args.cache_dir.mkdir(parents=True, exist_ok=True)

    if args.calendar:
        args.gcalcli_cmdline = f"{args.gcalcli_cmdline} --calendar {args.calendar}"

    # Fetch meetings
    fetcher = MeetingFetcher()
    meetings = fetcher.fetch_meetings(args)

    if not meetings:
        output = (
            {"text": "No meeting 🏖️"} if (args.waybar or args.json) else "No meeting"
        )
        if args.waybar or args.json:
            json.dump(output, sys.stdout)
        else:
            print(output)
        return

    # Handle URL opening
    if args.open_meet_url:
        meeting = get_next_meeting(meetings, args.skip_all_day_meeting)
        if meeting:
            url = meeting.meet_url or meeting.calendar_url
            if args.google_domain:
                url = replace_domain_url(args.google_domain, url)
            open_url(url)
        return

    # Format and output meetings
    formatter = OutputFormatter(args)

    if args.waybar or args.json:
        result = formatter.format_for_waybar(meetings)
        json.dump(result, sys.stdout)
    else:
        formatted_meetings, _ = formatter.format_meetings(meetings)
        if not formatted_meetings:
            debug(
                "No meetings detected. Try --calendar to specify another calendar", args
            )
            print("No meeting")
        else:
            print(bulletize(formatted_meetings))


if __name__ == "__main__":
    main()
