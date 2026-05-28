# New Repo Smoke Test

This checklist defines the minimum credible first-run evaluation for Patron in a brand new repository.

## Preconditions

- the repository is a git repository
- Rust toolchain is installed
- `npx` is available if QA evidence capture will be tested
- Patron is run from the repository root

## Steps

1. Install Patron with `cargo install --path /path/to/patron`.
2. Clone or create a fresh repository and `cd` to its root.
3. Run `git init` if the repository was not already initialized.
4. Run `patron doctor`.
5. Run `patron init`.
6. Start Patron with `patron serve`.
7. Open `http://127.0.0.1:3000`.
8. Confirm the dashboard shows the correct repository name, branch, and runtime root.
9. Open `/intake` and confirm the orchestrator task-definition console renders.
10. Open `/sample-app` and confirm the sample application renders.
11. Start one intake session from the sample app task pack.
12. Confirm Patron either asks a focused follow-up question or generates a draft immediately.
13. Approve the draft into a real task.
14. Run the task through planning, development, review, QA, and PR preparation as far as the current implementation allows.
15. Inspect the task detail page.
16. Confirm artifacts, logs, state history, activity feed, and QA evidence are visible.

## Expected Results

- `/.patron/` exists at the repository root
- `state.db` exists under `/.patron/`
- the setup screen disappears after successful init
- dashboard repository context matches the active repository
- the orchestrator intake console is available at `/intake`
- sample app route loads successfully
- task workspaces are created under `/.patron/tasks/`
- stage runs are created under `/.patron/runs/`
- QA captures `qa-report.md`, screenshot, HAR, and QA log
- intake sessions and activity events are persisted in SQLite
- blocked or fix-loop states are clearly visible when triggered

## Failure Reporting

Record any failure under one of these categories:

- setup failure
- repository detection failure
- runtime bootstrap failure
- task lifecycle failure
- runner execution failure
- QA evidence failure
- UI visibility failure

For each failure capture:

- exact command used
- page or stage where the failure happened
- visible error message
- expected behavior
- actual behavior
