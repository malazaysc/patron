# Sample App Task Pack

These tasks are the recommended first dogfooding tasks for Patron's built-in sample app at `/sample-app`.

Use them to verify that Patron can plan, execute, review, QA, and hand off work against a target application that is not Patron itself.

## Task 1

Title:

- `Sample App: add a visible queue summary note`

Suggested goal:

```text
Sample App: add a visible queue summary note under the Support Triage Board title that explains what the two control buttons do.
```

Acceptance criteria:

- the sample app shows a short explanatory note near the controls
- the note is visible on page load without interaction
- the note fits the existing visual style

QA focus:

- sample app loads successfully
- note is visible on the page
- browser evidence is captured against `/sample-app`

## Task 2

Title:

- `Sample App: resolve review should update the queue message`

Suggested goal:

```text
Sample App: when the Resolve Review button is clicked, the page should clearly show that there are no items left needing review.
```

Acceptance criteria:

- clicking `Resolve Review` changes the visible review count to `0`
- the page also shows a human-readable message that the review queue is clear
- existing layout remains intact

QA focus:

- sample app loads successfully
- the visible review state changes correctly
- browser evidence is captured against `/sample-app`

## Task 3

Title:

- `Sample App: add a fix-loop regression on ticket creation`

Suggested goal:

```text
Sample App: intentionally introduce and then detect a regression where Add Ticket fails to increment the open ticket count, so the QA flow can demonstrate a deterministic fix loop.
```

Acceptance criteria:

- the failing behavior is detectable by QA
- QA routes the task into `fix_required`
- a follow-up development pass can repair the broken increment behavior
- the repaired task can return to the review and QA path cleanly

QA focus:

- target-app QA detects the broken behavior
- evidence is preserved in `qa-report.md`, screenshot, HAR, and QA log
- fix loop re-enters development deterministically
