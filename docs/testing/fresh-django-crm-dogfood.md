# Fresh Django CRM Dogfood

## Goal
Validate Patron against a brand new repository by defining and driving a small CRM built with Django, HTMX, Alpine.js, and SQLite.

## Test Repository
- Path: `/private/tmp/patron-crm-test`
- Initialization:
  - `git init`
  - `patron init`
  - `patron serve`

## What Worked
- Patron intake created repo-scoped tasks and planning artifacts in a fresh repository.
- Repo-aware planning produced targeted `task.md`, `plan.md`, and `qa-steps.md` for the CRM.
- Real Codex-driven development created an actual Django project and CRM application in the target repository.
- Development now captures:
  - runner logs
  - Codex response output
  - changed-file lists
  - diff stats
  - diff patches
  - before/after repo status artifacts
- Review now consumes real repository outputs instead of generic summaries.
- QA started the Django target app from the fresh repo, captured browser evidence, and produced `qa-report.md`.
- PR preparation completed and moved the validated task into `awaiting_human`.

## Real Delivery Slice Completed
- Task: `TASK-0004`
- Goal: add a summary banner to the CRM contact index page
- Completed stages:
  - planning
  - development
  - review
  - QA
  - PR preparation

## Evidence
- Task workspace: `/private/tmp/patron-crm-test/.patron/tasks/TASK-0004`
- QA screenshot: `/private/tmp/patron-crm-test/.patron/qa/screenshots/TASK-0004-qa-001-board.png`
- QA HAR: `/private/tmp/patron-crm-test/.patron/qa/traces/TASK-0004-qa-001.har`
- QA logs:
  - `/private/tmp/patron-crm-test/.patron/qa/logs/TASK-0004-qa-001.log`
  - `/private/tmp/patron-crm-test/.patron/qa/logs/TASK-0004-qa-001-startup.log`

## What Failed Or Needed Fixes
- The first real development run used an outdated `codex exec` flag and failed.
- Early repo snapshots collapsed untracked directories like `crm/` instead of recording file-level changes.
- Early diff capture missed untracked file patches, which caused false review findings.
- Development failure handling originally left tasks stranded in `developing`; this was updated to return failed runs to `ready_for_development` for retry.

## Current Confidence
- Patron is now credible for:
  - fresh-repo intake
  - repo-aware planning
  - real Codex-driven development
  - diff-aware review
  - target-app QA
  - PR handoff preparation
- Patron still needs more hardening for:
  - repeated failure-path validation
  - cleaner handling of stale interrupted runs from older builds
  - larger multi-step implementation slices

## Next Hardening Work
- Add stronger timeout and heartbeat signals for long-running Codex stages.
- Improve stale-run recovery for tasks left mid-stage by older builds.
- Reduce noisy review diffs when a task operates on a repo with many pre-existing untracked files.
