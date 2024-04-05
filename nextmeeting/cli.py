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
# notificaiton via notify-send 5 minutes before meeting
# title elipsis
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
import typing
import webbrowser

import dateutil.parser as dtparse
import dateutil.relativedelta as dtrel

REG_TSV = re.compile(
    r"(?P<startdate>(\d{4})-(\d{2})-(\d{2}))\s*?(?P<starthour>(\d{2}:\d{2}))\s*(?P<enddate>(\d{4})-(\d{2})-(\d{2}))\s*?(?P<endhour>(\d{2}:\d{2}))\s*(?P<calendar_url>(https://\S+))\s*(?P<meet_url>(https://\S*)?)\s*(?P<title>.*)$"
)
DEFAULT_CALENDAR = os.environ.get("GCALCLI_DEFAULT_CALENDAR", "Work")
GCALCLI_CMDLINE = f"gcalcli --nocolor --calendar={DEFAULT_CALENDAR} agenda today --nodeclined  --details=end --details=url --tsv "
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


def elipsis(string: str, length: int) -> str:
    # remove all html elements first from it
    hstring = re.sub(r"<[^>]*>", "", string)
    if len(hstring) > length:
        return string[: length - 3] + "..."
    return string


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
        s += " at %02dh%02d" % (
            date.hour,
            date.minute,
        )  # pylint: disable=consider-using-f-string
    elif deltad.hours != 0:
        s = date.strftime("%HH%M")
    else:
        if (
            deltad.minutes <= NOTIFY_MIN_BEFORE_EVENTS
            and args.notify_min_color
            and args.waybar
        ):
            number = f"""<span background="{args.notify_min_color}" color="{args.notify_min_color_foreground}">{deltad.minutes}</span>"""
        else:
            number = f"{deltad.minutes}"

        s = f"In {number} minutes"
    return s


def make_hyperlink(uri: str, label: None | str = None):
    if label is None:
        label = uri
    parameters = ""

    # OSC 8 ; params ; URI ST <name> OSC 8 ;; ST
    escape_mask = "\033]8;{};{}\033\\{}\033]8;;\033\\"
    return escape_mask.format(parameters, uri, label)


def process_file(fp) -> list[re.Match]:
    ret = []
    for _line in fp.readlines():  # type: ignore
        try:
            line = str(_line.strip(), "utf-8")
        except TypeError:
            line = _line.strip()
        match = REG_TSV.match(line)
        enddate = dtparse.parse(
            f"{match.group('enddate')} {match.group('endhour')}"  # type: ignore
        )
        if datetime.datetime.now() > enddate:
            continue

        if not match:
            continue
        ret.append(match)
    return ret


def gcalcli_output(args: argparse.Namespace) -> list[re.Match]:
    # TODO: do unittests with this
    # with open("/tmp/debug") as f:
    #     return process_file(f)

    with subprocess.Popen(
        args.gcalcli_cmdline, shell=True, stdout=subprocess.PIPE
    ) as cmd:
        return process_file(cmd.stdout)


def ret_events(
    lines: list[re.Match], args: argparse.Namespace, hyperlink: bool = False
) -> typing.Tuple[list[str], str]:
    ret = []
    cssclass = ""
    for match in lines:
        title = match.group("title")
        if args.waybar:
            title = html.escape(title)
        if hyperlink and match.group("meet_url"):
            title = make_hyperlink(match.group("meet_url"), title)
        startdate = dtparse.parse(
            f"{match.group('startdate')} {match.group('starthour')}"
        )
        enddate = dtparse.parse(f"{match.group('enddate')} {match.group('endhour')}")
        if (
            args.skip_all_day_meeting
            and dtrel.relativedelta(enddate, startdate).days >= 1
        ):
            continue
        if datetime.datetime.now() > startdate:
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
                thetime = make_hyperlink(match.group("calendar_url"), thetime)
            ret.append(f"{thetime} - {title}")
        else:
            timeuntilstarting = dtrel.relativedelta(
                startdate + datetime.timedelta(minutes=1), datetime.datetime.now()
            )

            url = match.group("calendar_url")
            if args.google_domain:
                url = replace_domain_url(args.google_domain, url)
            if (
                not timeuntilstarting.days
                and not timeuntilstarting.hours
                and timeuntilstarting.minutes <= args.notify_min_before_events
            ):
                cssclass = "soon"
                notify(title, startdate, enddate, args)

            thetime = pretty_date(timeuntilstarting, startdate, args)
            if hyperlink:
                thetime = f"{thetime: <17}"
                thetime = make_hyperlink(
                    replace_domain_url(args.google_domain, match.group("calendar_url")),
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
            cached = json.load(f)
            if uuid in cached:
                notified = True
    if notified:
        return
    cached.append(uuid)
    with cache_path.open("w") as f:
        if cached >= MAX_CACHED_ENTRIES:
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
        "--gcalcli-cmdline", help="gcalcli command line", default=GCALCLI_CMDLINE
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
        help="how long is an all day meeting in hours, (default: %s)"
        % (ALL_DAYS_MEETING_HOURS),
    )

    parser.add_argument(
        "--notify-expiry",
        type=int,
        help="notifcation expiration in minutes (0 no expiry, -1 show notification until the meeting sart)",
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
    return parser.parse_args()


def replace_domain_url(domain, url: str) -> str:
    return url.replace(
        GOOGLE_CALENDAR_PUBLIC_URL,
        f"calendar.google.com/a/{domain}",
    )


def bulletize(rets: list[str]) -> str:
    return "• " + "\n• ".join(rets)


def get_next_non_all_day_meeting(
    matches: list[re.Match], rets: list[str], all_day_meeting_hours: int
) -> None | str:
    for m in matches:
        start_date = dtparse.parse("%s %s" % (m["startdate"], m["starthour"]))
        end_date = dtparse.parse("%s %s" % (m["enddate"], m["endhour"]))
        if end_date > (start_date + datetime.timedelta(hours=all_day_meeting_hours)):
            continue
        return rets[matches.index(m)]
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
    # calendar, maybe speicfy a /u/number/ for multi accounts ?
    if url:
        open_url(url)
    sys.exit(0)


def main():
    args = parse_args()
    args.cache_dir.mkdir(parents=True, exist_ok=True)
    matches = gcalcli_output(args)
    rets, cssclass = ret_events(matches, args)
    if args.open_meet_url:
        open_meet_url(rets, matches, args)
        return

    elif args.waybar:
        if not rets:
            ret = {"text": "No meeting 🏖️"}
        else:
            if args.waybar_show_all_day_meeting:
                coming_up_next = rets[0]
            else:
                coming_up_next = get_next_non_all_day_meeting(
                    matches, rets, int(args.all_day_meeting_hours)
                )
                if not coming_up_next:  # only all days meeting
                    coming_up_next = rets[0]
            ret = {
                "text": elipsis(coming_up_next, args.max_title_length),
                "tooltip": bulletize(rets),
            }
            if cssclass:
                ret["class"] = cssclass
        json.dump(ret, sys.stdout)
    else:
        rets, _ = ret_events(matches, args, hyperlink=True)
        print(bulletize(rets))


if __name__ == "__main__":
    main()
