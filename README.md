# NextMeeting

NextMeeting is a Linux calendar tool with a CLI/daemon architecture. It reads
Google Calendar and CalDAV, shows upcoming meetings in the terminal or Waybar,
and supports quick meeting actions.

## Features

- Google Calendar and CalDAV provider support.
- Terminal, JSON, and Waybar outputs.
- Google event actions: edit in calendar, decline, and delete.
- Automatic meeting-link detection (Zoom, Meet, Teams, Webex, Jitsi, and more).
- Desktop notification scheduling with snooze support.
- Action commands for joining meetings, copying meeting details, refreshing
  providers, and creating meeting links.

## Installation

### From source

No packaged binaries are published yet. Install from source.

Install the CLI:

```sh
cargo install --path crates/nextmeeting-client
```

## Quick Start

### Google Calendar

Run the setup wizard:

```sh
nextmeeting auth google --guide
```

Quick path:

1. Create a Google OAuth client of type `Desktop app` (a `Web
   application` client will fail with `redirect_uri_mismatch`).
2. Download the credentials JSON file.
3. Run:

```sh
nextmeeting auth google --account work \
  --credentials-file /path/to/client_secret_<id>.json
```

By default, nextmeeting requests `calendar.events` (read/write events) and
`calendar.readonly` (list calendars). This enables decline and delete actions.
Use `--read-only` (or `read_only = true` in account config) for read-only
access.

Useful authentication commands:

```sh
nextmeeting auth                          # starts Google guide when no providers are configured
nextmeeting auth status                   # inspect all accounts and tokens
nextmeeting auth logout --account work    # clear stored tokens
nextmeeting auth logout --revoke          # also revoke them with Google
nextmeeting auth google --no-browser      # headless/SSH flow, paste redirect URL in terminal
```

If you re-authenticate whilst the daemon is running, the CLI notifies it and
it refreshes with the new tokens.

Tokens are stored in `~/.local/share/nextmeeting/` with `0600` permissions by
default. To store them in the desktop keyring instead (freedesktop Secret
Service, via `secret-tool` from `libsecret`), set
`token_storage = "keyring"` on the account.

### CalDAV

Add a CalDAV section to `~/.config/nextmeeting/config.toml`:

```toml
[caldav]
url = "https://caldav.example.com/calendars/chmouel/"
username = "env::NEXTMEETING_CALDAV_USER"
password = "env::NEXTMEETING_CALDAV_PASSWORD"
calendar_hint = "work"
```

### Run it

Show the next meeting:

```sh
nextmeeting
```

Use Waybar output:

```sh
nextmeeting --waybar
```

The CLI starts the daemon automatically when needed.

## Common Commands

- `nextmeeting --open-meet-url`
- `nextmeeting --copy-meeting-url`
- `nextmeeting --copy-meeting-id`
- `nextmeeting --copy-meeting-passcode`
- `nextmeeting --open-calendar-day`
- `nextmeeting --open-link-from-clipboard`
- `nextmeeting --create meet|zoom|teams|gcal`
- `nextmeeting --refresh`
- `nextmeeting --snooze N`

## Configuration

Default config path:

`~/.config/nextmeeting/config.toml`

Use `config.example.toml` as a template. Validate the resulting file with:

```sh
nextmeeting config validate
```

Google credentials may be supplied in either of these forms:

- `credentials_file = "/path/to/client_secret_<id>.json"` for the downloaded
  Google OAuth Desktop App JSON
- `client_id` and `client_secret` values, including `env::` and `pass::`
  references

CalDAV credentials also support `env::` and `pass::` references.

The daemon's cadence may be tuned in the `[server]` section:

- `sync_interval_secs` — interval between calendar syncs (default 300, minimum 30)
- `refresh_cooldown_secs` — cooldown after a manual refresh (default 30)
- `notify_tick_secs` — interval between notification checks (default 30, minimum 5)

Notifications run on their own ticker, independent of the sync interval, so
short-fuse reminders arrive punctually even with a leisurely sync cadence.

### Notification timing

For Google Calendar events, NextMeeting reads the event's own `reminders`
setting (popup reminders configured in Google Calendar, on by default) and
uses those minutes-before-start values in preference to the global
`notifications.minutes_before` configuration. If an event has explicit
reminder overrides, those are used; if it relies on the calendar's default
reminders, those defaults are resolved and used instead. Only when an event
has disabled reminders altogether, or none can be resolved (e.g. non-Google
providers), does NextMeeting fall back to `notifications.minutes_before`.

## Environment Variables

- `NEXTMEETING_CONFIG`
- `NEXTMEETING_SOCKET`
- `GOOGLE_CLIENT_ID`
- `GOOGLE_CLIENT_SECRET`
- `GOOGLE_CREDENTIALS_FILE`
- `RUST_LOG`

## Architecture

The CLI communicates with a background daemon over a Unix socket. The daemon
handles provider polling, caching, and notifications.

For implementation details, see `DESIGN.md`.

## Licence

Apache-2.0
