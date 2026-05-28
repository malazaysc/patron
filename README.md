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

Patron is currently a working local prototype with:

- explicit runtime initialization
- repository-aware startup checks
- a local Axum UI with dashboard, orchestrator intake, board, tasks, runs, and detail pages
- SQLite-backed task, run, transition, artifact, and human-action state
- persistent orchestrator intake sessions, follow-up prompts, draft approval, and activity events
- planning, development, review, QA, fix-loop, and PR-preparation stages
- a built-in sample app route for dogfooding

It is not yet a general-purpose autonomous code delivery system for arbitrary repos. The best current use is evaluating the harness flow itself and testing it against the built-in sample app.

## Prerequisites

- macOS
- Rust toolchain
- `git`
- `npx`

`npx` is only required for Playwright-backed QA evidence capture. Patron can still start without it, but QA capture will be flagged as unavailable.

## Getting Started

Run Patron from the root of a git repository.

If this is a brand new project directory, you can either run:

```bash
git init
patron init
```

or let Patron do the safe explicit git setup step for you:

```bash
patron init --git
```

### 1. Install Patron

From the Patron repository:

```bash
cargo install --path .
```

Then verify the CLI is available:

```bash
patron --help
patron --version
```

### 2. Initialize Patron

```bash
patron init
```

This creates `/.patron/` at the repository root and initializes the SQLite runtime state.

For a brand new non-git directory, use:

```bash
patron init --git
```

Patron will only run `git init` when you explicitly opt into it with `--git`.

You can inspect readiness without mutating state:

```bash
patron doctor
```

### 3. Start Patron

```bash
patron serve
```

The app starts on:

```text
http://127.0.0.1:3000
```

If setup is incomplete, Patron will show a setup screen instead of the normal dashboard.

### 4. Open the Built-In Sample App

After startup, open:

```text
http://127.0.0.1:3000/sample-app
```

This tiny app is the recommended first target for dogfooding Patron’s task and QA flow.

### 5. Define a Sample Task

Use one of the goals from:

- [Sample App Task Pack](/Users/malazay/dev/patron/docs/testing/sample-app-task-pack.md)

Open `/intake`, define the work through the orchestrator console, approve the draft, and then move the resulting task through the visible pipeline inside Patron.

## Runtime Model

Patron keeps operational state in `/.patron/`, not in tracked repo content.

That runtime area holds:

- `state.db`
- task workspaces
- stage run logs
- QA evidence

`/.patron/` is gitignored by default.

## First External Test

Use:

- [New Repo Smoke Test](/Users/malazay/dev/patron/docs/testing/new-repo-smoke-test.md)

That checklist defines what “ready to test on a new repository” means for the current prototype.

## Docs

- [Docs Index](/Users/malazay/dev/patron/docs/README.md)
- [Architecture Overview](/Users/malazay/dev/patron/docs/architecture/v1-overview.md)
- [Runtime Layout](/Users/malazay/dev/patron/docs/architecture/runtime-layout.md)
- [SQLite Schema](/Users/malazay/dev/patron/docs/architecture/sqlite-schema.md)
- [V1 PRD](/Users/malazay/dev/patron/docs/prd/001-v1-local-autonomous-delivery-harness.md)
- [GitHub Backlog Draft](/Users/malazay/dev/patron/docs/planning/github-backlog.md)
- [New Project Readiness Backlog](/Users/malazay/dev/patron/docs/planning/new-project-readiness-backlog.md)

## Development

Current local commands:

```bash
patron init
patron doctor
patron serve
cargo run -- init
cargo run
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Git Hooks

The repo uses `lefthook` for pre-commit enforcement.

Once `lefthook` is installed locally, run:

```bash
lefthook install
```

The pre-commit hook runs:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
