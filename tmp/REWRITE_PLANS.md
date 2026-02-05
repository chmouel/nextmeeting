# nextmeeting Rust Rewrite Plan

> **Keep this file updated with every implementation step.**

> If you need to refer to the original Python implementation for design decisions, you can read this repomix generated xml file python-nextmeeting.xml

## Decisions

| Decision | Choice |
|----------|--------|
| gcalcli | Direct API only |
| Scope | MVP first, feature parity roadmap |
| Daemon | systemd + launchd + auto-spawn |
| OAuth | User-provided client ID |
| Hash | SHA-256 |
| All-day | Trust calendar VALUE=DATE |
| RRULE | Server-side expand |
| Config | Hot reload via SIGHUP |
| Socket path | $XDG_RUNTIME_DIR/nextmeeting.sock |
| Config keys | snake_case |
| Error codes | MVP enum |

---

## Repository Structure

```
nextmeeting/
├── Cargo.toml
├── tmp/
│   ├── REWRITE_PLANS.md        # This file
│   ├── python-nextmeeting.xml # Output from `repomix --format xml` for original Python codebase
├── crates/
│   ├── nextmeeting-core/       # Time, events, links, filters, formatting
│   ├── nextmeeting-protocol/   # Framing, request/response types
│   ├── nextmeeting-providers/  # CalendarProvider trait, google/, caldav/
│   ├── nextmeeting-server/     # Daemon, scheduler, cache, notifications
│   └── nextmeeting-client/     # CLI, socket client, output, actions
```

---

## Phases

### Phase 0 — Foundations

- [x] Workspace with 5 crates
- [x] `EventTime` (DateTime/AllDay), `TimeWindow`
- [x] `NormalizedEvent`, `EventLink`, `LinkKind`, `MeetingView`
- [x] Link detection (Zoom, Meet, Teams, Jitsi, SafeLinks)
- [x] Link test corpus
- [x] Output formatting (TTY/Waybar/Polybar/JSON)
- [x] Golden tests for outputs
- [x] Protocol v1 framing + types
- [x] Tracing setup

### Phase 1 — Provider Abstraction

- [x] `CalendarProvider` trait
- [x] `RawEvent` struct
- [x] `RawEvent` → `NormalizedEvent` pipeline
- [x] `ProviderError` types

### Phase 2 — CalDAV Provider

- [x] HTTP client with digest auth
- [x] PROPFIND calendar discovery
- [x] REPORT with expand
- [x] ICS parsing (`icalendar` crate)
- [x] URL extraction from description/location
- [x] TLS toggle

### Phase 3 — Google Provider

- [ ] OAuth PKCE loopback
- [ ] Token persistence + scope tracking
- [ ] Events.list with singleEvents=true
- [ ] ETag conditional fetch
- [ ] Backoff on rate limits
- [ ] `nextmeeting auth google` command

### Phase 4 — Server Runtime

- [ ] Unix socket listener
- [ ] Request/response dispatch
- [ ] Event cache with TTL
- [ ] Scheduler (jitter, cooldown, backoff)
- [ ] Notification engine (`notify-rust`)
- [ ] SHA-256 dedup + snooze persistence
- [ ] SIGHUP reload, SIGTERM shutdown
- [ ] PID file

### Phase 5 — Client

- [ ] clap args (MVP subset)
- [ ] Socket client with timeout
- [ ] Auto-spawn fallback
- [ ] Output rendering (OSC8 hyperlinks)
- [ ] Actions: open, copy, snooze
- [ ] `config dump` / `validate` / `status`

### Phase 6 — Packaging

- [ ] systemd user unit
- [ ] launchd plist
- [ ] Shell completions
- [ ] AUR PKGBUILD
- [ ] Homebrew formula
- [ ] Migration guide

---

## MVP CLI Flags

```
--waybar, --polybar, --json
--max-title-length, --today-only, --limit
--skip-all-day-meeting, --include-title, --exclude-title
--notify-min-before-events, --snooze
--open-meet-url, --copy-meeting-url, --open-calendar-day
--google-domain, --config, --debug, --no-meeting-text
```

**Subcommands:** `auth google`, `config dump`, `config validate`, `status`, `server`

---

## Post-MVP Roadmap

| Version | Features |
|---------|----------|
| v0.2 | `--copy-meeting-id`, `--copy-meeting-passcode`, `--morning-agenda` |
| v0.3 | `--within-mins`, `--work-hours`, `--notify-offsets` |
| v0.4 | `--privacy`, `--include-calendar`, `--exclude-calendar` |
| v0.5 | `--open-link-from-clipboard`, `--create` |
| v1.0 | Full parity, 12h format, custom templates |

---

## Core Types

```rust
enum EventTime {
    DateTime(DateTime<Utc>),
    AllDay(NaiveDate),
}

struct NormalizedEvent {
    id, title, start, end, source_timezone,
    links: Vec<EventLink>, raw_location, raw_description,
    calendar_id, calendar_url, is_recurring_instance,
}

struct EventLink { kind: LinkKind, url, meeting_id, passcode }
enum LinkKind { GoogleMeet, Zoom, ZoomGov, Teams, Jitsi, Calendar, Other }

struct MeetingView {
    id, title, start_local, end_local,
    is_all_day, is_ongoing, primary_link, secondary_links, calendar_url,
}
```

---

## Protocol v1

**Envelope:** `{ protocol_version, request_id, payload }`

**Requests:** `GetMeetings`, `Status`, `Refresh { force }`, `Snooze { minutes }`, `Shutdown`, `Ping`

**Responses:** `Meetings { meetings }`, `Status { uptime, last_sync, providers, snoozed_until }`, `Ok`, `Error { code, message }`, `Pong`

---

## Dependencies

```toml
tokio, serde, serde_json, toml, chrono, clap, clap_complete,
reqwest, oauth2, url, uuid, tracing, tracing-subscriber,
notify-rust, icalendar, arboard (optional)
```

---

## Testing

- **Unit:** Time model, links, filters, formatting
- **Golden:** Output snapshots (insta)
- **Integration:** Protocol roundtrip, provider mocks
- **DST:** Spring-forward, fall-back, all-day on transition

---

## Open Items

- [ ] Socket path: `$XDG_RUNTIME_DIR/nextmeeting.sock` vs `/tmp/nextmeeting-$UID.sock`
- [x] Config key format: snake_case
- [x] Error code enumeration for protocol (MVP enum)
