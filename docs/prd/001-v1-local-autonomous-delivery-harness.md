# PRD 001: Local-First Autonomous Software Delivery Harness

Status: Draft

## Problem Statement

Software delivery with Codex is powerful but still too chat-driven, manual, and fragile for sustained repository work.

There is no practical local harness that turns goal discussion into a visible, recoverable software delivery pipeline with deterministic stages, structured working artifacts, and behavior-focused QA.

## Product Goal

Build a local-only autopilot for a single repository on macOS that helps a human move work from task definition to PR-ready output through a deterministic workflow:

- planning
- development
- review
- QA testing
- fix loops
- PR preparation

The system should feel like a visual autopilot for software delivery, not an opaque agent swarm.

## Non-Goals

- multi-repository orchestration
- distributed execution
- cloud workers
- multi-user collaboration
- generalized workflow engine
- provider abstraction
- autonomous merge
- generic AI operating system ambitions

## Primary User Workflow

1. User discusses a goal with the orchestrator
2. Orchestrator converts it into a structured task
3. User creates or approves the task
4. System runs planning and generates artifacts
5. System runs development
6. System runs review
7. System runs QA scenarios with browser automation and evidence capture
8. If QA or review fails, system enters a deterministic fix loop
9. System prepares a PR summary and marks the task ready for human review
10. User reviews the PR and merges outside the system

## Success Criteria

- A user can create and track tasks from one UI
- Every task stage is visible with logs and active working artifacts
- Every task has readable planning and QA documents
- QA produces evidence, not only command output
- Interrupted runs can be resumed or replayed without losing context
- Human-required actions are explicit and easy to find

## Functional Requirements

### Task Intake

- The system must let the user create a task from a free-form goal
- The orchestrator must write `task.md`
- `task.md` must include scope, constraints, acceptance criteria, dependencies, and human approvals

### Planning

- Planning must write `plan.md`
- Planning must write `qa-steps.md`
- Planning must define behavioral scenarios, not only test commands

### Development

- Development must operate from planning artifacts
- Development must produce code changes plus a stage summary

### Review

- Review must produce `review.md`
- Review must identify findings, risks, and missing validation

### QA

- QA must execute scenarios from `qa-steps.md`
- QA must use Playwright or browser automation
- QA must capture screenshots and logs
- QA must write `qa-report.md`

### Fix Loop

- QA or review failures must produce explicit fix inputs
- Fix work must append to `fix-log.md`
- The task must re-enter development from a controlled boundary

### PR Preparation

- The system must write `pr-summary.md`
- The system must provide the user with the task outcome, artifacts, and approval status

### Artifact Retention

- The system must create task-local working artifacts only when needed by a stage
- The system must retain working artifacts while a task is active, blocked, or awaiting review
- The system does not need to retain all task-local artifacts indefinitely after completion
- The system may rely on the pull request, repository history, and external project tracking tools for long-term historical context
- Active task working artifacts must live under `/.patron/`
- Repository setup must ensure `.patron/` is gitignored

## UX Requirements

- Show a Kanban board with task states
- Show task detail pages with stage history
- Show run logs, artifacts, and QA evidence
- Show blocked states and required human actions prominently
- Support manual reprioritization of queued tasks

## System Constraints

- macOS only
- one repository only
- `/.patron/` for active task working artifacts
- SQLite for state
- Rust backend
- Axum + HTMX + Alpine.js UI
- Codex ecosystem only

## Task Lifecycle Requirements

- The system must enforce explicit allowed state transitions
- The system must allow only one active execution per task
- The system must record all transitions durably
- The system must distinguish `blocked`, `failed`, and `awaiting_human`

## QA and Evidence Requirements

- QA scenarios must be readable by a human
- Each required scenario must have evidence requirements
- A task may not be marked ready for PR if required QA evidence is missing
- QA reports must include pass/fail reasoning and reproduction notes on failure

## Operational Visibility Requirements

- Every stage run must have logs
- Every active working artifact must have a known path
- The UI must show the latest run status without reading raw terminal state
- The system must preserve stage history for auditability

## Failure and Recovery Requirements

- The system must tolerate Codex disconnects
- The system must tolerate internet interruptions
- The system must preserve partial working artifacts after interrupted runs
- The system must support replaying a stage from its last clean boundary

## Out of Scope for V1

- background parallelism across many tasks
- auto-merge
- branch protection integration
- remote access workflows
- team permissions
- cross-repo dependencies

## Remaining Questions

- Should review be implemented as a Codex stage before QA in all cases?
- Should PR creation itself be automated in V1 or only the PR summary?

## Acceptance Checklist

- A task can be created from a free-form goal
- The system writes the required working artifacts for each executed stage
- The task lifecycle is enforced by code
- QA runs from human-readable scenarios
- QA evidence is visible in the UI
- Failed QA can route into a fix loop
- The user can clearly see when a task is ready for PR review
