# nextmeeting

Show your next calendar meeting in your terminal or status bar.

nextmeeting is a client/server tool that fetches events from Google Calendar
and CalDAV servers, then displays the next meeting with countdown timers,
meeting links, and desktop notifications. It integrates with Waybar and other
status bars.

## Features

- **Multiple providers** -- Google Calendar (OAuth) and CalDAV
- **Status bar integration** -- Waybar JSON, plain TTY, or raw JSON output
- **Desktop GUI (preview)** -- Tauri-based desktop panel inspired by MeetingBar
- **Background daemon** -- Persistent background service with automatic refresh
- **Auto-spawn** -- The client starts the server automatically if it isn't running
- **Desktop notifications** -- Configurable alerts before meetings, with snooze
- **Meeting link detection** -- Extracts links for Zoom, Google Meet, Teams, Webex, Jitsi, and other services
- **Actions** -- Open meeting URLs, copy to clipboard, open calendar day view
- **Configuration reload** -- Reload settings after configuration updates
- **Terminal hyperlinks** -- Clickable meeting links in compatible terminals

## Installation

### From source

```sh
cargo install --path crates/nextmeeting-client
```

Or build the whole workspace:

```sh
cargo build --release
```

The binary is at `target/release/nextmeeting`.

### GUI preview (Tauri)

The repository now includes a focused desktop GUI shell in
`crates/nextmeeting-tauri`. It presents a compact meeting panel suitable for a
desktop menu or popover workflow.

To run it from source:

```sh
cargo run -p nextmeeting-tauri --bin nextmeeting-gui
```

Launch modes are selectable at startup:

```sh
# Standard desktop window mode (default)
cargo run -p nextmeeting-tauri --bin nextmeeting-gui -- --desktop

# Menu bar/tray mode with a toggleable popover window
cargo run -p nextmeeting-tauri --bin nextmeeting-gui -- --menubar
```

You may also set `NEXTMEETING_GUI_MODE=menubar` (or `desktop`).

The wired desktop actions currently are **Join next meeting**, **Create
meeting**, **Quick Actions** (open calendar day, refresh, snooze controls, and
clearing dismissed events), and **Preferences**.

## Quick Start

1. **Authenticate with Google Calendar:**

   ```sh
   nextmeeting auth google \
     --account work \
     --client-id YOUR_CLIENT_ID.apps.googleusercontent.com \
     --client-secret YOUR_CLIENT_SECRET
   ```

   This opens a browser for OAuth consent. Tokens are stored in
   `~/.config/nextmeeting/google-tokens-work.json`. If you only have one
   account configured, `--account` can be omitted.

2. **Show your next meeting:**

   ```sh
   nextmeeting
   ```

3. **Use with Waybar:**

   ```sh
   nextmeeting --waybar
   ```

The server starts automatically in the background on first use.

## Usage

```
nextmeeting [OPTIONS] [COMMAND]
```

### Output formats

| Flag         | Format                          |
|--------------|---------------------------------|
| *(default)*  | Human-readable terminal output  |
| `--waybar`   | JSON for Waybar custom module   |
| `--json`     | Machine-readable JSON           |

### Display options

| Flag                    | Description                                   |
|-------------------------|-----------------------------------------------|
| `--config` / `-c`       | Path to configuration file                     |
| `--debug` / `-v`        | Enable debug output                            |
| `--socket-path`         | Path to server socket                          |
| `--max-title-length N`  | Truncate titles with ellipsis                  |
| `--no-meeting-text TXT` | Text when no meetings (default: "No meeting")  |
| `--today-only`          | Only show today's meetings                     |
| `--limit N`             | Maximum number of meetings                     |
| `--skip-all-day-meeting`| Hide all-day events                            |

### Filters

| Flag                | Description                                      |
|---------------------|--------------------------------------------------|
| `--include-title P` | Only show meetings matching pattern (repeatable)  |
| `--exclude-title P` | Hide meetings matching pattern (repeatable)       |

### Actions

| Flag                  | Description                          |
|-----------------------|--------------------------------------|
| `--open-meet-url`     | Open the meeting link in browser     |
| `--copy-meeting-url`  | Copy the meeting URL to clipboard    |
| `--open-calendar-day` | Open the calendar day view           |
| `--refresh`           | Force refresh calendar data from providers |

### Notifications

| Flag                           | Description                            |
|--------------------------------|----------------------------------------|
| `--notify-min-before-events N` | Minutes before to notify (repeatable)  |
| `--snooze N`                   | Snooze notifications for N minutes     |

### Subcommands

| Command           | Description                        |
|-------------------|------------------------------------|
| `auth google`     | Authenticate with Google Calendar   |
| `config dump`     | Print current configuration        |
| `config validate` | Validate configuration file        |
| `config path`     | Show configuration file path       |
| `status`          | Show daemon status                 |
| `server`          | Run the server in the foreground   |

#### `auth google` flags

| Flag                  | Description                          |
|-----------------------|--------------------------------------|
| `--account` / `-a`    | Account name to authenticate         |
| `--client-id`         | OAuth client ID                      |
| `--client-secret`     | OAuth client secret                  |
| `--credentials-file`  | Google Cloud Console credentials JSON |
| `--domain`            | Google Workspace domain              |
| `--force` / `-f`      | Force re-authentication              |

## Configuration

Configuration file: `~/.config/nextmeeting/config.toml`

```toml
debug = false
# google_domain = "example.com"        # Google Workspace domain (for calendar URLs)

[[google.accounts]]
name = "work"
client_id = "YOUR_CLIENT_ID.apps.googleusercontent.com"
client_secret = "YOUR_CLIENT_SECRET"
# domain = "example.com"              # Google Workspace domain (per-account)
calendar_ids = ["primary"]
# token_path = "~/.config/nextmeeting/google-tokens-work.json"

# Add more accounts as needed:
# [[google.accounts]]
# name = "personal"
# client_id = "OTHER_CLIENT_ID.apps.googleusercontent.com"
# client_secret = "OTHER_CLIENT_SECRET"
# calendar_ids = ["primary"]

# [display]
# max_title_length = 30                # Truncate titles with ellipsis
# no_meeting_text = "No meeting"       # Text when no meetings
# format = "{title} {time}"            # Custom format template
# tooltip_format = "{title} {time}"    # Custom tooltip format template
# hour_separator = ":"                 # Hour separator character (e.g., ":", "h")
# until_offset = 60                    # Minutes offset after which absolute time is shown
# time_format = "24h"                  # Time format ("24h" or "12h")
# open_with = "firefox"                # Custom command for opening URLs
# tooltip_limit = 10                   # Maximum number of meetings in tooltip
# waybar_show_all_day = true           # Show all-day meetings in Waybar output

# [filters]
# today_only = false                   # Only show today's meetings
# limit = 5                            # Maximum number of meetings
# skip_all_day = false                 # Hide all-day events
# include_titles = ["standup"]         # Only show matching titles
# exclude_titles = ["lunch"]           # Hide matching titles
# include_calendars = ["Work"]         # Only include events from these calendars
# exclude_calendars = ["Holidays"]     # Exclude events from these calendars
# within_minutes = 60                  # Only show events starting within N minutes
# work_hours = "09:00-17:00"           # Only show events within work hours
# only_with_link = false               # Only show events with a meeting link
# privacy = false                      # Enable privacy mode (replace titles)
# privacy_title = "Meeting"            # Title to use in privacy mode
# skip_declined = false                # Skip events where you've declined
# skip_tentative = false               # Skip events where you've tentatively accepted
# skip_pending = false                 # Skip events where you haven't responded
# skip_without_guests = false          # Skip events without other attendees (solo events)

# [notifications]
# minutes_before = [5, 1]              # Minutes before to notify

# [server]
# socket_path = "/tmp/nextmeeting.sock"  # Path to server socket
# timeout = 5                            # Connection timeout in seconds

# [menubar]
# title_format = "full"                  # "full", "dot", or "hidden"
# title_max_length = 40                  # Max title length (ellipsis truncation)
# show_time = true                       # Show "(in Xm)" or "(Xm left)" suffix
# event_threshold_minutes = 30           # Optional: only show near-term events
```

### Secret references

Credential values (`client_id`, `client_secret`) support secret references
so you don't have to store secrets in cleartext:

| Prefix       | Behaviour                                         |
|--------------|----------------------------------------------------|
| `pass::path` | Runs `pass show path`, returns first line of stdout |
| `env::VAR`   | Reads environment variable `$VAR`                   |
| *(none)*     | Used as plain text                                  |

Example:

```toml
[[google.accounts]]
name = "work"
client_id = "pass::google/nextmeeting/client_id"
client_secret = "env::GOOGLE_CLIENT_SECRET"
```

### Google credentials

Credentials can be provided in order of priority:

1. CLI flags `--account <name> --client-id` / `--client-secret`
2. CLI flag `--account <name> --credentials-file` (Google Cloud Console JSON)
3. Config file `[[google.accounts]]` entries (with secret reference resolution)

When using CLI flags, `--account` is required to name the account. When
reading from config, `--account` can be omitted if only one account exists.

### CalDAV

CalDAV is supported as an additional calendar provider alongside Google
Calendar. It supports digest authentication and optional TLS verification.

## Status Bar Integration

### Waybar

Add to your Waybar config:

```json
"custom/nextmeeting": {
    "exec": "nextmeeting --waybar --max-title-length 30 --skip-all-day-meeting",
    "return-type": "json",
    "interval": 60,
    "tooltip": true,
    "on-click": "nextmeeting --open-meet-url",
    "on-click-right": "nextmeeting --copy-meeting-url"
}
```

Style with CSS classes `ongoing`, `soon`, `upcoming`, and `allday`:

```css
#custom-nextmeeting.soon {
    color: #f0c674;
}
#custom-nextmeeting.ongoing {
    color: #b5bd68;
}
```

## Server

The daemon runs in the background and keeps event data current for client
queries.

To run the server manually:

```sh
nextmeeting server
```

## Environment Variables

| Variable                  | Description                          |
|---------------------------|--------------------------------------|
| `NEXTMEETING_CONFIG`      | Path to configuration file           |
| `NEXTMEETING_SOCKET`      | Path to server socket                |
| `GOOGLE_CLIENT_ID`        | OAuth client ID                      |
| `GOOGLE_CLIENT_SECRET`    | OAuth client secret                  |
| `GOOGLE_CREDENTIALS_FILE` | Path to Google credentials JSON      |
| `RUST_LOG`                | Logging level (e.g. `debug`, `info`) |

## Architecture Notes

For architecture, runtime internals, and engineering notes, see `DESIGN.md`.

## License

Apache-2.0
