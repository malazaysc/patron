# Runtime Layout

## Purpose

Patron stores local runtime state under `/.patron/` at the repository root.

This directory is operational state, not source code, and must remain untracked by git.

## Contract

Patron should treat the following layout as the V1 runtime contract:

```text
.patron/
  state.db
  logs/
  runs/
  tasks/
  qa/
    logs/
    screenshots/
    traces/
```

## First-Run Behavior

On startup, Patron should:

1. resolve the repository-local `/.patron/` root
2. create any missing runtime directories
3. create `state.db` if it does not already exist
4. fail fast with a clear error if runtime initialization cannot complete

## Rules

- No runtime state should be written into tracked repository paths
- Task working files should live under `/.patron/tasks/`
- Run metadata and transient execution outputs should live under `/.patron/runs/`
- General runtime logs should live under `/.patron/logs/`
- QA evidence should live under `/.patron/qa/`

## Failure Behavior

Runtime initialization errors should include the path that failed and whether the problem occurred while creating a directory or the SQLite file.
