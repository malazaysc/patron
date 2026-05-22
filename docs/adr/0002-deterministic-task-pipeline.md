# ADR 0002: Deterministic Task Pipeline

Status: Accepted

## Context

Agentic systems often hide workflow decisions inside prompts and long chat context.

That makes them difficult to reason about, recover, and trust.

## Decision

Patron will implement an explicit task state machine with code-enforced transitions for:

- planning
- development
- review
- QA
- fix loops
- PR preparation

The orchestrator may recommend actions, but the application layer owns transitions.

## Consequences

### Positive

- predictable behavior
- easier debugging
- better auditability
- safer resume and replay behavior

### Negative

- less flexibility for novel workflows
- more upfront modeling effort

## Rationale

Determinism is a core product principle and more important than maximal workflow flexibility in V1.
