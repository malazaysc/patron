# SQLite Schema

## Purpose

Patron uses SQLite as the local system of record for active task state.

The goal of the initial schema is to support:

- task lifecycle tracking
- stage execution history
- human approval gates
- working artifact lookup
- efficient UI queries for current state

## Migration Strategy

V1 starts with tracked SQL migrations in [src/db/migrations](/Users/malazay/dev/patron/src/db/migrations).

The initial schema is:

- [0001_initial.sql](/Users/malazay/dev/patron/src/db/migrations/0001_initial.sql)

The current schema version is also exposed in [src/db/schema.rs](/Users/malazay/dev/patron/src/db/schema.rs).

## Tables

### `tasks`

Stores the current task record and its current lifecycle state.

Important fields:

- `id`
- `title`
- `goal`
- `state`
- `current_stage`
- `blocked_reason_code`
- `blocked_reason_text`
- timestamps for creation, update, completion, and cancellation

Why it exists:

- powers Kanban and task detail views
- gives one current row per task

### `stage_runs`

Stores execution attempts for pipeline stages.

Important fields:

- `task_id`
- `stage`
- `status`
- `attempt_number`
- `runner_kind`
- `prompt_fingerprint`
- `started_at`
- `finished_at`
- `exit_code`
- `error_summary`

Why it exists:

- tracks deterministic execution attempts
- distinguishes completed, failed, interrupted, and cancelled runs

### `state_transitions`

Append-only history of task lifecycle movement.

Important fields:

- `from_state`
- `to_state`
- `actor_kind`
- `actor_id`
- `reason_code`
- `reason_text`
- `stage_run_id`
- `created_at`

Why it exists:

- preserves a durable audit trail
- supports recovery and operator visibility

### `human_actions`

Tracks explicit human gates and required interventions.

Important fields:

- `action_type`
- `status`
- `requested_by`
- `instructions`
- `requested_at`
- `resolved_at`

Why it exists:

- makes waiting-on-human states queryable
- separates approvals from implicit comments or chat history

### `working_artifacts`

Tracks active task-local documents and evidence under `/.patron/`.

Important fields:

- `artifact_kind`
- `role`
- `relative_path`
- `media_type`
- `required_for_stage`
- `stage_run_id`

Why it exists:

- resolves active files without scanning the filesystem
- supports task detail views and QA evidence lookup

## Indexes

The initial schema includes indexes for the main V1 UI and recovery paths:

- tasks by `state` and `updated_at`
- stage runs by `task_id` and `started_at`
- stage runs by `status` and `stage`
- transitions by `task_id` and `created_at`
- human actions by `status` and `requested_at`
- working artifacts by `task_id` and `role`
- working artifacts by `stage_run_id` and `artifact_kind`

## Query Model

The intended split is:

- current task state comes from `tasks`
- historical state movement comes from `state_transitions`
- execution history comes from `stage_runs`
- required human intervention comes from `human_actions`
- task-local document and evidence lookup comes from `working_artifacts`

## Notes

- This schema is intentionally single-repo and single-user scoped
- Working artifacts are indexed while tasks are active; they are not treated as permanent archives
- State transitions remain append-only even when task rows are updated in place
