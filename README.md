# NextMeeting

NextMeeting is a calendar companion for Linux and macOS desktop environments.
It integrates with Google Calendar to display upcoming events in a status bar
(such as Waybar), in a desktop menubar app, or in your terminal.

## Features

- Google Calendar integration with OAuth authentication.
- Waybar integration plus a standalone desktop/menubar application.
- Automatic meeting-link detection for Zoom, Google Meet, Teams, Webex, and Jitsi.
- Customisable desktop alerts before meetings commence.
- Client/server model: a background daemon with a responsive CLI client.

## Installation

### From Source (Rust)

NextMeeting is built with Rust. Install the CLI with Cargo:

```sh
cargo install --path crates/nextmeeting-client
```

### Linux desktop prerequisites

The Tauri desktop build depends on WebKitGTK 4.1 and JavaScriptCoreGTK 4.1.
On Arch Linux, install:

```sh
sudo pacman -S --needed webkit2gtk-4.1
```

If `pkg-config` still cannot find `webkit2gtk-4.1.pc` or
`javascriptcoregtk-4.1.pc`, set:

```sh
export PKG_CONFIG_PATH=/usr/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}
```

## Quick Start

1. Connect your calendar:

   ```sh
   nextmeeting auth google --account work \
     --client-id <YOUR_CLIENT_ID> \
     --client-secret <YOUR_CLIENT_SECRET>
   ```

2. Show your next event:

   ```sh
   nextmeeting
   ```

3. Integrate with Waybar:

   ```sh
   nextmeeting --waybar
   ```

The server starts automatically in the background on first use.

## Usage

### Common commands

- `nextmeeting --open-meet-url` opens the active meeting URL.
- `nextmeeting --copy-meeting-url` copies the meeting URL to clipboard.
- `nextmeeting --copy-meeting-id` copies the meeting ID to clipboard.
- `nextmeeting --copy-meeting-passcode` copies the meeting passcode to clipboard.
- `nextmeeting --open-calendar-day` opens today in Google Calendar.
- `nextmeeting --open-link-from-clipboard` opens a meeting link found in clipboard text.
- `nextmeeting --create meet|zoom|teams|gcal` creates a quick meeting link.
- `nextmeeting --refresh` forces a provider refresh.

### Useful flags

| Flag | Description |
|------|-------------|
| `--waybar` | Output JSON for Waybar custom modules |
| `--json` | Output machine-readable JSON |
| `--config` / `-c` | Path to configuration file |
| `--debug` / `-v` | Enable debug output |
| `--socket-path` | Path to server socket |
| `--privacy` | Replace meeting titles in output |
| `--snooze N` | Snooze notifications for N minutes |
| `--create meet|zoom|teams|gcal` | Create a quick meeting link |
| `--create-url URL` | Use a custom URL with `--create` |

### Subcommands

- `nextmeeting auth google` authenticates with Google Calendar.
- `nextmeeting config dump` prints effective configuration.
- `nextmeeting config validate` validates the configuration file.
- `nextmeeting config path` shows the resolved config path.
- `nextmeeting status` shows daemon status.
- `nextmeeting server` starts the daemon in foreground mode.

### Waybar module example

```json
"custom/nextmeeting": {
    "exec": "nextmeeting --waybar",
    "return-type": "json",
    "interval": 60,
    "on-click": "nextmeeting --open-meet-url",
    "on-click-right": "nextmeeting --copy-meeting-url",
    "tooltip": true
}
```

Display, filtering, and notification behaviour are configured in
`~/.config/nextmeeting/config.toml` (see [config.example.toml](./config.example.toml)).

## Configuration

Configuration path: `~/.config/nextmeeting/config.toml`

A full template is available in [`config.example.toml`](./config.example.toml).

Minimal example:

```toml
[[google.accounts]]
name = "work"
client_id = "pass::google/nextmeeting/client_id"
client_secret = "env::GOOGLE_CLIENT_SECRET"
calendar_ids = ["primary"]

[display]
time_format = "24h"
max_title_length = 30
no_meeting_text = "No meeting"

[filters]
today_only = true
exclude_titles = ["Lunch", "Out of Office"]
```

### Secret references

Credential values (`client_id`, `client_secret`) support secret references:

| Prefix | Behaviour |
|--------|-----------|
| `pass::path` | Runs `pass show path` and uses the first stdout line |
| `env::VAR` | Reads environment variable `$VAR` |
| *(none)* | Uses literal plain text |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `NEXTMEETING_CONFIG` | Path to configuration file |
| `NEXTMEETING_SOCKET` | Path to server socket |
| `GOOGLE_CLIENT_ID` | OAuth client ID |
| `GOOGLE_CLIENT_SECRET` | OAuth client secret |
| `GOOGLE_CREDENTIALS_FILE` | Path to Google credentials JSON |
| `RUST_LOG` | Logging level (for example `debug` or `info`) |

## Architecture

NextMeeting uses a client/server design. The CLI talks to a lightweight
background daemon that handles caching and provider polling.

For implementation internals, see [`DESIGN.md`](./DESIGN.md).

## License

Apache-2.0
