# Repository Guidelines

## Project Structure & Module Organization

- Source: `src/nextmeeting/` (CLI entry in `cli.py`).
- Package name: `nextmeeting` (entrypoint `nextmeeting.cli:main`).
- Tests: `tests/` (create `test_*.py` files here).
- Packaging: `packaging/` (AUR, Nix flake, release scripts).
- CI: `.github/workflows/precommit.yml` runs lint/format via pre-commit.

## Build, Test, and Development Commands

- Install deps: `make sync` (uses `uv`); alternatively `uv sync`.
- Run locally: `make run` or `uv run nextmeeting`.
- Lint: `make lint` (ruff + pylint).
- Format: `make format` (ruff format).
- Tests: `make test` or `uv run pytest -sv tests`.
- Coverage: `make coverage` (HTML + terminal report).

## Coding Style & Naming Conventions

- Language: Python 3.9+; 4‑space indentation; use type hints where helpful.
- Naming: modules/functions `snake_case`, classes `PascalCase`, constants `UPPER_SNAKE`.
- Linters: ruff (E,F,D4,PT,PL; ignores E501, PLR0912) and pylint. Keep code ruff/pylint‑clean.
- Formatting: `ruff format` (run via `make format` or pre-commit). No manual line wrapping required.
- Imports: prefer standard → third‑party → local grouping; keep unused imports out.

## Testing Guidelines

- Framework: pytest (with `pytest-cov`). Place tests under `tests/` as `test_*.py`.
- Focus: pure functions/utilities (e.g., time formatting, filters) should have unit tests; mock external commands like `gcalcli`.
- Run: `make test`. For coverage HTML open `htmlcov/index.html` after `make coverage`.

## Commit & Pull Request Guidelines

- Style: Conventional Commits (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`, scopes like `feat(waybar): ...`).
- Commits: small, focused; include rationale when behavior changes.
- PRs: clear description, link issues, note user‑visible changes, add before/after CLI output if relevant. Ensure CI green and pre-commit passes: `pre-commit run -a`.

## Security & Configuration Tips

- External tools: relies on `gcalcli`; ensure OAuth is configured before testing.
- Env/config: `GCALCLI_DEFAULT_CALENDAR` may be set to target a calendar; consider `--google-domain` for workspace URLs.
- Notifications: uses `notify-send` when available; paths and colors are configurable via CLI flags.
- Local setup: `direnv` is optional; `.envrc` adds `.venv/bin` to `PATH`.
