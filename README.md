# nextmeeting

Show your next calendar meeting in your terminal or status bar.

nextmeeting is a client/server tool that fetches events from Google Calendar
and CalDAV servers, then displays the next meeting with countdown timers,
meeting links, and desktop notifications. It integrates with Waybar, Polybar,
and other status bars.

## Features

- **Multiple providers** -- Google Calendar (OAuth) and CalDAV
- **Status bar integration** -- Waybar JSON, Polybar single-line, plain TTY, or raw JSON output
- **Background daemon** -- Persistent server with event caching, automatic polling, and exponential backoff
- **Auto-spawn** -- The client starts the server automatically if it isn't running
- **Desktop notifications** -- Configurable alerts before meetings with SHA-256 deduplication and snooze
- **Meeting link detection** -- Extracts Zoom, Google Meet, Teams, and Jitsi links (including SafeLinks unwrapping)
- **Actions** -- Open meeting URLs, copy to clipboard, open calendar day view
- **Hot reload** -- Server reloads configuration on SIGHUP
- **Terminal hyperlinks** -- OSC8 clickable links in terminal output

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

## Quick Start

1. **Authenticate with Google Calendar:**

   ```sh
   nextmeeting auth google \
     --client-id YOUR_CLIENT_ID.apps.googleusercontent.com \
     --client-secret YOUR_CLIENT_SECRET
   ```

   This opens a browser for OAuth consent. Tokens are stored in
   `~/.config/nextmeeting/google-tokens.json`.

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
| `--polybar`  | Single-line text for Polybar    |
| `--json`     | Machine-readable JSON           |

### Display options

| Flag                    | Description                                   |
|-------------------------|-----------------------------------------------|
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

### Notifications

| Flag                           | Description                            |
|--------------------------------|----------------------------------------|
| `--notify-min-before-events N` | Minutes before to notify (repeatable)  |
| `--snooze N`                   | Snooze notifications for N minutes     |

### Subcommands

| Command           | Description                        |
|-------------------|------------------------------------|
| `auth google`     | Authenticate with Google Calendar  |
| `config dump`     | Print current configuration        |
| `config validate` | Validate configuration file        |
| `config path`     | Show configuration file path       |
| `status`          | Show daemon status                 |
| `server`          | Run the server in the foreground   |

## Configuration

Configuration file: `~/.config/nextmeeting/config.toml`

```toml
debug = false

[google]
client_id = "YOUR_CLIENT_ID.apps.googleusercontent.com"
client_secret = "YOUR_CLIENT_SECRET"
# domain = "example.com"              # Google Workspace domain
calendar_ids = ["primary"]
# token_path = "~/.config/nextmeeting/google-tokens.json"
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
[google]
client_id = "pass::google/nextmeeting/client_id"
client_secret = "env::GOOGLE_CLIENT_SECRET"
```

### Google credentials

Credentials can be provided in order of priority:

1. CLI flags `--client-id` / `--client-secret`
2. CLI flag `--credentials-file` (Google Cloud Console JSON)
3. Config file `client_id` / `client_secret` (with secret reference resolution)

### CalDAV

CalDAV providers are configured with a server URL and optional credentials.
The provider supports digest authentication and optional TLS verification.

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

### Polybar

```ini
[module/nextmeeting]
type = custom/script
exec = nextmeeting --polybar --max-title-length 30 --skip-all-day-meeting
interval = 60
click-left = nextmeeting --open-meet-url
click-right = nextmeeting --copy-meeting-url
```

## Server

The daemon runs in the background and caches calendar events. It communicates
with the client over a Unix socket at `$XDG_RUNTIME_DIR/nextmeeting.sock`.

- **Auto-spawn**: The client starts the server automatically when needed
- **Polling**: Events are synced every 5 minutes with jitter and exponential backoff
- **Signals**: Send `SIGHUP` to reload config, `SIGTERM` to shut down gracefully
- **PID file**: Written to `$XDG_RUNTIME_DIR/nextmeeting.pid`

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

## Roadmap

| Version | Features                                              |
|---------|-------------------------------------------------------|
| v0.2    | Copy meeting ID/passcode, morning agenda              |
| v0.3    | Time window filters, work hours, notification offsets  |
| v0.4    | Privacy mode, calendar include/exclude filters         |
| v0.5    | Clipboard link opening, event creation                 |
| v1.0    | Full parity, 12h time format, custom templates         |

## License

Apache-2.0
