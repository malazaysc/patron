# ADR 0001: Local-First Single-Repo Scope

Status: Accepted

## Context

The project could easily expand into a generic agent platform, distributed runner, or cloud automation product.

That would increase complexity before proving the core workflow.

## Decision

V1 is constrained to:

- local execution only
- macOS only
- a single repository
- a single primary user

## Consequences

### Positive

- simpler recovery model
- simpler security and permissions model
- faster iteration
- clearer UX

### Negative

- no team workflows
- no horizontal scaling
- no cross-repo coordination

## Rationale

The main value is trustworthy local orchestration, not broad infrastructure reach.
