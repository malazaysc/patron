# Patron Docs

This repository starts with design documents first.

The goal is a local-first, single-repository software delivery harness for Codex-driven engineering workflows on macOS.

## Document Map

- [Architecture Overview](./architecture/v1-overview.md)
- [Runtime Layout](./architecture/runtime-layout.md)
- [SQLite Schema](./architecture/sqlite-schema.md)
- [PRD Template](./prd/README.md)
- [V1 PRD](./prd/001-v1-local-autonomous-delivery-harness.md)
- [GitHub Backlog Draft](./planning/github-backlog.md)
- [New Project Readiness Backlog](./planning/new-project-readiness-backlog.md)
- [ADR 0001](./adr/0001-local-first-single-repo-scope.md)
- [ADR 0002](./adr/0002-deterministic-task-pipeline.md)
- [ADR 0003](./adr/0003-stage-artifact-workflow.md)
- [ADR 0004](./adr/0004-browser-driven-qa.md)

## Principles

- Deterministic orchestration over opaque agent behavior
- Observable workflows over hidden automation
- Structured artifacts over giant chat transcripts
- Human-readable QA over test-only validation
- Controlled autonomy with explicit approval gates
