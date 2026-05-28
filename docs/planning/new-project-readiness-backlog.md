# New Project Readiness Backlog

This document defines the next execution wave for Patron: make it testable on a brand new repository with a credible first-run experience.

This is not a multi-project plan and not a platform expansion plan.

The goal is narrower:

- a developer can open a fresh repository
- initialize Patron with minimal friction
- understand what Patron expects
- create a task
- run the visible workflow
- see deterministic outputs against that repository
- evaluate where the harness is real vs scaffolded

## Readiness Definition

Patron is ready for a real new-project test when all of the following are true:

- a fresh repository can be bootstrapped without reading source code
- Patron has an explicit initialization flow instead of relying on implicit first-run behavior
- the UI clearly shows the current repository context and runtime status
- task execution is explicitly repo-aware
- a sample target application can be used to validate the delivery flow end to end
- setup failures and missing dependencies are shown clearly
- the README documents the exact first-run workflow

## Scope For This Wave

### In Scope

- explicit repository bootstrap
- first-run setup and validation
- runtime health checks
- clearer setup and usage documentation
- repository-aware execution contracts
- a tiny sample app for dogfooding
- a realistic smoke-test checklist for a new repository

### Out Of Scope

- multi-project support
- global project registry
- distributed execution
- cloud setup
- generalized plugin systems
- autonomous code generation for arbitrary repos without guardrails

## Delivery Phases

### Phase 1: Onboarding

Make first use obvious and deterministic.

- add a `patron init` command or equivalent explicit setup flow
- create `/.patron/` only through a visible bootstrap path
- validate runtime prerequisites before the server starts
- show setup guidance when prerequisites are missing

### Phase 2: Repository Awareness

Make it obvious which repository Patron is operating on.

- display repo root and current git branch in the UI
- store repo metadata in runtime state
- make task execution reference the active repository intentionally
- add guardrails when Patron is launched outside a git repository

### Phase 3: Realistic Testing Surface

Create one small target app that Patron can use as a proving ground.

- add a tiny sample application fixture
- define a representative task pack against that app
- verify planning, development, review, QA, fix loop, and PR prep against it
- document what is still simulated vs truly repo-changing

### Phase 4: First External Test

Turn the above into a clear operator checklist.

- define a brand new repository smoke test
- define expected artifacts and visible state changes
- define failure-reporting expectations
- capture feedback from the first external test run

## Proposed GitHub Milestone

- `new project readiness`

## Proposed Epic

### Epic: Make Patron Testable On A Brand New Repository

Goal:

Close the gap between “local prototype” and “credible first-use harness” by adding explicit setup, repository awareness, and one realistic dogfooding target.

Suggested labels:

- `epic`
- `setup`
- `architecture`
- `runner`
- `qa`
- `p0`

## Proposed Issues

### Issue 1: Add explicit Patron initialization flow

Why:

Today Patron relies on implicit bootstrap behavior. A new user should not have to infer how startup works.

Acceptance criteria:

- Patron supports an explicit initialization flow such as `patron init`
- initialization creates `/.patron/` and required runtime directories
- initialization is safe to rerun
- initialization reports what it created

### Issue 2: Validate repository and runtime prerequisites on startup

Why:

Patron should fail clearly when launched in the wrong place or without required dependencies.

Acceptance criteria:

- startup detects whether the current directory is a git repository
- startup checks required runtime prerequisites
- failures are shown as actionable messages
- the UI can surface setup status and missing requirements

### Issue 3: Persist repository metadata in runtime state

Why:

Patron should know which repository it is operating on and expose that clearly.

Acceptance criteria:

- runtime state stores repo root, repo name, and active branch snapshot
- the dashboard shows repository context
- task detail pages reference the active repository context
- startup updates repository metadata when it changes

### Issue 4: Add a first-run setup screen in the UI

Why:

A new user should land on an understandable setup experience instead of a generic board.

Acceptance criteria:

- Patron shows setup guidance when the runtime is not initialized correctly
- the screen explains next steps and expected dependencies
- the screen links to setup documentation
- the app transitions into the normal dashboard after setup succeeds

### Issue 5: Make runner execution explicitly repo-aware

Why:

The runner should operate against a repository intentionally, not just through ambient cwd assumptions.

Acceptance criteria:

- runner jobs include repository context
- logs show which repository path was targeted
- orchestrator and runner contracts reference the active repository explicitly
- execution fails safely if repository context is missing or inconsistent

### Issue 6: Add a tiny sample application fixture for dogfooding

Why:

We need one stable target app to test Patron end to end before broader adoption.

Acceptance criteria:

- the repository includes or can generate a tiny sample application fixture
- the fixture is small enough for quick setup
- the fixture supports at least one UI-facing QA flow
- setup for the fixture is documented

### Issue 7: Create a realistic task pack for the sample app

Why:

A testable harness needs repeatable tasks, not ad hoc manual prompts.

Acceptance criteria:

- at least three representative tasks are defined for the sample app
- tasks cover happy path, regression detection, and a fix-loop case
- each task has expected acceptance criteria and QA behavior
- tasks are suitable for manual and automated smoke testing

### Issue 8: Prove the QA workflow against the sample app instead of Patron itself

Why:

The current QA flow is strongest when validating Patron’s own UI; we need proof against a target application.

Acceptance criteria:

- Playwright QA can run against the sample app
- QA steps reference target-app behavior, not Patron internals
- screenshots, logs, and traces are captured for the target app flow
- deterministic failures route back into the fix loop correctly

### Issue 9: Rewrite the README as a real getting-started guide

Why:

The current README still reads like an internal status note more than a user-facing setup guide.

Acceptance criteria:

- README includes prerequisites
- README includes initialization steps
- README includes how to launch the app
- README includes how to test Patron on the sample app
- README distinguishes current limitations from supported behavior

### Issue 10: Publish a new-repo smoke test checklist

Why:

We need a crisp definition of what “testable” means before asking you to evaluate Patron on a fresh repository.

Acceptance criteria:

- a documented smoke test exists for a brand new repo
- the checklist includes setup, task creation, stage progression, QA evidence, and failure handling
- expected outputs are explicit
- the checklist can be used by someone who did not build Patron

## Recommended Execution Order

1. Add explicit Patron initialization flow
2. Validate repository and runtime prerequisites on startup
3. Persist repository metadata in runtime state
4. Add a first-run setup screen in the UI
5. Make runner execution explicitly repo-aware
6. Add a tiny sample application fixture for dogfooding
7. Create a realistic task pack for the sample app
8. Prove the QA workflow against the sample app instead of Patron itself
9. Rewrite the README as a real getting-started guide
10. Publish a new-repo smoke test checklist

## Recommendation

Do not jump to multi-project support yet.

If Patron cannot convincingly onboard and operate inside one brand new repository, adding project switching will only spread the ambiguity around. The right next proof is:

- one repository
- one explicit setup flow
- one tiny target app
- one clear smoke test
