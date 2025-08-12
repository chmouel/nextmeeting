from __future__ import annotations

import argparse
import asyncio
import json
import os
from typing import Any, Dict

from .protocol import Request, Response


DEFAULT_SOCKET = os.path.expanduser("~/.cache/nextmeeting/socket")


async def _rpc_call(socket_path: str, method: str, params: Dict[str, Any]) -> Response:
    reader, writer = await asyncio.open_unix_connection(socket_path)
    try:
        req = Request(id="1", method=method, params=params)
        writer.write(req.to_json_line())
        await writer.drain()
        line = await reader.readline()
        if not line:
            raise RuntimeError("no response from server")
        return Response.from_json_line(line)
    finally:
        writer.close()
        await writer.wait_closed()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="nextmeeting", description="Nextmeeting client CLI"
    )
    parser.add_argument(
        "command",
        choices=[
            "ping",
            "version",
            "get-next",
            "list",
            "watch",
            "waybar",
            "polybar",
            "open",
            "copy-url",
            "copy-id",
            "copy-passcode",
            "open-calendar-day",
            "snooze",
            "server",
        ],
        help="command",
    )
    parser.add_argument(
        "--socket-path", default=DEFAULT_SOCKET, help="UNIX socket path"
    )
    parser.add_argument("--minutes", type=int, default=0, help="minutes (for snooze)")
    parser.add_argument(
        "--limit", type=int, default=0, help="limit number of items for list"
    )
    parser.add_argument(
        "--only-with-link", action="store_true", help="filter: only meetings with link"
    )
    parser.add_argument(
        "--within-mins", type=int, default=None, help="filter: within minutes from now"
    )
    parser.add_argument("--today-only", action="store_true", help="filter: today only")
    parser.add_argument(
        "--skip-all-day-meeting", action="store_true", help="filter: skip all-day"
    )
    parser.add_argument(
        "--include-title",
        action="append",
        default=[],
        help="include title substring (repeatable)",
    )
    parser.add_argument(
        "--exclude-title",
        action="append",
        default=[],
        help="exclude title substring (repeatable)",
    )
    parser.add_argument(
        "--include-calendar",
        action="append",
        default=[],
        help="include calendar substring (repeatable)",
    )
    parser.add_argument(
        "--exclude-calendar",
        action="append",
        default=[],
        help="exclude calendar substring (repeatable)",
    )
    parser.add_argument(
        "--work-hours", default=None, help="HH:MM-HH:MM working hours filter"
    )
    # waybar formatting options
    parser.add_argument("--privacy", action="store_true", help="hide actual titles")
    parser.add_argument(
        "--privacy-title", default="Busy", help="replacement title when privacy enabled"
    )
    parser.add_argument(
        "--max-title-length", type=int, default=50, help="truncate titles to N chars"
    )
    parser.add_argument(
        "--tooltip-limit", type=int, default=3, help="number of lines in tooltip"
    )
    parser.add_argument(
        "--time-format", choices=["24h", "12h"], default="24h", help="time format"
    )
    parser.add_argument(
        "--format", dest="format_str", default=None, help="custom text template"
    )
    parser.add_argument(
        "--tooltip-format",
        dest="tooltip_format",
        default=None,
        help="custom tooltip template",
    )
    # polybar coloring
    parser.add_argument(
        "--notify-min", type=int, default=5, help="minutes threshold to color"
    )
    parser.add_argument(
        "--notify-min-color", default="#FF5733", help="background color when soon"
    )
    parser.add_argument(
        "--notify-min-color-foreground",
        default="#F4F1DE",
        help="foreground color when soon",
    )
    parser.add_argument(
        "--open-with",
        dest="open_with",
        default=None,
        help="program to open links (for open)",
    )
    parser.add_argument(
        "--topics",
        default="next,notification,morning_agenda",
        help="comma-separated topics for watch (next,notification,morning_agenda)",
    )
    args, extra = parser.parse_known_args(argv)

    if args.command == "server":
        # Delegate to server daemon main
        from .server import main as server_main

        return server_main(extra)

    if args.command == "watch":
        topics = [t.strip() for t in str(args.topics).split(",") if t.strip()]
        return asyncio.run(_watch(args.socket_path, topics))
    if args.command == "waybar":
        return asyncio.run(
            _waybar(
                args.socket_path,
                privacy=args.privacy,
                privacy_title=args.privacy_title,
                max_title_length=args.max_title_length,
                tooltip_limit=args.tooltip_limit,
                time_format=args.time_format,
                notify_min=args.notify_min,
                format_str=args.format_str,
                tooltip_format=args.tooltip_format,
            )
        )
    if args.command == "polybar":
        return asyncio.run(
            _polybar(
                args.socket_path,
                privacy=args.privacy,
                privacy_title=args.privacy_title,
                max_title_length=args.max_title_length,
                notify_min=args.notify_min,
                notify_min_color=args.notify_min_color,
                notify_min_color_fg=args.notify_min_color_foreground,
                time_format=args.time_format,
                format_str=args.format_str,
            )
        )
    if args.command == "open":
        return asyncio.run(
            _open_meet(args.socket_path, getattr(args, "open_with", None))
        )
    if args.command in ("copy-url", "copy-id", "copy-passcode"):
        return asyncio.run(_copy_details(args.socket_path, args.command))
    if args.command == "open-calendar-day":
        return asyncio.run(_open_calendar_day(args.socket_path))
    if args.command == "snooze":
        return asyncio.run(_snooze(args.socket_path, args.minutes))

    async def run() -> int:
        params: Dict[str, Any] = {}
        if args.command in ("list", "get-next"):
            if args.command == "list":
                params["limit"] = args.limit
            params.update(
                {
                    "only_with_link": args.only_with_link,
                    "within_mins": args.within_mins,
                    "today_only": args.today_only,
                    "skip_all_day_meeting": args.skip_all_day_meeting,
                    "include_title": args.include_title,
                    "exclude_title": args.exclude_title,
                    "include_calendar": args.include_calendar,
                    "exclude_calendar": args.exclude_calendar,
                    "work_hours": args.work_hours,
                }
            )
        resp = await _rpc_call(
            args.socket_path, _map_cmd_to_method(args.command), params
        )
        if resp.error:
            print(
                json.dumps(
                    {"error": {"code": resp.error.code, "message": resp.error.message}},
                    indent=2,
                )
            )
            return 1
        print(json.dumps(resp.result, indent=2))
        return 0

    return asyncio.run(run())


def _map_cmd_to_method(cmd: str) -> str:
    return {
        "ping": "ping",
        "version": "version",
        "get-next": "get_next",
        "list": "list",
    }[cmd]


async def _watch(socket_path: str, topics: list[str]) -> int:
    reader, writer = await asyncio.open_unix_connection(socket_path)
    try:
        # send subscribe
        req = Request(id="sub1", method="subscribe", params={"topics": topics})
        writer.write(req.to_json_line())
        await writer.drain()
        # read ack
        _ = await reader.readline()
        # ignore parsing result here
        while True:
            line = await reader.readline()
            if not line:
                break
            try:
                obj = json.loads(line.decode("utf-8"))
            except Exception:
                continue
            if isinstance(obj, dict) and obj.get("event"):
                print(json.dumps(obj, indent=2))
    finally:
        writer.close()
        await writer.wait_closed()
    return 0


async def _snooze(socket_path: str, minutes: int) -> int:
    resp = await _rpc_call(socket_path, "snooze", {"minutes": minutes})
    if resp.error:
        print(
            json.dumps(
                {"error": {"code": resp.error.code, "message": resp.error.message}}
            )
        )
        return 1
    print(json.dumps(resp.result))
    return 0


async def _waybar(
    socket_path: str,
    *,
    privacy: bool,
    privacy_title: str,
    max_title_length: int,
    tooltip_limit: int,
    time_format: str,
    notify_min: int,
    format_str: str | None,
    tooltip_format: str | None,
) -> int:
    from .formatting import format_waybar

    # Fetch next and a small list for tooltip
    next_resp = await _rpc_call(
        socket_path,
        "get_next",
        {
            "only_with_link": False,
            "within_mins": None,
            "today_only": False,
            "skip_all_day_meeting": False,
        },
    )
    list_resp = await _rpc_call(socket_path, "list", {"limit": tooltip_limit})
    if next_resp.error or list_resp.error:
        print(json.dumps({"text": "", "tooltip": "", "class": "error"}))
        return 1
    payload = format_waybar(
        next_resp.result,
        list_resp.result or [],
        privacy=privacy,
        privacy_title=privacy_title,
        max_title_length=max_title_length,
        tooltip_limit=tooltip_limit,
        notify_min=notify_min,
        time_format=time_format,
        format_str=format_str,
        tooltip_format=tooltip_format,
    )
    print(json.dumps(payload))
    return 0


async def _polybar(
    socket_path: str,
    *,
    privacy: bool,
    privacy_title: str,
    max_title_length: int,
    notify_min: int,
    notify_min_color: str,
    notify_min_color_fg: str,
    time_format: str,
    format_str: str | None,
) -> int:
    from .formatting import format_polybar

    resp = await _rpc_call(
        socket_path,
        "get_next",
        {
            "only_with_link": False,
            "within_mins": None,
            "today_only": False,
            "skip_all_day_meeting": False,
        },
    )
    if resp.error:
        print("")
        return 1
    text = format_polybar(
        resp.result,
        privacy=privacy,
        privacy_title=privacy_title,
        max_title_length=max_title_length,
        notify_min=notify_min,
        notify_min_color=notify_min_color,
        notify_min_color_fg=notify_min_color_fg,
        time_format=time_format,
        format_str=format_str,
    )
    print(text)
    return 0


async def _open_meet(socket_path: str, open_with: str | None) -> int:
    import webbrowser
    import subprocess
    import shlex

    resp = await _rpc_call(
        socket_path,
        "get_next",
        {
            "only_with_link": False,
            "within_mins": None,
            "today_only": False,
            "skip_all_day_meeting": False,
        },
    )
    if resp.error or not resp.result:
        return 1
    url = resp.result.get("meet_url") or resp.result.get("calendar_url")
    if not url:
        return 1
    if open_with:
        try:
            subprocess.Popen(shlex.split(open_with) + [url])  # noqa: S603
            return 0
        except Exception:
            pass
    webbrowser.open(url)
    return 0


async def _copy_details(socket_path: str, which: str) -> int:
    from .core import extract_meeting_details

    resp = await _rpc_call(
        socket_path,
        "get_next",
        {
            "only_with_link": False,
            "within_mins": None,
            "today_only": False,
            "skip_all_day_meeting": False,
        },
    )
    if resp.error or not resp.result:
        return 1
    url = resp.result.get("meet_url") or resp.result.get("calendar_url")
    if not url:
        return 1
    details = extract_meeting_details(url)
    text = None
    if which == "copy-url":
        text = details.get("url") or url
    elif which == "copy-id":
        text = details.get("meeting_id")
    elif which == "copy-passcode":
        text = details.get("passcode")
    if not text:
        return 1
    if _copy_to_clipboard(str(text)):
        return 0
    print(text)
    return 0


def _copy_to_clipboard(text: str) -> bool:
    import shutil
    import subprocess

    try:
        if shutil.which("wl-copy"):
            subprocess.run(["wl-copy"], input=text.encode(), check=True)
            return True
        if shutil.which("xclip"):
            subprocess.run(
                ["xclip", "-selection", "clipboard"], input=text.encode(), check=True
            )
            return True
        if shutil.which("pbcopy"):
            subprocess.run(["pbcopy"], input=text.encode(), check=True)
            return True
    except Exception:
        return False
    return False


async def _open_calendar_day(socket_path: str) -> int:
    import webbrowser

    resp = await _rpc_call(
        socket_path,
        "get_next",
        {
            "only_with_link": False,
            "within_mins": None,
            "today_only": False,
            "skip_all_day_meeting": False,
        },
    )
    if resp.error or not resp.result:
        return 1
    url = resp.result.get("calendar_url")
    if not url:
        return 1
    webbrowser.open(url)
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
