# V1 Architecture Overview

## Product Intent

Patron is a local-only software delivery autopilot for one repository on macOS.

It is not a generic agent platform, workflow engine, CI/CD system, or cloud orchestrator.

Its job is to move a task through a predictable delivery pipeline with high visibility:

1. Planning
2. Development
3. Review
4. QA testing
5. Fix loops
6. PR preparation

## V1 Design Constraints

- Local-first only
- macOS only
- Single repository only
- Single user only
- Rust backend
- Axum + HTMX + Alpine.js + SortableJS UI
- SQLite state store
- Codex ecosystem only
- Playwright for browser QA

## System Shape

V1 should be a single Rust application with a small number of explicit subsystems.

### 1. Web UI

Purpose:

- Show Kanban board
- Show task state and stage history
- Show logs, artifacts, QA evidence, and blocked reasons
- Show approval requests and next human action

Implementation:

- Axum server
- HTMX for server-driven updates
- Alpine.js for local interaction
- SortableJS for task ordering and manual reprioritization

### 2. API and Application Layer

Purpose:

- Receive user actions from the UI
- Validate transitions
- Persist state changes
- Schedule stage execution

Notes:

- Keep API thin and internal-facing
- Favor synchronous command handling plus background job dispatch

### 3. Orchestrator

Purpose:

- Convert goals into structured tasks
- Produce deterministic stage instructions
- Decide the next valid stage
- Enforce approval gates and transition rules

Notes:

- The orchestrator does not directly “do the work”
- It produces bounded work packages for the runner

### 4. Runner

Purpose:

- Execute one stage job at a time
- Call Codex with the exact prompt and context for that stage
- Capture outputs, logs, exit state, and artifacts

Notes:

- A runner owns execution, not planning
- A failed runner run must be recoverable and replayable

### 5. Task State Machine

Purpose:

- Define the only legal lifecycle transitions
- Prevent hidden or ad hoc workflow jumps

Notes:

- State transitions happen in application code, never only in prompts

### 6. Working Artifact Store

Purpose:

- Store stage documents and evidence needed for active execution
- Make each stage readable without replaying long chats

Storage shape:

- Files on disk under `/.patron/` at the repository root
- Paths indexed in SQLite while the task is active

Notes:

- These are working artifacts, not a permanent historical archive
- Long-term records should primarily live in git history, PR descriptions, PR conversations, and issue tracking
- Completed task workspaces may be pruned or archived later
- Working artifacts should not be committed to the repository by default
- Repository setup should add `.patron/` to `.gitignore`

### 7. SQLite State Store

Purpose:

- Store tasks, stages, runs, approvals, artifacts, and event history

Notes:

- SQLite is enough for V1
- Prefer append-only event records plus current-state tables

### 8. QA Harness

Purpose:

- Execute human-readable QA scenarios
- Capture screenshots, traces, logs, and pass/fail judgments

Notes:

- QA is behavior-first, not only test-command-first
- Playwright should be a visible subsystem, not a hidden post-step

## Suggested V1 Directory Model

```text
docs/
  architecture/
  prd/
  adr/
src/
  app/
  db/
  domain/
  orchestrator/
  runner/
  qa/
  ui/
```

## Suggested Runtime Layout

V1 runtime state should live in `/.patron/`, untracked by git and separate from product source files.

```text
.patron/
  state.db
  runs/
  tasks/
    TASK-0001/
      task.md
      plan.md
      qa-steps.md
      review.md
      qa-report.md
      fix-log.md
      pr-summary.md
      artifacts/
        screenshots/
        traces/
        logs/
```

Recommended rule:

- tracked repo paths contain source, docs, and skills
- `.patron/` contains SQLite, logs, task workspaces, and QA evidence
- `.patron/` is ignored by git by default

## Task Lifecycle

Use explicit states with narrow meanings.

### Core States

1. `draft`
2. `ready_for_planning`
3. `planning`
4. `ready_for_development`
5. `developing`
6. `ready_for_review`
7. `reviewing`
8. `ready_for_qa`
9. `qa_running`
10. `fix_required`
11. `ready_for_pr`
12. `pr_prepared`
13. `awaiting_human`
14. `done`
15. `blocked`
16. `failed`
17. `cancelled`

### Allowed High-Level Transitions

- `draft -> ready_for_planning`
- `ready_for_planning -> planning`
- `planning -> ready_for_development`
- `ready_for_development -> developing`
- `developing -> ready_for_review`
- `ready_for_review -> reviewing`
- `reviewing -> ready_for_qa`
- `reviewing -> fix_required`
- `ready_for_qa -> qa_running`
- `qa_running -> ready_for_pr`
- `qa_running -> fix_required`
- `fix_required -> ready_for_development`
- `ready_for_pr -> pr_prepared`
- `pr_prepared -> awaiting_human`
- `awaiting_human -> done`
- `* -> blocked`
- `blocked -> previous_ready_state`
- `* -> failed`
- `* -> cancelled`

### State Rules

- Only one active stage execution per task
- Every transition must record an actor, timestamp, reason, and run id if applicable
- `blocked` must always carry a machine-readable reason code and human-readable explanation
- `awaiting_human` must always identify the required action

Note:

- The example above shows the maximum common task workspace shape, not a requirement that every task always materializes every file
- The task workspace lives in runtime state, not in tracked repository content

## Repository Setup

Minimum setup should:

1. Create `/.patron/` if it does not exist
2. Add `.patron/` to `.gitignore`
3. Initialize runtime subdirectories and SQLite on first run

## Orchestrator Responsibilities

- Convert user goals into scoped tasks
- Write `task.md` with problem statement, constraints, acceptance criteria, and dependencies
- Produce `plan.md` with deterministic implementation steps
- Produce `qa-steps.md` with human-readable scenarios
- Decide stage readiness based on required artifacts and prior outcomes
- Create runner job inputs for each stage
- Route failures into explicit fix loops
- Detect when human approval is required
- Never directly mutate code or run QA tools

## Runner Responsibilities

- Execute exactly one stage job with explicit inputs
- Materialize a clean task workspace
- Invoke Codex with stage-specific prompts and references
- Capture stdout, stderr, timestamps, exit results, and artifact paths
- Detect interruption versus failure versus invalid output
- Publish stage result summaries back to SQLite
- Never invent task transitions on its own

## Skills System

Patron should reuse Codex-native skills as the execution guidance layer.

V1 should not introduce a separate skill runtime, skill DSL, or competing skill abstraction.

Patron's job is to select, compose, and supply the right existing skills and repo-specific operational documents for a given stage.

### Skill Categories

- `planning`
- `development`
- `review`
- `qa`
- `repo-ops`

### Skill Design Rules

- Patron uses Codex-style skills stored on disk
- Skills and repo operational docs are tracked repository inputs, not runtime state
- Skills are selected deterministically by stage and task type, not free-form agent preference
- Patron does not invent a separate runtime skill model in V1
- Skills must describe inputs, outputs, and failure conditions
- Repo-specific operational docs should augment skills, not replace them

### Recommended V1 Structure

```text
skills/
  planning/
    task-intake.md
    acceptance-criteria.md
    qa-scenario-writer.md
  development/
    rust-feature-delivery.md
    ui-htmx-flow.md
  review/
    code-review.md
  qa/
    browser-scenario-execution.md
    evidence-capture.md
  repo-ops/
    branch-and-pr-summary.md
```

## Recovery and Resume Behavior

Recovery must be explicit and working-artifact-driven.

### Rules

- Every stage run gets a durable run record before execution starts
- Every stage run writes to a task-local working directory
- Partial outputs are preserved long enough for recovery, not discarded immediately
- A resumed run never depends on chat memory alone

### Resume Strategy

1. Load task state, last completed stage, and latest in-progress run
2. Inspect run heartbeat, exit status, and artifact completeness
3. Mark run as `interrupted`, `failed`, or `completed`
4. Reconstruct the next action from persisted artifacts
5. Requeue the stage or move to `blocked` if human intervention is required

### Interruption Types

- Codex disconnected
- Internet unavailable
- Local process terminated
- QA browser run incomplete
- Working artifact write partial

### V1 Principle

Prefer replaying a stage from its last clean boundary over trying to continue arbitrary partial execution.

## QA Workflow

QA is a first-class pipeline stage, not a side effect.

### Planning Output

Planning must produce:

- user-facing scenarios
- setup/preconditions
- expected behaviors
- negative cases where relevant
- evidence requirements

### QA Execution

1. Load `qa-steps.md`
2. Launch the target app locally
3. Execute scenarios with Playwright/browser automation
4. Capture screenshots, traces, logs, and final assertions
5. Write `qa-report.md`

### QA Result Rules

- Pass only when behavior matches expected outcomes
- Fail when a required scenario is unexecuted
- Fail when evidence is missing for a required scenario
- On failure, produce concrete reproduction steps and route to `fix_required`

### Retention

- QA evidence must exist while the task is active and reviewable
- V1 does not require indefinite retention of screenshots, traces, or task-local QA reports after completion
- The durable historical record should be the PR and tracking system, not a forever-growing local evidence archive

## Human Gates

V1 should keep these approval points:

- Task creation or task approval
- Optional plan approval
- PR review and merge outside the system
- Manual unblock decisions

Avoid adding more gates unless they materially increase trust.

## Strong V1 Scope Cuts

Do not include in V1:

- multi-repo support
- distributed workers
- multi-user auth
- cloud execution
- provider abstraction
- autonomous merge
- generalized plugin ecosystem
- dynamic DAG workflow composition

## Recommended V1 Build Order

1. Task model and SQLite schema
2. Artifact directory model
3. Task state machine
4. Orchestrator prompt and document generation
5. Runner execution wrapper
6. Kanban and task detail UI
7. QA scenario execution and evidence capture
8. Recovery and resume logic
