PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    goal TEXT NOT NULL,
    state TEXT NOT NULL,
    priority TEXT NOT NULL DEFAULT 'normal',
    source_kind TEXT NOT NULL DEFAULT 'user',
    branch_name TEXT,
    blocked_reason_code TEXT,
    blocked_reason_text TEXT,
    current_stage TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT,
    cancelled_at TEXT,
    CHECK (
        state IN (
            'draft',
            'ready_for_planning',
            'planning',
            'ready_for_development',
            'developing',
            'ready_for_review',
            'reviewing',
            'ready_for_qa',
            'qa_running',
            'fix_required',
            'ready_for_pr',
            'pr_prepared',
            'awaiting_human',
            'done',
            'blocked',
            'failed',
            'cancelled'
        )
    )
);

CREATE TABLE IF NOT EXISTS stage_runs (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    stage TEXT NOT NULL,
    status TEXT NOT NULL,
    trigger_kind TEXT NOT NULL DEFAULT 'system',
    attempt_number INTEGER NOT NULL DEFAULT 1,
    prompt_fingerprint TEXT,
    runner_kind TEXT NOT NULL DEFAULT 'codex',
    started_at TEXT NOT NULL,
    finished_at TEXT,
    exit_code INTEGER,
    error_summary TEXT,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    CHECK (
        stage IN (
            'planning',
            'development',
            'review',
            'qa',
            'fix',
            'pr_preparation'
        )
    ),
    CHECK (
        status IN (
            'queued',
            'running',
            'completed',
            'failed',
            'interrupted',
            'cancelled'
        )
    )
);

CREATE TABLE IF NOT EXISTS state_transitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL,
    from_state TEXT,
    to_state TEXT NOT NULL,
    actor_kind TEXT NOT NULL,
    actor_id TEXT,
    reason_code TEXT,
    reason_text TEXT,
    stage_run_id TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (stage_run_id) REFERENCES stage_runs(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS human_actions (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    action_type TEXT NOT NULL,
    status TEXT NOT NULL,
    requested_by TEXT NOT NULL,
    instructions TEXT NOT NULL,
    requested_at TEXT NOT NULL,
    resolved_at TEXT,
    resolution_notes TEXT,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    CHECK (
        action_type IN (
            'approve_task',
            'approve_plan',
            'review_pr',
            'resolve_block'
        )
    ),
    CHECK (
        status IN (
            'pending',
            'completed',
            'cancelled'
        )
    )
);

CREATE TABLE IF NOT EXISTS working_artifacts (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    stage_run_id TEXT,
    artifact_kind TEXT NOT NULL,
    role TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    media_type TEXT NOT NULL DEFAULT 'text/markdown',
    required_for_stage INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (stage_run_id) REFERENCES stage_runs(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_tasks_state_updated
    ON tasks(state, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_stage_runs_task_started
    ON stage_runs(task_id, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_stage_runs_status_stage
    ON stage_runs(status, stage, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_state_transitions_task_created
    ON state_transitions(task_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_human_actions_status_requested
    ON human_actions(status, requested_at ASC);

CREATE INDEX IF NOT EXISTS idx_working_artifacts_task_role
    ON working_artifacts(task_id, role, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_working_artifacts_run_kind
    ON working_artifacts(stage_run_id, artifact_kind, created_at DESC);
