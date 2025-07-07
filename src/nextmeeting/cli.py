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
#
# Features
#
# smart date in english (not just the date, tomorrow or others)
# time to go for current meeting
# change colors if there is 5 minutes to go to the meeting
# hyperlink in default view to click on terminal
# notification via notify-send 5 minutes before meeting
# title ellipsis
#
# Install: configure gcalcli https://github.com/insanum/gcalcli
# Use it like you want, ie.: waybar
#
# "custom/agenda": {
#     "format": "{}",
#     "exec": "nextmeeting.py --waybar",
#     "on-click": "nextmeeting.py --open-meet-url;swaymsg '[app=chromium] focus'",
#     "on-click-right": "kitty -- /bin/bash -c \"cal -3;echo;nextmeeting;read;\"",
#     "interval": 59
# },
#
# see --help for other customization
#
# Screenshot: https://user-images.githubusercontent.com/98980/192647099-ccfa2002-0db3-4738-a54b-176a03474483.png
#

import argparse
import datetime
import hashlib
import html
import json
import os.path
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

# pylint: disable=import-error
import dateutil.parser as dtparse
import dateutil.relativedelta as dtrel

REG_TSV = re.compile(
    r"(?P<startdate>(\d{4})-(\d{2})-(\d{2}))\s*?(?P<starthour>(\d{2}:\d{2}))\s*(?P<enddate>(\d{4})-(\d{2})-(\d{2}))\s*?(?P<endhour>(\d{2}:\d{2}))\s*(?P<calendar_url>(https://\S+))\s*(?P<meet_url>(https://\S*)?)\s*(?P<title>.*)$"
)
DEFAULT_CALENDAR = os.environ.get("GCALCLI_DEFAULT_CALENDAR", "")
GCALCLI_CMDLINE = (
    "gcalcli --nocolor agenda today --nodeclined  --details=end --details=url --tsv "
)
TITLE_ELIPSIS_LENGTH = 50
MAX_CACHED_ENTRIES = 30
NOTIFY_MIN_BEFORE_EVENTS = 5
NOTIFY_MIN_COLOR = "#FF5733"  # red
NOTIFY_MIN_COLOR_FOREGROUND = "#F4F1DE"  # white
CACHE_DIR = pathlib.Path(os.path.expanduser("~/.cache/nextmeeting"))
NOTIFY_PROGRAM: str = shutil.which("notify-send") or ""
NOTIFY_ICON = "/usr/share/icons/hicolor/scalable/apps/org.gnome.Calendar.svg"
GOOGLE_CALENDAR_PUBLIC_URL = "www.google.com/calendar"
ALL_DAYS_MEETING_HOURS = 24


@dataclass
class Meeting:
    title: str
    start_time: datetime.datetime
    end_time: datetime.datetime
    calendar_url: str
    meet_url: Optional[str]

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


def ellipsis(string: str, length: int) -> str:
    # remove all html elements first from it
    hstring = re.sub(r"<[^>]*>", "", string)
    if len(hstring) > length:
        return hstring[: length - 3] + "..."
    return hstring


def debug(msg: str, args: argparse.Namespace):
    """Print debug messages if --debug is enabled."""
    if args.debug:
        print(f"[DEBUG] {msg}", file=sys.stderr)


def open_url(url: str):
    webbrowser.open_new_tab(url)


def pretty_date(
    deltad: dtrel.relativedelta, date: datetime.datetime, args: argparse.Namespace
) -> str:
    today = datetime.datetime.now()
    s = ""
    if date.day != today.day:
        if deltad.days == 0:
            s = "Tomorrow"
        else:
            s = f"{date.strftime('%a %d')}"
        # pylint: disable=consider-using-f-string
        s += " at %02dh%02d" % (
            date.hour,
            date.minute,
        )  # pylint: disable=consider-using-f-string
    elif deltad.hours != 0:
        s = date.strftime("%HH%M")
    elif deltad.days < 0 or deltad.hours < 0 or deltad.minutes < 0:
        s = "Now"
    elif (
        deltad.minutes <= NOTIFY_MIN_BEFORE_EVENTS
        and args.notify_min_color
        and args.waybar
    ):
        number = f"""<span background=\"{args.notify_min_color}\" color=\"{args.notify_min_color_foreground}\">{deltad.minutes}</span>"""
        s = f"In {number} minutes"
    else:
        s = f"In {deltad.minutes} minutes"
    return s


def make_hyperlink(uri: str, label: None | str = None):
    if label is None:
        label = uri
    parameters = ""

    # OSC 8 ; params ; URI ST <name> OSC 8 ;; ST
    escape_mask = "\033]8;{};{}\033\\{}\033]8;;\033\\"
    return escape_mask.format(parameters, uri, label)


# pylint: disable=too-few-public-methods
class MeetingFetcher:
    def __init__(self, gcalcli_cmdline: str = GCALCLI_CMDLINE):
        self.gcalcli_cmdline = gcalcli_cmdline

    def fetch_meetings(self, args: argparse.Namespace) -> list[Meeting]:
        cmdline = (
            args.gcalcli_cmdline
            if hasattr(args, "gcalcli_cmdline")
            else self.gcalcli_cmdline
        )
        debug(f"Executing gcalcli command: {cmdline}", args)
        with subprocess.Popen(
            cmdline, shell=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE
        ) as cmd:
            stdout, stderr = cmd.communicate()  # Wait for the process to complete
            if cmd.returncode:
                calendar_list_cmd = subprocess.run(
                    "gcalcli list",
                    shell=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    check=False,
                )
                calendar_list = calendar_list_cmd.stdout.decode().strip()
                debug(stderr.decode(), args)
                raise RuntimeError(
                    f"""-----\ngcalcli command failed with exit code {cmd.returncode}, command: {cmdline}\nCalendar available:\n{calendar_list}\nTry nextmeeting --work=CALENDAR option to target the right calendar.\n\nUse --debug to see the full error message.\n"""
                )
            return process_lines(stdout.decode().splitlines())


def gcalcli_output(args: argparse.Namespace) -> list[Meeting]:
    debug(f"Executing gcalcli command: {args.gcalcli_cmdline}", args)
    with subprocess.Popen(
        args.gcalcli_cmdline, shell=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    ) as cmd:
        stdout, stderr = cmd.communicate()  # Wait for the process to complete
        if cmd.returncode:
            calendar_list_cmd = subprocess.run(
                "gcalcli list",
                shell=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            calendar_list = calendar_list_cmd.stdout.decode().strip()
            debug(stderr.decode(), args)
            raise RuntimeError(
                f"""-----
gcalcli command failed with exit code {cmd.returncode}, command: {args.gcalcli_cmdline}
Calendar available:
{calendar_list}
Try nextmeeting --work=CALENDAR option to target the right calendar.

Use --debug to see the full error message.
"""
            )
        return process_lines(stdout.decode().splitlines())


def process_lines(lines: Sequence[str | bytes]) -> list[Meeting]:
    """Process gcalcli output lines into Meeting objects."""
    meetings = []
    now = datetime.datetime.now()

    for line in lines:
        try:
            if isinstance(line, memoryview):
                line_str = line.tobytes().decode().strip()
            elif isinstance(line, bytes):
                line_str = line.decode().strip()
            else:
                line_str = line.strip()
        except (AttributeError, UnicodeDecodeError):
            continue

        if isinstance(line_str, str) and (match := REG_TSV.match(line_str)):
            meeting = Meeting.from_match(match)
            if meeting.end_time > now:
                meetings.append(meeting)

    return meetings


def ret_events(
    meetings: list[Meeting], args: argparse.Namespace, hyperlink: bool = False
) -> tuple[list[str], str]:
    ret = []
    cssclass = ""
    today = datetime.datetime.now()
    for meeting in meetings:
        title = meeting.title
        startdate = meeting.start_time
        enddate = meeting.end_time
        # Skip if --today-only is set and the meeting is not today, unless ongoing
        if (
            args.today_only
            and startdate.date() != today.date()
            and not (startdate <= datetime.datetime.now() <= enddate)
        ):
            continue
        if args.waybar:
            title = html.escape(title)
        if hyperlink and meeting.meet_url:
            title = make_hyperlink(meeting.meet_url, title)
        if args.skip_all_day_meeting and meeting.is_all_day:
            continue
        if datetime.datetime.now() >= startdate and datetime.datetime.now() <= enddate:
            cssclass = "current"
            timetofinish = dtrel.relativedelta(enddate, datetime.datetime.now())
            if timetofinish.hours == 0:
                s = f"{timetofinish.minutes} minutes"
            else:
                s = f"{timetofinish.hours}H{timetofinish.minutes}"
            thetime = f"{s} to go"
            if hyperlink:
                thetime = f"{thetime: <17}"
            if hyperlink:
                thetime = make_hyperlink(meeting.calendar_url, thetime)
            ret.append(f"{thetime} - {title}")
        else:
            timeuntilstarting = dtrel.relativedelta(startdate, datetime.datetime.now())
            url = meeting.calendar_url
            if args.google_domain:
                url = replace_domain_url(args.google_domain, url)
            # Only notify if meeting is in the future
            if (
                not timeuntilstarting.days
                and not timeuntilstarting.hours
                and 0 <= timeuntilstarting.minutes <= args.notify_min_before_events
            ):
                cssclass = "soon"
                notify(title, startdate, enddate, args)
            thetime = pretty_date(timeuntilstarting, startdate, args)
            if hyperlink:
                thetime = f"{thetime: <17}"
                thetime = make_hyperlink(
                    replace_domain_url(args.google_domain, meeting.calendar_url)
                    if args.google_domain
                    else meeting.calendar_url,
                    thetime,
                )
            ret.append(f"{thetime} - {title}")
    return ret, cssclass


def notify(
    title: str,
    start_date: datetime.datetime,
    end_date: datetime.datetime,
    args: argparse.Namespace,
):
    t = f"{title}{start_date}{end_date}".encode("utf-8")
    uuid = hashlib.md5(t).hexdigest()
    notified = False
    cached = []
    cache_path = args.cache_dir / "cache.json"
    if cache_path.exists():
        with cache_path.open() as f:
            try:
                cached = json.load(f)
            except json.JSONDecodeError:
                cached = []
            if uuid in cached:
                notified = True
            debug(
                f"Notification status for UUID {uuid}: {'Notified' if notified else 'Not Notified'}",
                args,
            )
    if notified:
        return
    cached.append(uuid)
    with cache_path.open("w") as f:
        if len(cached) >= MAX_CACHED_ENTRIES:
            cached = cached[-MAX_CACHED_ENTRIES:]
        json.dump(cached, f)
    if NOTIFY_PROGRAM == "":
        return
    other_args = []
    if args.notify_expiry > 0:
        milliseconds = args.notify_expiry * 60 * 1000
        other_args += ["-t", str(milliseconds)]
    elif args.notify_expiry < 0:
        milliseconds = NOTIFY_MIN_BEFORE_EVENTS * 60 * 1000
        other_args += ["-t", str(milliseconds)]
    subprocess.call(
        [
            NOTIFY_PROGRAM,
            "-i",
            os.path.expanduser(args.notify_icon),
            *other_args,
            title,
            f"Start: {start_date.strftime('%H:%M')} End: {end_date.strftime('%H:%M')}",
        ]
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--gcalcli-cmdline",
        help="gcalcli command line used, --calendar will be added if you specify one to nextmeeting",
        default=GCALCLI_CMDLINE,
    )
    # Add the --debug flag
    parser.add_argument(
        "--debug", action="store_true", help="Enable debug mode to log detailed actions"
    )
    parser.add_argument(
        "--waybar", action="store_true", help="get a json for to display for waybar"
    )

    parser.add_argument(
        "--waybar-show-all-day-meeting",
        action="store_true",
        help="show all day meeting in next event for waybar",
    )

    parser.add_argument(
        "--all-day-meeting-hours",
        default=ALL_DAYS_MEETING_HOURS,
        help=f"how long is an all day meeting in hours, (default: {ALL_DAYS_MEETING_HOURS})",
    )

    parser.add_argument(
        "--notify-expiry",
        type=int,
        help="notification expiration in minutes (0 no expiry, -1 show notification until the meeting start)",
        default=0,
    )

    parser.add_argument(
        "--open-meet-url", action="store_true", help="click on invite url"
    )
    parser.add_argument("--max-title-length", type=int, default=TITLE_ELIPSIS_LENGTH)
    parser.add_argument(
        "--cache-dir", default=CACHE_DIR.expanduser(), help="cache dir location"
    )

    parser.add_argument(
        "--skip-all-day-meeting", "-S", action="store_true", help="skip all day meeting"
    )

    parser.add_argument(
        "--google-domain",
        help="let you specify your google domain instead of the default google.com one",
        default=os.environ.get("NEXTMEETING_GOOGLE_DOMAIN"),
    )
    parser.add_argument(
        "--notify-min-before-events",
        type=int,
        default=NOTIFY_MIN_BEFORE_EVENTS,
        help="How many before minutes to notify the events is coming up",
    )
    parser.add_argument(
        "--notify-min-color",
        default=NOTIFY_MIN_COLOR,
        help="How many before minutes to notify the events is coming up",
    )

    parser.add_argument(
        "--notify-min-color-foreground",
        default=NOTIFY_MIN_COLOR_FOREGROUND,
        help="How many before minutes to notify the events is coming up",
    )

    parser.add_argument(
        "--notify-icon",
        default=NOTIFY_ICON,
        help="Notification icon to use for the notify-send",
    )
    parser.add_argument(
        "--calendar",
        default=os.environ.get("GCALCLI_DEFAULT_CALENDAR"),
        help="calendar to use",
    )
    parser.add_argument(
        "--today-only",
        action="store_true",
        help="Show only meetings scheduled for today",
    )
    return parser.parse_args()


def replace_domain_url(domain, url: str) -> str:
    return url.replace(
        GOOGLE_CALENDAR_PUBLIC_URL,
        f"calendar.google.com/a/{domain}",
    )


def bulletize(rets: list[str]) -> str:
    return "• " + "\n• ".join(rets)


def get_next_non_all_day_meeting(
    meetings: list[Meeting], rets: list[str], all_day_meeting_hours: int
) -> None | str:
    for idx, m in enumerate(meetings):
        start_date = m.start_time
        end_date = m.end_time
        if end_date > (start_date + timedelta(hours=all_day_meeting_hours)):
            continue
        return rets[idx]
    return None


def get_next_meeting(meetings: list[Meeting], skip_all_day: bool) -> Optional[Meeting]:
    for m in meetings:
        if skip_all_day and m.is_all_day:
            continue
        return m
    return None


def open_meet_url(rets, matches: list[re.Match], args: argparse.Namespace):
    url = ""
    if not rets:
        print("No meeting 🏖️")
        return
    for match in matches:
        startdate = dtparse.parse(
            f"{match.group('startdate')} {match.group('starthour')}"
        )
        enddate = dtparse.parse(f"{match.group('enddate')} {match.group('endhour')}")
        if (
            args.skip_all_day_meeting
            and dtrel.relativedelta(enddate, startdate).days >= 1
        ):
            continue
        url = match.group("meet_url")
        if not url:
            url = match.group("calendar_url")
            # TODO: go over the description and detect zoom and other stuff
            # gnome-next-meeting-applet has a huge amount of regexp for that already we can reuse
            # Maybe show a dialog with the description and the user can click on the link with some gtk
            if args.google_domain:
                url = replace_domain_url(args.google_domain, url)
        break
    # TODO: we can't do the "domain" switch thing on meet url that are not
    # calendar, maybe specify a /u/number/ for multi accounts ?
    if url:
        open_url(url)
    sys.exit(0)


def main():
    args = parse_args()
    Path(args.cache_dir).mkdir(parents=True, exist_ok=True)

    if args.calendar:
        args.gcalcli_cmdline = f"{args.gcalcli_cmdline} --calendar {args.calendar}"

    fetcher = MeetingFetcher()
    meetings = fetcher.fetch_meetings(args)

    if not meetings:
        if args.waybar:
            json.dump({"text": "No meeting 🏖️"}, sys.stdout)
        else:
            print("No meeting")
        return

    if args.open_meet_url:
        meeting = get_next_meeting(meetings, args.skip_all_day_meeting)
        if meeting:
            url = meeting.meet_url or meeting.calendar_url
            if args.google_domain:
                url = replace_domain_url(args.google_domain, url)
            open_url(url)
        return

    if args.waybar:
        rets_with_hyperlinks, cssclass = ret_events(meetings, args, hyperlink=True)
        if not rets_with_hyperlinks:
            ret = {"text": "No meeting 🏖️"}
        else:
            rets_no_hyperlinks, _ = ret_events(meetings, args, hyperlink=False)
            if args.waybar_show_all_day_meeting:
                coming_up_next = rets_no_hyperlinks[0]
            else:
                coming_up_next = get_next_non_all_day_meeting(
                    meetings, rets_no_hyperlinks, int(args.all_day_meeting_hours)
                )
                if not coming_up_next:  # only all days meeting
                    coming_up_next = rets_no_hyperlinks[0]
            ret = {
                "text": ellipsis(coming_up_next, args.max_title_length),
                "tooltip": bulletize(rets_with_hyperlinks),
            }
            if cssclass:
                ret["class"] = cssclass
        json.dump(ret, sys.stdout)
    else:
        rets, _ = ret_events(meetings, args, hyperlink=True)
        if not rets:
            debug(
                "No meeting has been detected perhaps use --calendar to specify another calendar if you don't have any in the default calendar",
                args,
            )
            print("No meeting")
        else:
            print(bulletize(rets))


if __name__ == "__main__":
    main()
