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

### Completed in this run (PR1 implemented)

- Deleted `crates/nextmeeting-tauri/`.
- Removed `crates/nextmeeting-tauri` from workspace members in `Cargo.toml`.
- Removed stale `.gitignore` entry for deleted Tauri node modules.
- Removed menubar config API from `crates/nextmeeting-client/src/config.rs`:
  - `ClientConfig.menubar`
  - `MenuBarSettings`
  - `MenuBarTitleFormat`
- Removed macOS-specific XDG path overrides in `crates/nextmeeting-client/src/config.rs`.
- Removed `[menubar]` section from `config.example.toml`.
- Updated docs to Linux-only state:
  - `README.md`
  - `DESIGN.md`

### Blocker hit for PR2

The environment cannot resolve/download crates from crates.io (offline DNS failure).  
Required crates for GTK implementation are not present in local cache:
- `gtk4`
- `libadwaita`
- `ksni`
- `glib-build-tools`
- `gtk-blueprint`

Because of this, PR2/PR3 code implementation is blocked in this run.

## Verification Notes (from this run)

- `cargo check --all` passed before removal work.
- `cargo clippy --all --all-features` passed before removal work (with Tauri warnings).
- `cargo test --all` in this environment fails in server socket tests with `PermissionDenied` due sandboxed Unix socket restrictions.

Re-run full verification after PR1 changes on a normal host/CI:

```bash
cargo build --all
cargo clippy --all --all-features --fix
cargo test --all
```

## Next Run: Exact Steps

1. Ensure network access for cargo (or provide vendored dependencies).
2. Add new workspace crate `crates/nextmeeting-gtk4`.
3. Add dependencies:
   - `gtk4`
   - `libadwaita`
   - `glib`
   - `gio`
   - `ksni`
   - build deps `glib-build-tools`, `gtk-blueprint`
4. Implement PR2 MVP:
   - app/window bootstrap
   - tray icon toggle
   - daemon meetings load
   - join/create/refresh
   - dismissals persistence
5. Run:
   - `cargo build --all`
   - `cargo clippy --all --all-features --fix`
   - targeted manual smoke checks (tray toggle, meeting data, actions).
6. Implement PR3 parity:
   - timeline
   - snooze/calendar day/preferences
   - polish and tests.

## Open Risk

- Cargo.lock will change significantly once GTK dependencies are added.
- Final manual verification must be done in a desktop Linux session with a StatusNotifier host.
