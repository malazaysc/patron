# GitHub Backlog Draft

This document proposes the initial GitHub issue structure for Patron.

Use GitHub issues as the source of truth for project tracking until Patron can eventually manage its own delivery workflow.

## Recommended GitHub Setup

### Labels

- `epic`
- `feature`
- `task`
- `decision`
- `architecture`
- `orchestrator`
- `runner`
- `state-machine`
- `qa`
- `ui`
- `setup`
- `p0`
- `p1`
- `p2`
- `blocked`
- `needs-decision`

### Milestones

- `v1 foundation`
- `v1 task pipeline`
- `v1 qa and visibility`

## Epics

## Epic 1: V1 Foundation

Goal:

Establish the local runtime boundary, Rust application skeleton, SQLite foundation, and core domain model.

Suggested labels:

- `epic`
- `architecture`
- `setup`
- `p0`

Suggested milestone:

- `v1 foundation`

## Epic 2: Deterministic Task Pipeline

Goal:

Implement task intake, planning, state transitions, runner execution, and fix-loop behavior.

Suggested labels:

- `epic`
- `architecture`
- `orchestrator`
- `runner`
- `state-machine`
- `p0`

Suggested milestone:

- `v1 task pipeline`

## Epic 3: QA and Operational Visibility

Goal:

Implement the UI, task detail views, QA execution, evidence capture, and recovery visibility.

Suggested labels:

- `epic`
- `qa`
- `ui`
- `p1`

Suggested milestone:

- `v1 qa and visibility`

## Initial Issues

## Issue 1: Initialize Rust application skeleton and repository structure

Type:

- `feature`

Labels:

- `setup`
- `architecture`
- `p0`

Milestone:

- `v1 foundation`

Description:

Create the initial Rust project structure for a single Axum application and align it with the architecture docs.

Scope:

- create the Rust workspace or single crate structure
- add Axum server bootstrap
- create top-level modules for `app`, `db`, `domain`, `orchestrator`, `runner`, `qa`, and `ui`
- add `.patron/` bootstrap assumptions to setup paths
- keep implementation minimal and compileable

Acceptance criteria:

- the repository contains a buildable Rust app skeleton
- the module layout reflects the architecture docs
- `.patron/` is treated as runtime state and is gitignored
- the app can start with a basic health or placeholder route

## Issue 2: Define `.patron/` runtime layout and bootstrap behavior

Type:

- `feature`

Labels:

- `setup`
- `architecture`
- `p0`

Milestone:

- `v1 foundation`

Description:

Define exactly how Patron initializes and uses `/.patron/` as local runtime state.

Scope:

- define runtime directory contract
- define first-run creation behavior
- define expected subdirectories for tasks, runs, logs, and QA evidence
- define failure behavior when runtime initialization fails

Acceptance criteria:

- the runtime layout is documented in code or design notes
- first run creates required runtime directories if missing
- runtime initialization failures produce clear errors
- no runtime state is written into tracked repo paths

## Issue 3: Design SQLite schema for tasks, runs, transitions, and working artifacts

Type:

- `feature`

Labels:

- `architecture`
- `p0`

Milestone:

- `v1 foundation`

Description:

Design the initial SQLite schema for Patron's core runtime model.

Scope:

- define tables for tasks
- define tables for stage runs
- define tables for state transitions
- define tables for approvals or human actions
- define tables for working artifact references
- define indexes needed for current-state UI queries

Acceptance criteria:

- the schema supports the full v1 task lifecycle
- stage runs and transitions are durable and queryable
- working artifact paths can be resolved from the database
- the schema design is documented and migration-friendly

## Issue 4: Model the task lifecycle and allowed state transitions in code

Type:

- `feature`

Labels:

- `state-machine`
- `architecture`
- `p0`

Milestone:

- `v1 task pipeline`

Description:

Implement the code-level task state machine so transitions are validated by the application, not by prompts.

Scope:

- encode task states
- encode allowed transitions
- encode transition metadata requirements
- define blocked, failed, cancelled, and awaiting-human behavior

Acceptance criteria:

- invalid transitions are rejected by code
- valid transitions require the expected metadata
- the state machine covers all states in the architecture doc
- unit tests verify transition rules

## Issue 5: Implement task creation from a free-form goal

Type:

- `feature`

Labels:

- `orchestrator`
- `p0`

Milestone:

- `v1 task pipeline`

Description:

Create the first task intake flow that turns a user goal into a draft Patron task.

Scope:

- accept a free-form goal
- create a draft task record
- assign a stable task id
- create a task workspace under `/.patron/`
- prepare the initial orchestrator handoff

Acceptance criteria:

- a user can create a draft task from one input
- the task appears in persisted state
- the task has a stable id and runtime workspace
- the flow does not require downstream stages to exist yet

## Issue 6: Implement orchestrator planning output for `task.md`, `plan.md`, and `qa-steps.md`

Type:

- `feature`

Labels:

- `orchestrator`
- `p0`

Milestone:

- `v1 task pipeline`

Description:

Implement the planning stage that produces bounded, human-readable task artifacts for downstream execution.

Scope:

- define the planning prompt contract
- generate `task.md`
- generate `plan.md`
- generate `qa-steps.md`
- validate artifact presence before moving forward

Acceptance criteria:

- planning can run for a draft or approved task
- required planning artifacts are written to `/.patron/`
- `qa-steps.md` contains behavioral scenarios rather than only test commands
- the task transitions cleanly to the next ready state on success

## Issue 7: Implement runner job model and execution wrapper

Type:

- `feature`

Labels:

- `runner`
- `p0`

Milestone:

- `v1 task pipeline`

Description:

Implement the subsystem that executes one stage job with explicit inputs and records the result.

Scope:

- define runner job inputs
- define run start and completion records
- capture logs and exit status
- write runner outputs back to SQLite
- distinguish interruption from failure

Acceptance criteria:

- a runner can execute one stage job at a time
- each run has a durable record before execution begins
- logs and exit state are persisted
- interrupted and failed runs are distinguishable

## Issue 8: Implement development stage execution contract

Type:

- `feature`

Labels:

- `runner`
- `orchestrator`
- `p1`

Milestone:

- `v1 task pipeline`

Description:

Define how the development stage consumes planning artifacts and produces a reviewable output.

Scope:

- define development stage inputs
- define required development outputs
- record stage summaries
- enforce transition readiness for review

Acceptance criteria:

- development runs from planning artifacts rather than free-form memory
- the system can determine whether development completed successfully
- stage outputs are visible in task state and runtime files

## Issue 9: Implement review stage and `review.md`

Type:

- `feature`

Labels:

- `orchestrator`
- `runner`
- `p1`

Milestone:

- `v1 task pipeline`

Description:

Implement the review stage as an explicit pipeline step before QA.

Scope:

- define review stage inputs
- generate `review.md`
- classify findings versus pass
- route failures into `fix_required`

Acceptance criteria:

- review is an explicit stage with persisted output
- findings are recorded in `review.md`
- review can gate progression to QA
- review failures deterministically trigger the fix loop

## Issue 10: Implement fix-loop routing and `fix-log.md`

Type:

- `feature`

Labels:

- `state-machine`
- `runner`
- `orchestrator`
- `p1`

Milestone:

- `v1 task pipeline`

Description:

Implement deterministic fix-loop behavior for failed review or QA stages.

Scope:

- define fix-loop entry conditions
- append to `fix-log.md`
- route the task back to the correct ready state
- preserve failure context for the next development pass

Acceptance criteria:

- review and QA failures can route into one consistent fix loop
- fix context is preserved in structured form
- repeated loops remain understandable in task history

## Issue 11: Build initial Kanban board for task states

Type:

- `feature`

Labels:

- `ui`
- `p1`

Milestone:

- `v1 qa and visibility`

Description:

Build the main visual board that shows tasks by lifecycle state.

Scope:

- render task columns by state
- show task title, id, and current status
- support live refresh or HTMX polling
- support manual reprioritization where appropriate

Acceptance criteria:

- the UI shows tasks grouped by lifecycle state
- current task status updates without full manual inspection of runtime files
- the board makes blocked and awaiting-human tasks obvious

## Issue 12: Build task detail view for logs, artifacts, and stage history

Type:

- `feature`

Labels:

- `ui`
- `p1`

Milestone:

- `v1 qa and visibility`

Description:

Create the task detail page that lets a user inspect what happened without replaying agent conversations.

Scope:

- show current state
- show transition history
- show run logs
- show working artifact links
- show required human actions

Acceptance criteria:

- a user can inspect one task end to end from the UI
- logs and working artifacts are discoverable
- blocked reasons and approvals are clearly visible

## Issue 13: Implement QA runner with Playwright scenario execution

Type:

- `feature`

Labels:

- `qa`
- `runner`
- `p1`

Milestone:

- `v1 qa and visibility`

Description:

Implement the QA stage as browser-driven execution of human-readable scenarios.

Scope:

- load `qa-steps.md`
- execute scenarios with Playwright
- record pass/fail by scenario
- capture screenshots, traces, and logs as needed

Acceptance criteria:

- QA runs from planning-generated scenarios
- scenario results are visible and structured
- required evidence is captured for active tasks
- QA can mark a task ready for PR or route it into fix-required

## Issue 14: Implement `qa-report.md` generation and evidence visibility

Type:

- `feature`

Labels:

- `qa`
- `ui`
- `p1`

Milestone:

- `v1 qa and visibility`

Description:

Make QA results readable to humans and inspectable from the UI.

Scope:

- write `qa-report.md`
- summarize scenario outcomes
- expose screenshots and logs in the task detail view
- make missing evidence visible as a failure condition

Acceptance criteria:

- QA reports explain pass/fail outcomes clearly
- users can inspect QA evidence from the UI
- missing required evidence prevents advancement

## Issue 15: Implement recovery and resume behavior for interrupted stage runs

Type:

- `feature`

Labels:

- `runner`
- `qa`
- `architecture`
- `p1`

Milestone:

- `v1 qa and visibility`

Description:

Implement recovery behavior for Codex disconnects, process interruption, and partial stage output.

Scope:

- detect interrupted runs
- classify interrupted versus failed
- reconstruct next action from SQLite and `/.patron/`
- support stage replay from a clean boundary

Acceptance criteria:

- interrupted runs are visible in state and UI
- the system can determine whether a stage should be retried or blocked
- recovery never depends only on prior chat context

## Issue 16: Implement PR preparation stage and `pr-summary.md`

Type:

- `feature`

Labels:

- `orchestrator`
- `ui`
- `p2`

Milestone:

- `v1 qa and visibility`

Description:

Implement the final pre-human stage that prepares a concise PR-ready summary.

Scope:

- generate `pr-summary.md`
- summarize changes, QA outcome, and open risks
- move the task into an explicit human-review state

Acceptance criteria:

- the system can mark a task ready for human PR review
- a concise PR summary is available in runtime state and UI
- the task lifecycle clearly hands off to the human reviewer

## Issue 17: Decision issue for plan approval policy

Type:

- `decision`

Labels:

- `needs-decision`
- `architecture`
- `p2`

Milestone:

- `v1 task pipeline`

Description:

Decide whether plan approval is mandatory by default or optional in V1.

Decision options:

- require plan approval for every task
- require plan approval only for selected tasks
- do not require plan approval in V1

Acceptance criteria:

- the selected policy is documented
- task lifecycle and UI requirements reflect the decision

Decision:

- do not require plan approval in V1
- planning starts immediately after task intake by default
- future repo-specific policies may add plan approval later without changing the default v1 workflow

## Suggested First Wave

If you want the smallest sensible implementation sequence, open these first:

1. Issue 1
2. Issue 2
3. Issue 3
4. Issue 4
5. Issue 5
6. Issue 6
7. Issue 7

Those seven issues are enough to build the backbone before UI and QA details start branching.
