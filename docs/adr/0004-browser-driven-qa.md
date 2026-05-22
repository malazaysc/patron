# ADR 0004: Browser-Driven QA

Status: Accepted

## Context

Agent-generated changes often pass unit tests while still failing at real user behavior.

For this product, trust depends on behavior verification and visible evidence.

## Decision

QA in V1 will be based on human-readable scenarios executed with Playwright or browser automation.

QA must capture evidence such as:

- screenshots
- traces
- logs
- pass/fail notes

Unit and integration tests may still run during development, but they do not replace QA stage completion.

## Consequences

### Positive

- stronger confidence in user-visible behavior
- easier human review of results
- better fix-loop inputs

### Negative

- slower than command-only validation
- more infrastructure around local app launch and browser control

## Rationale

The product is optimizing for trustworthy delivery, not just fast code generation.
