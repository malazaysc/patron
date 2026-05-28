# New Repo Smoke Test

This checklist defines the minimum credible first-run evaluation for Patron in a brand new repository.

## Preconditions

- the repository is a git repository
- Rust toolchain is installed
- `npx` is available if QA evidence capture will be tested
- Patron is run from the repository root

## Steps

1. Clone or create a fresh repository and copy Patron into it, or run Patron from the Patron repository for the first smoke test.
2. Run `cargo run -- init`.
3. Start Patron with `cargo run`.
4. Open `http://127.0.0.1:3000`.
5. Confirm the dashboard shows the correct repository name, branch, and runtime root.
6. Open `/sample-app` and confirm the sample application renders.
7. Create one task from the sample app task pack.
8. Run the task through planning, development, review, QA, and PR preparation as far as the current implementation allows.
9. Inspect the task detail page.
10. Confirm artifacts, logs, state history, and QA evidence are visible.

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
