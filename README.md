# nextmeeting - Show your calendar next meeting in your waybar or polybar

## What is it?

nextmeeting is a simple CLI tool that leverages `gcalcli` to display your
upcoming meetings.

It offers several features beyond basic `gcalcli` functionality:

- **Bar Integration:** Seamlessly integrates with status bars like
  [Waybar](https://github.com/Alexays/Waybar) and [Polybar](https://github.com/polybar/polybar).
- **Smart Date Display:** Shows dates in a human-readable English format (e.g.,
  "tomorrow," "next Monday," not just raw dates).
- **Time-to-Meeting:** Displays the remaining time until the current meeting starts.
- **Color-Coded Alerts:** Changes colors when a meeting is 5 minutes away.
- **Hyperlink Support:** Provides clickable hyperlinks in the default terminal view.
- **Meeting Notifications:** Sends notifications via `notify-send` 5 minutes
  before a meeting.
- **Title Ellipsis:** Truncates long meeting titles for better display.
- **Next-Day Exclusion:** Option to exclude meetings scheduled for the next day.
- **Generic Server Support:** Query CalDAV-compatible calendars with the
  `--caldav-*` flags.

## Screenshot

![192647099-ccfa2002-0db3-4738-a54b-176a03474483](https://user-images.githubusercontent.com/98980/212869786-1acd56e2-2e8a-4255-98c3-ebbb45b28d6e.png)

## Installation

Use `pip` with:

`pip install -U nextmeeting`

Alternatively, if you prefer to run from source, you can use `uv` (recommended)
or install dependencies manually.

### Using uv (recommended)

First, install `uv` by following the instructions [uv installation
guide](https://docs.astral.sh/uv/getting-started/installation/). Then, clone
this repository and run:

```shell
uv run nextmeeting
```

### Manual Installation

If you don't want to use `uv`, you can install the dependencies manually from
PyPI or your operating system's package manager:

- [python-dateutil](https://pypi.org/project/python-dateutil/)
- [gcalcli](https://pypi.org/project/gcalcli/)
- [caldav](https://pypi.org/project/caldav/) *(required when you use the
  `--caldav-*` options)*

After installing dependencies, you can run the `nextmeeting` script directly:

```shell
python3 src/nextmeeting/cli.py
```

You can also copy `src/nextmeeting/cli.py` to your system's PATH for convenience.

### [AUR](https://aur.archlinux.org/packages/nextmeeting)

```shell
yay -S nextmeeting
```

### NixOS

<details><summary>Flake and Home-Manager install instructions.</summary>

- Add nextmeeting to your flake.

```nix
nextmeeting = {
  url = "github:chmouel/nextmeeting?dir=packaging";
  inputs.nixpkgs.follows = "nixpkgs";
};
```

- Use Home-manager to add nextmeeting to waybar like this:

```nix
let 
  nextmeeting = lib.getExe inputs.nextmeeting.packages.${pkgs.system}.default;
in
{
  "custom/agenda" = {
      format = "{}";
      exec = nextmeeting + "--max-title-length 30 --waybar";
      on-click = nextmeeting + "--open-meet-url";
      interval = 59;
      return-type = "json";
      tooltip = true;
  };
}
```

- Follow along with the rest of the instructions.

</details>

## How to use it?

You need to install the [gcalcli](https://github.com/insanum/gcalcli) tool and
[setup the google Oauth
integration](https://github.com/insanum/gcalcli?tab=readme-ov-file#initial-setup)
with google calendar.

By default, you can start `nextmeeting`, and it will display your list of
meetings with a human-readable date format.

If no meetings are displayed, you might need to specify the target calendar
using the `--calendar=CALENDAR` flag.

There are a few options to customize its behavior; see `nextmeeting --help` for
more details.

### Working with other calendar servers

Point nextmeeting straight at a CalDAV-compatible server when you do not want
to rely on `gcalcli`:

```shell
nextmeeting \
  --caldav-url https://example.com/dav/calendars/user/work/ \
  --caldav-username user \
  --caldav-password secret
```

Every filter and output mode still applies. If your server exposes several
collections, pass `--caldav-calendar` with the specific collection URL. For
basic-auth deployments the username and password flags are enough; for token
auth just put the token in `--caldav-password`.

The fetch window defaults to “12 hours back, 48 hours ahead”. Tune it with:

- `--caldav-lookbehind-hours` to include ongoing meetings that started up to N
  hours ago (default `12`).
- `--caldav-lookahead-hours` to look N hours into the future (default `48`).

To avoid typing the flags each time, drop them in your config file:

```toml
[nextmeeting]
caldav-url = "https://example.com/dav/calendars/user/work/"
caldav-username = "user"
caldav-password = "secret"
today-only = true
```

If a request fails, rerun with `-v` (or `--debug`) to see the full traceback and
the endpoints that were attempted.

### JSON output

If you need machine-readable output outside Waybar, use `--json` to print the
same JSON shape as `--waybar` (keys like `text`, `tooltip`, and optional
`class`). This is useful for other bars or scripts:

```shell
nextmeeting --json
```

### Configuration file

You can set defaults in a TOML file. By default, `~/.config/nextmeeting/config.toml`
is loaded if present, or you can point to a custom file with `--config`.

Example `~/.config/nextmeeting/config.toml`:

```toml
[nextmeeting]
calendar = "Work"
max-title-length = 30
today-only = true
include-title = ["standup", "1:1"]
exclude-title = ["OOO"]
notify-min-before-events = 5
notify-offsets = [15, 5]
privacy = false
```

CLI flags always override config values.

### Event caching

Reduce calls to `gcalcli` by caching its raw output for a short period:

```shell
nextmeeting --cache-events-ttl 2   # cache for 2 minutes
```

### Polybar output

For Polybar, print a single-line text with the next meeting:

```shell
nextmeeting --polybar
```

It uses the same formatting and filters as other modes and respects
`--max-title-length`.

### Custom formatting

You can customize how each line is rendered using templates. Available
placeholders: `{when}`, `{title}`, `{start_time}`, `{end_time}`, `{meet_url}`,
`{calendar_url}`, `{minutes_until}`, `{is_all_day}`, `{is_ongoing}`.

```shell
# Single-line formatting (TTY, Polybar, and Waybar text)
nextmeeting --format "{when} • {title}"

# Waybar tooltip formatting (applies to the tooltip only)
nextmeeting --waybar --tooltip-format "{start_time:%H:%M}-{end_time:%H:%M} · {title}"
```

Use 12-hour timestamps for absolute times:

```shell
nextmeeting --time-format 12h
```

### Showing multiple items

Limit the number of meetings shown in list-style outputs (TTY and Waybar
tooltip):

```shell
nextmeeting --limit 3
```

### Title filters

You can include or exclude meetings based on title substrings (case-insensitive):

```shell
# Only include meetings containing either "standup" or "1:1"
nextmeeting --include-title standup --include-title "1:1"

# Exclude meetings containing "OOO" or "holiday"
nextmeeting --exclude-title ooo --exclude-title holiday
```

Filters apply across modes (TTY, `--json`, `--waybar`).

You can also restrict to working hours by start time:

```shell
nextmeeting --work-hours 09:00-18:00
```

Filter by calendar using substrings of the event URL (useful when you have
multiple accounts/calendars):

```shell
nextmeeting --include-calendar "primary" --exclude-calendar "personal"
```

### Privacy mode

Redact meeting titles to a static label to avoid leaking details:

```shell
nextmeeting --privacy               # titles become "Busy"
nextmeeting --privacy --privacy-title "Busy 🗓️"
```

### Quick actions

- Open the next meeting URL: `nextmeeting --open-meet-url`
- Copy the next meeting URL to the clipboard (tries `wl-copy`, `xclip`, or `pbcopy`; falls back to printing):

```shell
nextmeeting --copy-meeting-url
```

- Open the Google Calendar day view for the next meeting (respects
  `--google-domain` if set):

```shell
nextmeeting --open-calendar-day
```

### Waybar

A more interesting use case for `nextmeeting` is its integration with Waybar,
allowing for a clean output on your desktop. For example, my configuration
looks like this:

```json
    "custom/agenda": {
        "format": "{}",
        "exec": "nextmeeting --max-title-length 30 --waybar",
        "on-click": "nextmeeting --open-meet-url",
        "on-click-right": "kitty -- /bin/bash -c \"batz;echo;cal -3;echo;nextmeeting;read;\"",
        "interval": 59,
        "return-type": "json",
        "tooltip": "true"
    },
```

This configuration displays the time remaining until my next meeting. Clicking
the item opens the meeting's URL. A right-click launches a `kitty` terminal to
show time zones using [batz](https://github.com/chmouel/batzconverter) and my
next meeting. I can also click on the meeting title within the terminal to open
its URL.

#### Styling

You can style the Waybar item using the following CSS:

```css
#custom-agenda {
  color: #696969;
}
```

If you enable the `--notify-min-before-events` option, `nextmeeting` will
output a `soon` class when an event is approaching, allowing you to style it
with:

```css
#custom-agenda.soon {
  color: #eb4d4b;
}
```

### Notifications

- Keep the existing “soon” visual cue with `--notify-min-before-events`.
- Add more reminder moments using `--notify-offsets` (repeatable or CSV):

```shell
nextmeeting --notify-offsets 15 --notify-offsets 5   # 15 and 5 minutes
nextmeeting --notify-offsets 20,10,5                 # CSV variant
```

Control the urgency with `--notify-urgency low|normal|critical`.

Snooze all notifications for a period and exit:

```shell
nextmeeting --snooze 30    # minutes
```

Send a once-per-day morning agenda summary at a given time:

```shell
nextmeeting --morning-agenda 09:00
```

### Related

- For Gnome: [gnome-next-meeting-applet](https://github.com/chmouel/gnome-next-meeting-applet)

## Copyright

[Apache-2.0](./LICENSE)

## Authors

- Chmouel Boudjnah <https://github.com/chmouel>
  - Fediverse - <[@chmouel@fosstodon.org](https://fosstodon.org/@chmouel)>
  - Twitter - <[@chmouel](https://twitter.com/chmouel)>
  - Blog - <[https://blog.chmouel.com](https://blog.chmouel.com)>
