# NextMeeting

NextMeeting is a Linux calendar companion with a client/daemon architecture.
It integrates with Google Calendar and CalDAV to display upcoming meetings in
the terminal or Waybar, and to run quick meeting actions.

## Features

- Google Calendar and CalDAV provider support.
- Terminal, JSON, and Waybar outputs.
- Calendar-backed per-event actions (Google): edit in calendar, decline, and delete.
- Automatic meeting-link detection (Zoom, Meet, Teams, Webex, Jitsi, and more).
- Desktop notification scheduling with snooze support.
- Action commands for joining meetings, copying meeting details, refreshing
  providers, and creating meeting links.

## Installation

### From source

No packaged binaries are published yet. The supported installation path is from
source.

Install the CLI:

```sh
cargo install --path crates/nextmeeting-client
```

## Quick Start

### Google Calendar

The interactive wizard walks you through the whole setup:

```sh
nextmeeting auth google --guide
```

The easiest Google path is:

1. Create a Google OAuth client of type `Desktop app` (a `Web
   application` client will fail with `redirect_uri_mismatch`).
2. Download the credentials JSON file.
3. Run:

```sh
nextmeeting auth google --account work \
  --credentials-file /path/to/client_secret_<id>.json
```

By default nextmeeting requests read/write access to events
(`calendar.events`) so actions such as decline and delete work out of
the box, plus `calendar.readonly` to list calendars. Pass `--read-only`
(or set `read_only = true` on the account) to request read-only access
instead.

Useful authentication commands:

```sh
nextmeeting auth                          # if no providers are configured, starts the Google guide
nextmeeting auth status                   # inspect all accounts and tokens
nextmeeting auth logout --account work    # clear stored tokens
nextmeeting auth logout --revoke          # also revoke them with Google
nextmeeting auth google --no-browser      # headless/SSH flow: paste the
                                          # redirect URL back into the terminal
```

If you re-authenticate whilst the daemon is running, it is notified
automatically and refreshes with the new tokens.

Tokens are stored in `~/.local/share/nextmeeting/` with `0600`
permissions by default. To keep them in the desktop keyring instead
(via the freedesktop Secret Service; requires `secret-tool` from
libsecret), set `token_storage = "keyring"` on the account.

### CalDAV

Add a CalDAV section to `~/.config/nextmeeting/config.toml`:

```toml
[caldav]
url = "https://caldav.example.com/calendars/chmouel/"
username = "env::NEXTMEETING_CALDAV_USER"
password = "env::NEXTMEETING_CALDAV_PASSWORD"
calendar_hint = "work"
```

### Run It

Show the next meeting:

```sh
nextmeeting
```

Use Waybar output:

```sh
nextmeeting --waybar
```

The daemon is started automatically when required.

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

Default configuration path:

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
