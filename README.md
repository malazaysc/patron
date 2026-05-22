# Patron

Patron is a local-first autonomous software delivery harness for a single repository on macOS.

It is intentionally narrow in scope:

- local-only
- single repository
- Rust backend
- Axum + HTMX UI
- SQLite state
- Codex-driven workflow execution
- Playwright-centered QA later in the pipeline

The goal is not to build a generic agent platform or cloud orchestrator. The goal is to build a practical, observable, deterministic engineering autopilot that can move a task through:

1. planning
2. development
3. review
4. QA testing
5. fix loops
6. PR preparation

## Current Status

The repository is in the foundation stage. The current implementation includes:

- a Rust Axum app scaffold
- local runtime bootstrap under `/.patron/`
- an initial SQLite schema
- a typed task lifecycle state machine
- draft task intake
- planning artifact generation for `task.md`, `plan.md`, and `qa-steps.md`

## Runtime Model

Patron keeps operational state in `/.patron/`, not in tracked repo content.

That runtime area holds:

- `state.db`
- task workspaces
- stage run logs
- QA evidence

`/.patron/` is gitignored by default.

## Docs

- [Docs Index](/Users/malazay/dev/patron/docs/README.md)
- [Architecture Overview](/Users/malazay/dev/patron/docs/architecture/v1-overview.md)
- [Runtime Layout](/Users/malazay/dev/patron/docs/architecture/runtime-layout.md)
- [SQLite Schema](/Users/malazay/dev/patron/docs/architecture/sqlite-schema.md)
- [V1 PRD](/Users/malazay/dev/patron/docs/prd/001-v1-local-autonomous-delivery-harness.md)
- [GitHub Backlog Draft](/Users/malazay/dev/patron/docs/planning/github-backlog.md)

## Development

Current local commands:

```bash
cargo run
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

The app starts on `http://127.0.0.1:3000`.

## Git Hooks

The repo uses `lefthook` for pre-commit enforcement.

Once `lefthook` is installed locally, run:

```bash
lefthook install
```

The pre-commit hook runs:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
