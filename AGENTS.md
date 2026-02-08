## Project

See README.md for project overview and goals, if needed.

## Building

- Don't ever do commit unless you are being explicitely asked for it.
- If you get asked to commit then use this rules:
  - Follow Conventional Commits 1.0.0.
  - 50 chars for title 70 chars for body.
  - Cohesive long phrase or paragraph unless multiple points are needed.
  - Use bullet points only if necessary for clarity.
  - Past tense.
  - State **what** and **why** only (no “how”).

## Documentation

- For any user-facing changes (features, options, keybindings, etc.), ensure you update:
  - `README.md`
- Repository architecture notes, implementation internals, and project conventions are maintained in `DESIGN.md`.
- Documentation and help string style guidelines:
  - Consistent British spelling.
  - Professional butler style: clear, helpful, dignified but not pompous
  - Remove any overly casual Americanisms
  - Keep technical precision whilst maintaining readability

## Before Finishing

- Always Run `cargo clippy --all --all-features --fix` which will run `golangci-lint`, `gofumpt`, and `go test`.
- Add tests for any new functionality.
- Make sure coverage is top notch
