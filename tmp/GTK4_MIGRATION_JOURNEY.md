# GTK4 Migration Journey

Last updated: 2026-02-08

## Goal

Migrate from Tauri/macOS support to a native Linux GTK4/libadwaita UI.

## Decision Log

1. Remove macOS support.
2. Remove Tauri GUI.
3. Remove legacy `[menubar]` config now (breaking change accepted).
4. Delivery shape: 3 PRs.
   - PR1: cleanup/removal
   - PR2: GTK4 MVP
   - PR3: parity/polish

## Current State

### Completed (PR1)

- Tauri crate removed and committed.
- Linux-only documentation/config baseline in place.

### Completed in this run (PR2 GTK MVP)

- Added new workspace crate: `crates/nextmeeting-gtk4`.
- Added binary target: `nextmeeting-gtk`.
- Added working GTK module layout:
  - `application.rs`
  - `config.rs`
  - `daemon/{mod.rs,client.rs,state.rs}`
  - `tray/{mod.rs,manager.rs,sni.rs}`
  - `widgets/{mod.rs,window.rs,meeting_card.rs,meeting_row.rs,timeline.rs,status_indicator.rs}`
  - `actions/{mod.rs,handlers.rs}`
  - `dismissals.rs`
  - `utils.rs`
- Added resources and blueprint placeholders:
  - `resources/nextmeeting.gresource.xml`
  - `resources/style.css`
  - `resources/icons/nextmeeting.svg`
  - `resources/icons/nextmeeting-symbolic.svg`
  - `blueprints/*.blp` placeholders
- Implemented working GTK MVP:
  - libadwaita application window
  - meeting list rendering from daemon state
  - daemon requests now honour `ClientConfig.filters` (same request filter model as CLI)
  - actions: join/create/refresh/snooze/open calendar day
  - event dismiss and clear dismissals
  - ksni tray with toggle/refresh/quit commands
  - async bridge using tokio runtime
  - refreshed visual design after user feedback:
    - tighter popup dimensions (`460x560`)
    - hero panel and status pill styling
    - section labels and pill-style action buttons
    - meeting row card styling and improved typography
    - reduced oversized blank list panel by setting list scroller minimum/natural sizing

### Remaining for parity

- Timeline widget implementation.
- Preferences dialog and additional advanced actions.
- Blueprint-compiled UI migration from code-built widgets.
- Explicit AppIndicator fallback for tray on environments without SNI host.
- Replace placeholder widget stubs:
  - `widgets/meeting_card.rs`
  - `widgets/meeting_row.rs`
  - `widgets/timeline.rs`
  - `widgets/status_indicator.rs`

## Verification Notes (from this run)

- `cargo build --all`: passed.
- `cargo clippy --all --all-features --fix --allow-dirty`: passed.
- `cargo test --all`: passed.
- GTK tests now include filter mapping coverage in `daemon/client.rs`:
  - `build_filter_maps_skip_all_day_and_exclusions`
- Note: existing warning remains in unrelated code:
  - `crates/nextmeeting-providers/src/caldav/mod.rs`
  - `unused import: auth::DigestAuth`

Re-run full verification after PR1 changes on a normal host/CI:

```bash
cargo build --all
cargo clippy --all --all-features --fix
cargo test --all
```

## Next Run: Exact Steps

1. Implement timeline visualisation in `widgets/timeline.rs`, then mount it in `widgets/window.rs` below hero/actions.
2. Move from code-built UI to Blueprint:
   - replace placeholders in `blueprints/*.blp` with real layouts
   - compile blueprint into GResource during build
   - load templates in widget classes instead of constructing all controls in Rust
3. Implement preferences dialogue:
   - create GTK preferences window/dialog
   - expose create-link defaults, socket path override, and UI toggles
4. Implement AppIndicator fallback in tray manager:
   - keep `ksni` as primary
   - on tray host failure, route to fallback backend
5. Expand action surface:
   - undismiss single event
   - clear dismissals confirmation flow
   - keyboard shortcuts for refresh/join/quit
6. Add targeted tests where possible:
   - dismissal persistence path handling
   - daemon client response mapping edge cases
7. Run:
   - `cargo build --all`
   - `cargo clippy --all --all-features --fix --allow-dirty`
   - targeted manual smoke checks (tray toggle, meeting data, actions).

## Open Risk

- Tray behaviour still depends on a running StatusNotifier host in the desktop session.
- Final UX validation must be done in an interactive Linux desktop session.
- Blueprint migration still pending, so UI is not yet designer-editable outside Rust.
