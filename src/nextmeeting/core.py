from __future__ import annotations

import dataclasses
import datetime as dt
import re
import subprocess
from dataclasses import dataclass
from typing import Iterable, List, Optional, Sequence

import dateutil.parser as dtparse


REG_TSV = re.compile(
    r"(?P<startdate>(\d{4})-(\d{2})-(\d{2}))\s*?"
    r"(?P<starthour>(\d{2}:\d{2}))\s*"
    r"(?P<enddate>(\d{4})-(\d{2})-(\d{2}))\s*?"
    r"(?P<endhour>(\d{2}:\d{2}))\s*"
    r"(?P<calendar_url>(https://\S+))\s*"
    r"(?P<meet_url>(https://\S*)?)?\s*"
    r"(?P<title>.*)$"
)

GCALCLI_CMD = (
    "gcalcli --nocolor agenda today --nodeclined --details=end --details=url --tsv"
)


@dataclass
class Meeting:
    title: str
    start_time: dt.datetime
    end_time: dt.datetime
    calendar_url: str
    meet_url: Optional[str] = None

    @property
    def is_ongoing(self) -> bool:
        now = dt.datetime.now()
        return self.start_time <= now <= self.end_time

    @property
    def is_all_day(self) -> bool:
        return (self.end_time - self.start_time) >= dt.timedelta(hours=24)


def parse_tsv_line(line: str) -> Optional[Meeting]:
    m = REG_TSV.match(line.rstrip("\n"))
    if not m:
        return None
    start_time = dtparse.parse(f"{m['startdate']} {m['starthour']}")
    end_time = dtparse.parse(f"{m['enddate']} {m['endhour']}")
    meet_url = m["meet_url"] or None
    return Meeting(
        title=m["title"],
        start_time=start_time,
        end_time=end_time,
        calendar_url=m["calendar_url"],
        meet_url=meet_url,
    )


def parse_tsv(tsv: str) -> List[Meeting]:
    meetings: List[Meeting] = []
    for line in tsv.splitlines():
        mm = parse_tsv_line(line)
        if mm:
            meetings.append(mm)
    meetings.sort(key=lambda x: x.start_time)
    return meetings


@dataclass
class FilterOptions:
    only_with_link: bool = False
    within_mins: Optional[int] = None
    today_only: bool = False
    skip_all_day_meeting: bool = False
    include_title: Sequence[str] = dataclasses.field(default_factory=list)
    exclude_title: Sequence[str] = dataclasses.field(default_factory=list)
    include_calendar: Sequence[str] = dataclasses.field(default_factory=list)
    exclude_calendar: Sequence[str] = dataclasses.field(default_factory=list)
    work_hours: Optional[str] = None  # "HH:MM-HH:MM"


def _within_work_hours(m: Meeting, spec: str) -> bool:
    try:
        beg, end = spec.split("-", 1)
        bh, bm = [int(x) for x in beg.split(":", 1)]
        eh, em = [int(x) for x in end.split(":", 1)]
    except Exception:  # noqa: BLE001
        return True
    start = m.start_time
    day_begin = start.replace(hour=bh, minute=bm, second=0, microsecond=0)
    day_end = start.replace(hour=eh, minute=em, second=0, microsecond=0)
    return (m.start_time >= day_begin) and (m.end_time <= day_end)


def apply_filters(meetings: Iterable[Meeting], opts: FilterOptions) -> List[Meeting]:
    now = dt.datetime.now()
    out: List[Meeting] = []
    for m in meetings:
        if opts.only_with_link and not m.meet_url:
            continue
        if opts.within_mins is not None:
            if (m.start_time - now) > dt.timedelta(minutes=opts.within_mins):
                continue
        if opts.today_only and m.start_time.date() != now.date():
            continue
        if opts.skip_all_day_meeting and m.is_all_day:
            continue
        title_lower = m.title.lower()
        if opts.include_title and not any(
            s.lower() in title_lower for s in opts.include_title
        ):
            continue
        if any(s.lower() in title_lower for s in opts.exclude_title):
            continue
        if opts.include_calendar and not any(
            s in m.calendar_url for s in opts.include_calendar
        ):
            continue
        if any(s in m.calendar_url for s in opts.exclude_calendar):
            continue
        if opts.work_hours and not _within_work_hours(m, opts.work_hours):
            continue
        out.append(m)
    out.sort(key=lambda x: x.start_time)
    return out


def compute_next(meetings: Sequence[Meeting]) -> Optional[Meeting]:
    now = dt.datetime.now()
    # prefer ongoing, else first upcoming
    ongoing = [m for m in meetings if m.is_ongoing]
    if ongoing:
        return sorted(ongoing, key=lambda m: m.end_time)[0]
    upcoming = [m for m in meetings if m.start_time >= now]
    return upcoming[0] if upcoming else None


def meeting_to_dict(m: Meeting) -> dict:
    return {
        "title": m.title,
        "start": m.start_time.isoformat(),
        "end": m.end_time.isoformat(),
        "calendar_url": m.calendar_url,
        "meet_url": m.meet_url,
        "is_all_day": m.is_all_day,
        "is_ongoing": m.is_ongoing,
    }


def dict_to_meeting(d: dict) -> Optional[Meeting]:
    try:
        return Meeting(
            title=str(d.get("title") or ""),
            start_time=dtparse.parse(str(d.get("start"))),
            end_time=dtparse.parse(str(d.get("end"))),
            calendar_url=str(d.get("calendar_url") or ""),
            meet_url=d.get("meet_url") or None,
        )
    except Exception:  # noqa: BLE001
        return None


def run_gcalcli(
    calendar: Optional[str] = None, extra_args: Optional[Sequence[str]] = None
) -> str:
    cmd = GCALCLI_CMD
    if calendar:
        cmd += f" --calendar {calendar}"
    if extra_args:
        cmd += " " + " ".join(extra_args)
    proc = subprocess.run(cmd, shell=True, check=False, capture_output=True, text=True)
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr.strip() or "gcalcli failed")
    return proc.stdout


# Meeting link sanitization and details extraction (minimal common cases)
def normalize_meet_url(url: Optional[str]) -> Optional[str]:
    if not url:
        return None
    try:
        u = url
        # Zoom: join?confno=123&pwd=xxx => j/123?pwd=xxx
        if "zoom.us" in u and "/join?" in u and "confno=" in u:
            import urllib.parse as _up

            parsed = _up.urlparse(u)
            qs = _up.parse_qs(parsed.query)
            confno = (qs.get("confno") or [None])[0]
            pwd = (qs.get("pwd") or [None])[0]
            if confno:
                base = f"https://{parsed.netloc}/j/{confno}"
                if pwd:
                    base += f"?pwd={pwd}"
                return base
        # Outlook SafeLink wrapper
        if "safelinks.protection.outlook.com" in u and "url=" in u:
            import urllib.parse as _up

            parsed = _up.urlparse(u)
            qs = _up.parse_qs(parsed.query)
            wrapped = (qs.get("url") or [None])[0]
            if wrapped:
                return normalize_meet_url(_up.unquote(wrapped)) or wrapped
        return url
    except Exception:
        return url


def extract_meeting_details(url: Optional[str]) -> dict:
    url = normalize_meet_url(url)
    info = {"service": None, "meeting_id": None, "passcode": None, "url": url}
    if not url:
        return info
    try:
        import re as _re
        import urllib.parse as _up

        if "zoom.us" in url:
            info["service"] = "zoom"
            m = _re.search(r"/j/(\d+)", url)
            if not m and "confno=" in url:
                parsed = _up.urlparse(url)
                qs = _up.parse_qs(parsed.query)
                m = _re.match(r"^(\d+)$", (qs.get("confno") or [""])[0])
                if m:
                    info["meeting_id"] = m.group(1)
                    pwd = (qs.get("pwd") or [None])[0]
                    info["passcode"] = pwd
            if m and not info.get("meeting_id"):
                info["meeting_id"] = m.group(1)
            pm = _re.search(r"[?&]pwd=([^&#]+)", url)
            if pm:
                info["passcode"] = pm.group(1)
        elif "meet.google.com" in url:
            info["service"] = "google"
            m = _re.search(r"meet.google.com/([a-z0-9-]+)", url)
            if m:
                info["meeting_id"] = m.group(1)
        elif "teams.microsoft.com" in url or "teams.live.com" in url:
            info["service"] = "teams"
        elif "meet.jit.si" in url:
            info["service"] = "jitsi"
    except Exception:
        return info
    return info
