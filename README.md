# NextMeeting

NextMeeting is a Linux calendar companion with a client/daemon architecture.
It integrates with Google Calendar and CalDAV to display upcoming meetings in
the terminal or Waybar, and to run quick meeting actions.

## Features

- Google Calendar and CalDAV provider support.
- Terminal, JSON, and Waybar outputs.
- Native GTK4/libadwaita desktop UI (`nextmeeting-gtk`) with StatusNotifier tray integration.
- GTK per-event actions: edit event in calendar, local dismiss, plus calendar-backed decline/delete (Google provider).
- Automatic meeting-link detection (Zoom, Meet, Teams, Webex, Jitsi, and more).
- Desktop notification scheduling with snooze support.
- Action commands for joining meetings, copying meeting details, refreshing
  providers, and creating meeting links.

## Installation

### From source

Install the CLI:

```sh
cargo install --path crates/nextmeeting-client
```

Run the GTK desktop UI from source:

```sh
cargo run -p nextmeeting-gtk4 --bin nextmeeting-gtk
```

## Quick Start

1. Authenticate a Google account:

```sh
nextmeeting auth google --account work \
  --client-id <YOUR_CLIENT_ID> \
  --client-secret <YOUR_CLIENT_SECRET>
```

2. Show the next meeting:

```sh
nextmeeting
```

3. Use Waybar output:

```sh
nextmeeting --waybar
```

4. Launch the GTK desktop UI:

```sh
cargo run -p nextmeeting-gtk4 --bin nextmeeting-gtk
```

The daemon is started automatically when required.

GTK lifecycle behaviour:
- The app runs as a single instance; launching `nextmeeting-gtk` again presents the existing window.
- Closing the titlebar window hides it to tray; use tray `Quit` to exit the app.

In the GTK agenda list, use the row action menu to:
- Edit an event directly in Google Calendar (or open provider event URL)
- Dismiss an event locally (hide only)
- Decline an event in the calendar provider
- Delete an event occurrence (with confirmation)
- Click a meeting card to expand and view its event description inline

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

Use `config.example.toml` as a template.

Display timing notes:
- Near-term meetings are shown as relative text (`In 15 minutes`).
- Meetings beyond `display.until_offset` (default 60 minutes) are shown as absolute time.
- Cross-day meetings are shown as `Tomorrow at ...` or `Mon 03 at ...`.

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
