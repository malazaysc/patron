CREATE TABLE IF NOT EXISTS intake_sessions (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    initial_goal TEXT NOT NULL,
    draft_title TEXT,
    draft_markdown TEXT,
    task_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE SET NULL,
    CHECK (
        status IN (
            'awaiting_input',
            'draft_ready',
            'task_created'
        )
    )
);

CREATE TABLE IF NOT EXISTS intake_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    author_kind TEXT NOT NULL,
    message_kind TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES intake_sessions(id) ON DELETE CASCADE,
    CHECK (
        author_kind IN (
            'user',
            'orchestrator',
            'system'
        )
    ),
    CHECK (
        message_kind IN (
            'goal',
            'follow_up',
            'answer',
            'draft',
            'event'
        )
    )
);

CREATE TABLE IF NOT EXISTS activity_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    scope_kind TEXT NOT NULL,
    scope_id TEXT NOT NULL,
    task_id TEXT,
    event_kind TEXT NOT NULL,
    headline TEXT NOT NULL,
    detail TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE SET NULL,
    CHECK (
        scope_kind IN (
            'task',
            'run',
            'intake',
            'system'
        )
    )
);

CREATE INDEX IF NOT EXISTS idx_intake_sessions_updated
    ON intake_sessions(updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_intake_messages_session_created
    ON intake_messages(session_id, created_at ASC, id ASC);

CREATE INDEX IF NOT EXISTS idx_activity_events_created
    ON activity_events(created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_activity_events_task_created
    ON activity_events(task_id, created_at DESC, id DESC);
