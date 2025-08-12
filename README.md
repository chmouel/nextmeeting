# nextmeeting — local calendar server/client for bars and scripts

nextmeeting exposes your upcoming Google Calendar meetings via a local UNIX socket server and a thin CLI client. It integrates with Waybar/Polybar, provides JSON output, notifications, and filterable results, while avoiding shelling out to `gcalcli` on every bar tick.

## Highlights

- Server caches meetings from `gcalcli` (configurable calendar and poll interval).
- Client prints JSON or bar-friendly outputs (Waybar/Polybar) and can subscribe to events.
- Filters: title include/exclude, calendar include/exclude, work hours, within window, link-only, today-only, all-day skip.
- Notifications before meetings and event streaming (`next_changed`, `notification`).
- Snooze notifications and morning agenda summary.
- Privacy and title ellipsis for clean bar display.

## Install

Using uv:

```
uv run nextmeeting version
```

## Run

Start the daemon and then call the client:

- Start server:
  - `uv run nextmeeting server --socket-path ~/.cache/nextmeeting/socket --poll-interval 60 --calendar 'Your Calendar' --enable-notify`
- Check:
  - `uv run nextmeeting ping`
  - `uv run nextmeeting version`
- Get data:
  - `uv run nextmeeting list --limit 5`
  - `uv run nextmeeting get-next --within-mins 30 --only-with-link`
- Bars:
  - Waybar: `uv run nextmeeting waybar --tooltip-limit 3 --max-title-length 40 --privacy`
  - Polybar: `uv run nextmeeting polybar --notify-min 5`
  - Click to open next meet URL: `uv run nextmeeting open`
  - Snooze server notifications: `uv run nextmeeting snooze --minutes 15`
  - Open with a specific program: `uv run nextmeeting open --open-with "firefox -P Work"`

## Waybar

Waybar config snippet for the `custom` module:

```
"custom/agenda": {
  "format": "{}",
  "exec": "nextmeeting waybar --tooltip-limit 3 --max-title-length 40 --privacy",
  "return-type": "json",
  "interval": 55,
  "tooltip": true,
  "on-click": "nextmeeting open"
}
```

Options:

- `--privacy` and `--privacy-title`: hide or replace titles.
- `--max-title-length N`: truncate titles with ellipsis.
- `--tooltip-limit N`: limit tooltip lines.
- `--time-format 12h|24h`: absolute time rendering.

Class names for styling: `.current`, `.upcoming` (and you can style the “soon” threshold via Polybar or your own CSS).

### Styling (Waybar CSS)

Example CSS for the `custom/agenda` module:

```
#custom-agenda {
  color: #696969;
}
#custom-agenda.current {
  color: #88c0d0;
}
#custom-agenda.upcoming {
  color: #a3be8c;
}
#custom-agenda.soon {
  color: #eb4d4b;
  font-weight: 600;
}
```

## Polybar

Example module:

```
[module/agenda]
type = custom/script
exec = nextmeeting polybar --notify-min 5 --max-title-length 40
interval = 55
click-left = nextmeeting open
```

When within the `--notify-min` threshold, the remaining minutes are highlighted using Polybar formatting sequences.

## Custom Templates

For Waybar/Polybar output you can render custom text via Python-style templates:

- Waybar text: `--format "{when} • {title}"`
- Waybar tooltip lines: `--tooltip-format "{start_time:%H:%M}-{end_time:%H:%M} · {title}"`
- Polybar text: `--format "{when} — {title}"`

Available fields in templates:

- `when`: human-friendly time string (e.g., `13:30`, `Tomorrow at 09:00`, `15 min left`).
- `title`: meeting title.
- `start_time`, `end_time`: datetime objects (support format specifiers like `%H:%M`).
- `meet_url`, `calendar_url`: strings.
- `minutes_until`: integer minutes until start (capped at 0 minimum).
- `is_all_day`, `is_ongoing`: booleans.

## Filters

Apply to `get-next` and `list`:

- `--only-with-link`: only meetings with a meeting URL.
- `--within-mins N`: only within N minutes from now.
- `--today-only`: restrict to today’s meetings.
- `--skip-all-day-meeting`: ignore all-day events.
- `--include-title`, `--exclude-title`: match substrings (repeatable).
- `--include-calendar`, `--exclude-calendar`: match calendar URL substrings (repeatable).
- `--work-hours HH:MM-HH:MM`: keep meetings within working hours.

## Notifications and Events

Server options:

- `--enable-notify`: enable desktop notifications via `notify-send`.
- `--notify-min-before-events N`: base minute mark (default 5).
- `--notify-offsets 15,5`: additional minute marks.
- `--notify-icon PATH`, `--notify-expiry MS`.
- `--notify-urgency low|normal|critical`, `--notify-critical-within N` to escalate near start.
- `--morning-agenda HH:MM` to show a daily summary.

Events (subscribe via `uv run nextmeeting watch`):

- `next_changed`: emitted whenever the current/next meeting changes.
- `notification`: emitted when a notification fires (`{"event":"notification","data":{...}}`).
- `morning_agenda`: emitted when the daily agenda summary is sent.

Subscribe: `uv run nextmeeting watch` to stream events as JSON lines.
You can filter topics: `uv run nextmeeting watch --topics next,notification`.

## Systemd user unit

Example unit is provided at `packaging/systemd/nextmeetingd.service`:

```
mkdir -p ~/.config/systemd/user
cp packaging/systemd/nextmeetingd.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now nextmeetingd.service
```

Adjust `ExecStart` and flags to match your setup.

## Packaging Notes

### Nix Flake

Consume the package via the `packaging` subdir:

```
nextmeeting = {
  url = "github:chmouel/nextmeeting?dir=packaging";
  inputs.nixpkgs.follows = "nixpkgs";
};
```

Then reference it in your config (example Waybar):

```
let nextmeeting = lib.getExe inputs.nextmeeting.packages.${pkgs.system}.default;
in {
  "custom/agenda" = {
    format = "{}";
    exec = nextmeeting + " waybar --tooltip-limit 3 --max-title-length 40";
    return-type = "json";
    interval = 55;
    tooltip = true;
    on-click = nextmeeting + " open";
  };
}
```

### AUR

If you package for AUR, expose both entrypoints:

- `nextmeeting` (client)
- `nextmeetingd` (server daemon)

Ensure the service file is installed or documented, and mention the socket path
default `~/.cache/nextmeeting/socket`.

## License

Apache-2.0. See `LICENSE`.
