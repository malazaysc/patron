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
3. Run `patron doctor`.
4. Run `patron init`.
5. Start Patron with `patron serve`.
6. Open `http://127.0.0.1:3000`.
7. Confirm the dashboard shows the correct repository name, branch, and runtime root.
8. Open `/sample-app` and confirm the sample application renders.
9. Create one task from the sample app task pack.
10. Run the task through planning, development, review, QA, and PR preparation as far as the current implementation allows.
11. Inspect the task detail page.
12. Confirm artifacts, logs, state history, and QA evidence are visible.

## Expected Results

- `/.patron/` exists at the repository root
- `state.db` exists under `/.patron/`
- the setup screen disappears after successful init
- dashboard repository context matches the active repository
- sample app route loads successfully
- task workspaces are created under `/.patron/tasks/`
- stage runs are created under `/.patron/runs/`
- QA captures `qa-report.md`, screenshot, HAR, and QA log
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
