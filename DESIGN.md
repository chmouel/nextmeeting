# nextmeeting Review

This document summarises the current architecture, implemented commands, and feature set of this repository as of February 8, 2026.

## 1. High-Level Architecture

`nextmeeting` is a Rust workspace with a client/server design:

- `nextmeeting-client` (`nextmeeting` binary):
  - CLI entrypoint.
  - Connects to daemon over Unix socket.
  - Auto-spawns daemon when unavailable.
  - Renders TTY/Waybar/JSON output.
  - Executes user actions (open/copy URLs, refresh, snooze, create meeting links).
- `nextmeeting-server`:
  - Long-running daemon.
  - Holds in-memory meeting/provider state.
  - Schedules periodic provider sync with jitter + exponential backoff.
  - Serves IPC requests over Unix domain socket.
  - Sends desktop notifications.
- `nextmeeting-providers`:
  - Provider abstraction (`CalendarProvider` trait).
  - Google and CalDAV provider implementations.
  - Normalisation from provider-specific raw events to canonical events.
- `nextmeeting-core`:
  - Canonical event/time/link models.
  - Meeting link detection/normalisation (many providers).
  - Output formatting for terminal, Waybar, and JSON.
- `nextmeeting-protocol`:
  - IPC request/response schema.
  - Framing: 4-byte big-endian length + JSON envelope.
  - Protocol constants (`PROTOCOL_VERSION = "1"`, max message size 1MB).
- `nextmeeting-tauri` (`nextmeeting-gui` binary):
  - Desktop panel/menu-bar preview application.
  - Reuses socket protocol and client actions.
  - Falls back to mock data when daemon is unavailable.

## 2. Runtime Data Flow

1. User runs `nextmeeting`.
2. CLI loads config (`~/.config/nextmeeting/config.toml` by default).
3. CLI builds a `Request` and calls daemon over Unix socket.
4. If connection fails, CLI auto-spawns `nextmeeting server`, waits for readiness, retries request.
5. Server request handler returns meetings/status/acknowledgements.
6. CLI either:
   - prints formatted output (TTY/Waybar/JSON), or
   - performs action (open URL, copy URL/ID/passcode, open calendar day, refresh, snooze, create).

Server background loop:

1. Build configured providers.
2. Scheduler triggers sync.
3. Each provider fetches `RawEvent`.
4. `normalize_events` converts to `NormalizedEvent`.
5. `MeetingView::from_event` produces display-ready meetings.
6. Server state is updated and sorted.
7. Notification engine checks thresholds and emits deduplicated notifications.

## 3. Crate Responsibilities

### 3.1 `nextmeeting-client`

Key modules:

- `src/main.rs`: CLI execution, auto-spawn, filter building, rendering, status output.
- `src/cli.rs`: Clap schema (flags and subcommands).
- `src/actions.rs`: local user actions and request-based actions (`refresh`, `snooze`).
- `src/socket.rs`: Unix socket request/response exchange with request correlation.
- `src/commands/auth.rs`: Google auth command and credential persistence.
- `src/commands/server.rs`: daemon bootstrap orchestration.
- `src/config.rs`: config schema + XDG paths + secret reference handling.

### 3.2 `nextmeeting-server`

Key modules:

- `scheduler.rs`: interval sync, jitter, cooldown, exponential backoff, commands (`Refresh`, `SyncNow`, `Pause`, `Resume`, `Stop`).
- `handler.rs`: protocol request dispatch (`Ping`, `Status`, `GetMeetings`, `Refresh`, `Snooze`, `Shutdown`) and filter application.
- `socket.rs`: Unix listener, framed protocol I/O, stale socket cleanup, connection concurrency limit.
- `notify.rs`: desktop notifications, dedup hash (`SHA-256`), optional morning agenda.
- `signals.rs`: `SIGTERM`/`SIGINT` shutdown and `SIGHUP` reload signalling.
- `pidfile.rs`: duplicate-instance guard.
- `cache.rs`: TTL cache utilities (currently generic infra; server state currently uses in-memory meetings directly).

### 3.3 `nextmeeting-providers`

- `provider.rs`: `CalendarProvider` trait + `FetchOptions` + `FetchResult`.
- `normalize.rs`: raw-to-normalized conversion pipeline, link extraction precedence.
- `google/*`: OAuth2 PKCE loopback auth, token refresh/storage, Calendar API fetch.
- `caldav/*`: CalDAV discovery (PROPFIND), event fetch (REPORT), ICS parsing.

### 3.4 `nextmeeting-core`

- `event.rs`: canonical event types (`NormalizedEvent`, `MeetingView`, link and attendee metadata).
- `links.rs`: URL extraction, SafeLinks unwrapping, service classification across many meeting services.
- `format/mod.rs`: formatter for TTY, Waybar JSON, structured JSON output.
- `time.rs`: time-window abstractions.

### 3.5 `nextmeeting-protocol`

- `types.rs`: `Envelope`, `Request`, `Response`, `MeetingsFilter`, provider status/state payloads.
- `framing.rs`: serialization with length-prefix framing.
- `error.rs`: protocol error model.

### 3.6 `nextmeeting-tauri`

- `src/main.rs`: dashboard data, socket-backed commands (`refresh`, `snooze`, join/open/create), desktop vs menubar launch modes.
- `ui/app.js`: rendering timeline/meeting cards/actions; periodic refresh.

## 4. Command Surface (Current CLI)

Binary:

- `nextmeeting [OPTIONS] [COMMAND]`

Top-level options:

- `--config, -c <PATH>`
- `--debug, -v`
- `--waybar`
- `--json`
- `--privacy`
- `--snooze <MINUTES>`
- `--open-meet-url`
- `--copy-meeting-url`
- `--copy-meeting-id`
- `--copy-meeting-passcode`
- `--open-calendar-day`
- `--open-link-from-clipboard`
- `--create <SERVICE>`
- `--create-url <URL>`
- `--refresh`
- `--socket-path <PATH>`

Subcommands:

- `nextmeeting auth google [...]`
- `nextmeeting config dump`
- `nextmeeting config validate`
- `nextmeeting config path`
- `nextmeeting status`
- `nextmeeting server`

`auth google` notable flags:

- `--account, -a`
- `--client-id`
- `--client-secret`
- `--credentials-file`
- `--domain`
- `--force, -f`

Environment variables:

- `NEXTMEETING_CONFIG`
- `NEXTMEETING_SOCKET`
- `GOOGLE_CLIENT_ID`
- `GOOGLE_CLIENT_SECRET`
- `GOOGLE_CREDENTIALS_FILE`
- `RUST_LOG`

## 5. Configuration Model

Default config path:

- `~/.config/nextmeeting/config.toml`

Main sections:

- `[google]` / `[[google.accounts]]` (when Google feature enabled)
- `[display]`
- `[filters]`
- `[notifications]`
- `[server]`

Credential resolution supports:

- `pass::path/to/secret` (via `pass show`, first line)
- `env::VAR_NAME`
- plain text values

## 6. Implemented Features

### Core UX

- Client/daemon split with automatic daemon spawning.
- Unix socket IPC with request-response correlation IDs.
- Output modes: terminal, Waybar JSON, machine JSON.
- Status command with provider health.

### Providers

- Google Calendar provider:
  - OAuth 2.0 PKCE (loopback callback).
  - token refresh support.
  - multi-account configuration.
- CalDAV provider:
  - calendar discovery.
  - digest auth support path in provider stack.
  - ICS parsing pipeline.

### Filtering and Display

- Filters include:
  - today-only, limit, skip all-day
  - include/exclude title patterns
  - include/exclude calendar patterns
  - within-minutes
  - only-with-link
  - work-hours window
  - privacy mode/title masking
  - response-state filtering (declined/tentative/pending)
  - skip solo events
- Formatter capabilities:
  - truncation
  - relative vs absolute time handling
  - custom format templates
  - 12h/24h options
  - Waybar class + optional colour markup

### Meeting Links and Actions

- Extensive meeting URL detection and normalisation.
- Meeting action commands:
  - open next meeting URL
  - copy meeting URL
  - copy meeting ID
  - copy meeting passcode
  - open calendar day
  - open link from clipboard
  - create meeting URLs for meet/zoom/teams/gcal

### Notifications

- Configurable pre-meeting reminders (`minutes_before` list).
- Deduplication based on notification hash.
- Snooze via command/protocol.
- Optional morning agenda notification time.

### GUI (Tauri Preview)

- Desktop and menubar modes.
- Live data via daemon protocol with fallback to mock mode.
- Actions wired to existing client/server behaviours.

## 7. Notable Implementation Characteristics

- Scheduler defaults: 5-minute sync interval, 10% jitter, cooldown + capped exponential backoff.
- Protocol messages are versioned via envelope and have strict size caps.
- Socket startup handles stale socket cleanup and concurrent connection limits.
- A PID file prevents duplicate server instances.

## 8. Current Boundaries and Gaps

- The `nextmeeting-server` cache module exists and is tested, but active meeting serving currently relies on in-memory `ServerState` rather than direct `EventCache` integration.
- `SIGHUP` reload signal plumbing exists; full dynamic provider/config rebuild flow is not yet surfaced as a complete runtime reload path in server orchestration.
- Some README flag examples describe options that are now configuration-driven rather than direct CLI flags.

## 9. Practical Command Examples

Run default output:

```bash
nextmeeting
```

Waybar mode:

```bash
nextmeeting --waybar
```

Machine JSON:

```bash
nextmeeting --json
```

Force provider refresh:

```bash
nextmeeting --refresh
```

Snooze notifications for 30 minutes:

```bash
nextmeeting --snooze 30
```

Start daemon in foreground:

```bash
nextmeeting server
```

Authenticate Google account:

```bash
nextmeeting auth google --account work --credentials-file /path/to/credentials.json
```
