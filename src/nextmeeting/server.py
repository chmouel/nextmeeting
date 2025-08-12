from __future__ import annotations

import argparse
import asyncio
import json
import os
import pathlib
import signal
from typing import Any, Awaitable, Callable, Dict, List, Optional, Tuple

from . import __version__
from .core import (
    FilterOptions,
    apply_filters,
    compute_next,
    dict_to_meeting,
    meeting_to_dict,
    parse_tsv,
    run_gcalcli,
)
from .protocol import Error, Request, Response

DEFAULT_SOCKET = os.path.expanduser("~/.cache/nextmeeting/socket")


class RpcServer:
    def __init__(
        self,
        socket_path: str = DEFAULT_SOCKET,
        *,
        poll_interval: int = 60,
        calendar: str | None = None,
        fetch_func: Optional[Callable[[Optional[str]], str]] = None,
        enable_notify: bool = False,
        notify_min_before_events: int = 5,
        notify_offsets: Optional[List[int]] = None,
        notify_icon: Optional[str] = None,
        notify_expiry: int = 0,
    ):
        self.socket_path = socket_path
        self._server: Optional[asyncio.AbstractServer] = None
        self._handlers: dict[str, Callable[[dict[str, Any]], Awaitable[Any]]]
        self._handlers = {
            "ping": self._rpc_ping,
            "version": self._rpc_version,
            "get_next": self._rpc_get_next,
            "list": self._rpc_list,
            "subscribe": self._rpc_subscribe,
            "snooze": self._rpc_snooze,
        }
        self._poll_interval = poll_interval
        self._calendar = calendar
        self._cache: list[dict] = []
        self._poll_task: asyncio.Task | None = None
        self._fetch = fetch_func or (lambda cal: run_gcalcli(cal, None))
        self._subscribers: List[
            Tuple[asyncio.StreamWriter, List[str]]
        ] = []  # (writer, topics)
        self._last_next_id: Optional[str] = None
        # notifications
        self._enable_notify = enable_notify
        self._notify_min = int(notify_min_before_events)
        self._notify_offsets = sorted(list(notify_offsets or []))
        self._notify_icon = notify_icon or ""
        self._notify_expiry = int(notify_expiry)
        self._notified: Dict[str, List[int]] = {}
        self._notify_urgency = notify_urgency
        self._notify_critical_within = notify_critical_within
        self._snoozed_until: Optional[float] = None
        self._agenda_task: asyncio.Task | None = None
        self._morning_agenda = morning_agenda

    async def _rpc_ping(self, params: dict[str, Any]) -> Any:  # noqa: ARG002
        return "pong"

    async def _rpc_version(self, params: dict[str, Any]) -> Any:  # noqa: ARG002
        return {"version": __version__}

    async def _rpc_get_next(self, params: dict[str, Any]) -> Any:  # noqa: ARG002
        await self._ensure_warm()
        opts = FilterOptions(
            only_with_link=bool(params.get("only_with_link", False)),
            within_mins=int(params["within_mins"])
            if params.get("within_mins") is not None
            else None,
            today_only=bool(params.get("today_only", False)),
            skip_all_day_meeting=bool(params.get("skip_all_day_meeting", False)),
            include_title=params.get("include_title") or [],
            exclude_title=params.get("exclude_title") or [],
            include_calendar=params.get("include_calendar") or [],
            exclude_calendar=params.get("exclude_calendar") or [],
            work_hours=params.get("work_hours"),
        )
        mlist = [dict_to_meeting(d) for d in self._cache]
        mlist = [m for m in mlist if m is not None]
        filtered = apply_filters(mlist, opts)
        nxt = compute_next(filtered)
        return meeting_to_dict(nxt) if nxt else None

    async def _rpc_list(self, params: dict[str, Any]) -> Any:  # noqa: ARG002
        await self._ensure_warm()
        limit = int(params.get("limit", 0) or 0)
        opts = FilterOptions(
            only_with_link=bool(params.get("only_with_link", False)),
            within_mins=int(params["within_mins"])
            if params.get("within_mins") is not None
            else None,
            today_only=bool(params.get("today_only", False)),
            skip_all_day_meeting=bool(params.get("skip_all_day_meeting", False)),
            include_title=params.get("include_title") or [],
            exclude_title=params.get("exclude_title") or [],
            include_calendar=params.get("include_calendar") or [],
            exclude_calendar=params.get("exclude_calendar") or [],
            work_hours=params.get("work_hours"),
        )
        mlist = [dict_to_meeting(d) for d in self._cache]
        mlist = [m for m in mlist if m is not None]
        filtered = apply_filters(mlist, opts)
        data = [meeting_to_dict(m) for m in filtered]
        if limit > 0:
            data = data[:limit]
        return data

    async def _rpc_subscribe(self, params: dict[str, Any]) -> Any:
        # Handled in connection handler; here just ack
        return {"ok": True}

    async def _rpc_snooze(self, params: dict[str, Any]) -> Any:
        minutes = int(params.get("minutes", 0) or 0)
        if minutes <= 0:
            self._snoozed_until = None
            return {"snoozed": False}
        self._snoozed_until = asyncio.get_running_loop().time() + minutes * 60
        return {"snoozed": True, "until_monotonic": self._snoozed_until}

    async def _dispatch(self, req: Request) -> Response:
        method = self._handlers.get(req.method)
        if method is None:
            return Response(
                id=req.id,
                error=Error(code=404, message=f"unknown method: {req.method}"),
            )
        try:
            result = await method(req.params)
            return Response(id=req.id, result=result)
        except Exception as exc:  # noqa: BLE001
            return Response(id=req.id, error=Error(code=500, message=str(exc)))

    async def _handle_client(
        self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        try:
            while not reader.at_eof():
                line = await reader.readline()
                if not line:
                    break
                req = Request.from_json_line(line)
                if req.method == "subscribe":
                    topics = req.params.get("topics") or ["next"]
                    # Ack subscribe
                    ack = Response(id=req.id, result={"subscribed": topics})
                    writer.write(ack.to_json_line())
                    await writer.drain()
                    self._subscribers.append((writer, topics))
                    # Do not break; keep processing for potential unsubscribe
                    continue
                resp = await self._dispatch(req)
                writer.write(resp.to_json_line())
                await writer.drain()
        finally:
            # Remove from subscribers if present
            self._subscribers = [
                (w, t) for (w, t) in self._subscribers if w is not writer
            ]
            writer.close()
            await writer.wait_closed()

    async def start(self) -> None:
        # Ensure parent directory exists and socket not lingering
        spath = pathlib.Path(self.socket_path)
        spath.parent.mkdir(parents=True, exist_ok=True)
        try:
            spath.unlink()
        except FileNotFoundError:
            pass
        self._server = await asyncio.start_unix_server(
            self._handle_client, path=self.socket_path
        )
        # start poller
        self._poll_task = asyncio.create_task(self._poll_loop())
        if self._morning_agenda:
            self._agenda_task = asyncio.create_task(self._agenda_loop())

    async def serve_forever(self) -> None:
        if self._server is None:
            await self.start()
        assert self._server is not None
        async with self._server:
            await self._server.serve_forever()

    async def close(self) -> None:
        if self._server is not None:
            self._server.close()
            await self._server.wait_closed()
        if self._poll_task is not None:
            self._poll_task.cancel()
            try:
                await self._poll_task
            except Exception:  # noqa: BLE001
                pass
        if self._agenda_task is not None:
            self._agenda_task.cancel()
            try:
                await self._agenda_task
            except Exception:
                pass
        try:
            os.unlink(self.socket_path)
        except FileNotFoundError:
            pass

    async def _poll_loop(self) -> None:
        # periodic fetch and cache
        while True:
            try:
                txt = await asyncio.to_thread(self._fetch, self._calendar)
                meetings = parse_tsv(txt)
                self._cache = [meeting_to_dict(m) for m in meetings]
                await self._post_update()
            except Exception:
                # Keep previous cache on failure
                pass
            await asyncio.sleep(self._poll_interval)

    async def _ensure_warm(self) -> None:
        # If cache empty, trigger immediate refresh once
        if not self._cache:
            try:
                txt = await asyncio.to_thread(self._fetch, self._calendar)
                meetings = parse_tsv(txt)
                self._cache = [meeting_to_dict(m) for m in meetings]
                await self._post_update()
            except Exception:
                self._cache = []

    async def _post_update(self) -> None:
        # Determine next meeting id and broadcast changes; schedule notifications
        from datetime import datetime

        from dateutil.parser import isoparse  # type: ignore

        now = datetime.now()
        # Compute next
        next_item = None
        parsed = []
        for d in self._cache:
            try:
                s = isoparse(d["start"])  # type: ignore
                e = isoparse(d["end"])  # type: ignore
            except Exception:  # noqa: BLE001
                continue
            parsed.append((s, e, d))
        parsed.sort(key=lambda t: t[0])
        for s, e, d in parsed:
            if s <= now <= e:
                next_item = d
                break
        if next_item is None:
            for s, _e, d in parsed:
                if s >= now:
                    next_item = d
                    break
        # Notify and broadcast
        nid = self._meeting_id(next_item) if next_item else None
        if nid != self._last_next_id:
            self._last_next_id = nid
            await self._broadcast({"event": "next_changed", "data": next_item})
        # Notifications: for any upcoming meeting, if enable_notify
        if self._enable_notify:
            for s, _e, d in parsed:
                mins = int((s - now).total_seconds() // 60)
                if mins < 0:
                    continue
                target_marks = set([self._notify_min] + self._notify_offsets)
                if mins in target_marks:
                    self._maybe_notify(self._meeting_id(d), d, mins)

    def _meeting_id(self, data: Optional[dict]) -> Optional[str]:
        if not data:
            return None
        return f"{data.get('title', '')}|{data.get('start', '')}|{data.get('end', '')}"

    async def _broadcast(self, payload: dict) -> None:
        # Send as a standalone event line (not a Response)
        dead: List[Tuple[asyncio.StreamWriter, List[str]]] = []
        line = (json.dumps(payload) + "\n").encode("utf-8")
        evt = payload.get("event") if isinstance(payload, dict) else None
        for w, topics in list(self._subscribers):
            if evt and topics and evt not in topics:
                continue
            try:
                w.write(line)
                await w.drain()
            except Exception:
                dead.append((w, topics))
        if dead:
            self._subscribers = [
                (w, t) for (w, t) in self._subscribers if (w, t) not in dead
            ]

    def _maybe_notify(self, mid: Optional[str], data: dict, mark: int) -> None:
        if not mid:
            return
        marks = self._notified.setdefault(mid, [])
        if mark in marks:
            return
        # Snooze in effect
        if (
            self._snoozed_until is not None
            and asyncio.get_running_loop().time() < self._snoozed_until
        ):
            return
        # Fire notify-send
        try:
            import shutil
            import subprocess

            notify = shutil.which("notify-send")
            if not notify:
                return
            args = [notify, data.get("title") or "Meeting soon"]
            body = data.get("meet_url") or data.get("calendar_url") or ""
            if body:
                args.extend(["--", body])
            if self._notify_icon:
                args.extend(["-i", self._notify_icon])
            if self._notify_expiry:
                args.extend(["-t", str(self._notify_expiry)])
            # urgency (with optional escalation)
            urgency = self._notify_urgency
            if self._notify_critical_within is not None:
                from datetime import datetime

                from dateutil.parser import isoparse  # type: ignore

                try:
                    s = isoparse(data.get("start"))
                    mins = int((s - datetime.now()).total_seconds() // 60)
                    if 0 <= mins <= int(self._notify_critical_within):
                        urgency = "critical"
                except Exception:
                    pass
            args.extend(["-u", urgency])
            subprocess.Popen(args)  # noqa: S603
            marks.append(mark)
            # Broadcast a notification event as well
            asyncio.create_task(
                self._broadcast(
                    {"event": "notification", "data": {**data, "at_min": mark}}
                )
            )
        except Exception:
            # ignore notification failures
            pass

    async def _agenda_loop(self) -> None:
        # Fire a daily morning agenda notification and event
        while True:
            try:
                from datetime import datetime, timedelta

                hh, mm = (self._morning_agenda or "09:00").split(":", 1)
                target = datetime.now().replace(
                    hour=int(hh), minute=int(mm), second=0, microsecond=0
                )
                now = datetime.now()
                if target <= now:
                    target = target + timedelta(days=1)
                await asyncio.sleep((target - now).total_seconds())
                await self._ensure_warm()
                summary = self._build_today_summary()
                self._notify_simple("Morning agenda", summary)
                await self._broadcast(
                    {"event": "morning_agenda", "data": {"text": summary}}
                )
            except asyncio.CancelledError:
                break
            except Exception:
                await asyncio.sleep(60)

    def _build_today_summary(self) -> str:
        from datetime import datetime

        from dateutil.parser import isoparse  # type: ignore

        lines: List[str] = []
        today = datetime.now().date()
        for d in self._cache:
            try:
                sdt = isoparse(d.get("start"))
                if sdt.date() == today:
                    s = sdt.strftime("%H:%M")
                    lines.append(f"{s} {d.get('title')}")
            except Exception:
                continue
        return "\n".join(lines[:10]) if lines else "No meetings today"

    def _notify_simple(self, title: str, body: str) -> None:
        try:
            import shutil
            import subprocess

            notify = shutil.which("notify-send")
            if not notify:
                return
            args = [notify, title, body]
            if self._notify_icon:
                args.extend(["-i", self._notify_icon])
            if self._notify_expiry:
                args.extend(["-t", str(self._notify_expiry)])
            args.extend(["-u", self._notify_urgency])
            subprocess.Popen(args)  # noqa: S603
        except Exception:
            pass


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="nextmeetingd", description="Nextmeeting server daemon"
    )
    parser.add_argument(
        "--socket-path", default=DEFAULT_SOCKET, help="UNIX socket path"
    )
    parser.add_argument(
        "--poll-interval", type=int, default=60, help="Polling interval in seconds"
    )
    parser.add_argument(
        "--calendar",
        default=os.environ.get("GCALCLI_DEFAULT_CALENDAR"),
        help="gcalcli calendar",
    )
    parser.add_argument(
        "--enable-notify", action="store_true", help="Enable desktop notifications"
    )
    parser.add_argument(
        "--notify-min-before-events",
        type=int,
        default=5,
        help="Notify N minutes before meetings",
    )
    parser.add_argument(
        "--notify-offsets",
        default="",
        help="Comma-separated additional minutes for notifications (e.g., 10,2)",
    )
    parser.add_argument("--notify-icon", default="", help="Icon path for notifications")
    parser.add_argument(
        "--notify-expiry", type=int, default=0, help="Notification timeout (ms)"
    )
    parser.add_argument(
        "--notify-urgency", choices=["low", "normal", "critical"], default="normal"
    )
    parser.add_argument(
        "--notify-critical-within",
        type=int,
        default=None,
        help="Escalate to critical within N minutes of start",
    )
    parser.add_argument(
        "--morning-agenda", default=None, help="Send a daily agenda at HH:MM"
    )
    args = parser.parse_args(argv)

    offsets: List[int] = []
    if str(args.notify_offsets).strip():
        try:
            offsets = [int(x) for x in str(args.notify_offsets).split(",") if x.strip()]
        except Exception:
            offsets = []
    server = RpcServer(
        socket_path=args.socket_path,
        poll_interval=args.poll_interval,
        calendar=args.calendar,
        enable_notify=bool(args.enable_notify),
        notify_min_before_events=int(args.notify_min_before_events),
        notify_offsets=offsets,
        notify_icon=str(args.notify_icon or ""),
        notify_expiry=int(args.notify_expiry or 0),
        notify_urgency=str(args.notify_urgency),
        notify_critical_within=int(args.notify_critical_within)
        if args.notify_critical_within is not None
        else None,
        morning_agenda=str(args.morning_agenda) if args.morning_agenda else None,
    )

    async def run() -> int:
        loop = asyncio.get_running_loop()
        stop = asyncio.Event()

        def _signal_handler() -> None:
            stop.set()

        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, _signal_handler)

        await server.start()
        try:
            await stop.wait()
        finally:
            await server.close()
        return 0

    asyncio.run(run())
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
