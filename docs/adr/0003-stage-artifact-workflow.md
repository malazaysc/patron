# ADR 0003: Stage-Artifact Workflow

Status: Accepted

## Context

Long conversational context is fragile, difficult to inspect, and hard to recover after interruptions.

At the same time, permanent retention of stage documents and QA evidence for every completed task would create unnecessary file sprawl in a real product with many tasks.

## Decision

Each task stage may produce human-readable working artifacts on disk when needed for execution, review, QA, or recovery.

These artifacts are operational and short-lived, not permanent records.

SQLite stores task metadata, current state, run status, and pointers to active working artifacts while a task is in flight.

Long-term history should live in the repository, pull request description, pull request conversation, and external tracking tools rather than in indefinitely retained task-local markdown files.

The primary V1 working artifacts are:

- `task.md`
- `plan.md`
- `qa-steps.md`
- `review.md`
- `qa-report.md`
- `fix-log.md`
- `pr-summary.md`

These files are created only when needed by a stage.

After task completion, the system may prune or archive task-local working artifacts because they are not the primary long-term audit store.

## Consequences

### Positive

- easier review by humans during execution
- smaller prompt contexts for later stages
- better recovery after disconnects
- less dependence on chat memory while a task is active

### Negative

- some file-management overhead while tasks are active
- less self-contained local history after task completion

## Rationale

Stage artifacts are a practical trust boundary during active execution, but they should not become a permanent document archive for every historical task.
