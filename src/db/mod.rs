pub mod schema;

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::domain::task_lifecycle::{TaskState, TransitionMetadata};
use crate::{app::RuntimePaths, bootstrap::RepoContext};

pub struct StateStoreStatus<'a> {
    pub engine: &'a str,
    pub initial_schema_bytes: usize,
    pub location: String,
    pub schema_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepoMetadataRecord {
    pub repo_root: String,
    pub repo_name: String,
    pub git_branch: Option<String>,
    pub is_git_repo: bool,
    pub captured_at: String,
}

pub fn state_store_status(runtime: &RuntimePaths) -> StateStoreStatus<'_> {
    StateStoreStatus {
        engine: "sqlite",
        initial_schema_bytes: initial_schema_sql().len(),
        location: runtime
            .relative_to_repo(&runtime.state_db)
            .display()
            .to_string(),
        schema_version: schema::CURRENT_SCHEMA_VERSION,
    }
}

pub fn initial_schema_sql() -> &'static str {
    schema::INITIAL_SCHEMA
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskRecord {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub state: String,
    pub blocked_reason_code: Option<String>,
    pub blocked_reason_text: Option<String>,
    pub current_stage: Option<String>,
    pub workspace_path: String,
    pub handoff_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntakeSessionRecord {
    pub id: String,
    pub status: String,
    pub initial_goal: String,
    pub draft_title: Option<String>,
    pub draft_markdown: Option<String>,
    pub task_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntakeMessageRecord {
    pub id: i64,
    pub session_id: String,
    pub author_kind: String,
    pub message_kind: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityEventRecord {
    pub id: i64,
    pub scope_kind: String,
    pub scope_id: String,
    pub task_id: Option<String>,
    pub event_kind: String,
    pub headline: String,
    pub detail: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StageRunRecord {
    pub id: String,
    pub task_id: String,
    pub stage: String,
    pub status: String,
    pub attempt_number: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub exit_code: Option<i64>,
    pub error_summary: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateTransitionRecord {
    pub id: i64,
    pub task_id: String,
    pub from_state: Option<String>,
    pub to_state: String,
    pub actor_kind: String,
    pub actor_id: Option<String>,
    pub reason_code: Option<String>,
    pub reason_text: String,
    pub stage_run_id: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingArtifactRecord {
    pub id: String,
    pub task_id: String,
    pub stage_run_id: Option<String>,
    pub artifact_kind: String,
    pub role: String,
    pub relative_path: String,
    pub media_type: String,
    pub required_for_stage: bool,
    pub created_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HumanActionRecord {
    pub id: String,
    pub task_id: String,
    pub action_type: String,
    pub status: String,
    pub requested_by: String,
    pub instructions: String,
    pub requested_at: String,
    pub resolved_at: Option<String>,
    pub resolution_notes: Option<String>,
}

pub struct HumanActionCreate<'a> {
    pub id: &'a str,
    pub task_id: &'a str,
    pub action_type: &'a str,
    pub requested_by: &'a str,
    pub instructions: &'a str,
}

pub struct WorkingArtifactUpsert<'a> {
    pub task_id: &'a str,
    pub role: &'a str,
    pub artifact_kind: &'a str,
    pub relative_path: &'a str,
    pub media_type: &'a str,
    pub required_for_stage: bool,
    pub stage_run_id: Option<&'a str>,
}

pub struct IntakeSessionUpdate<'a> {
    pub status: &'a str,
    pub draft_title: Option<&'a str>,
    pub draft_markdown: Option<&'a str>,
    pub task_id: Option<&'a str>,
}

pub struct ActivityEventCreate<'a> {
    pub scope_kind: &'a str,
    pub scope_id: &'a str,
    pub task_id: Option<&'a str>,
    pub event_kind: &'a str,
    pub headline: &'a str,
    pub detail: Option<&'a str>,
}

pub fn initialize(runtime: &RuntimePaths) -> Result<(), String> {
    let connection = open_connection(&runtime.state_db)?;
    for (version, sql) in schema::MIGRATIONS {
        connection
            .execute_batch(sql)
            .map_err(|error| format!("failed to apply schema migration {version}: {error}"))?;
        connection
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
                params![version, timestamp_now()],
            )
            .map_err(|error| format!("failed to record schema migration {version}: {error}"))?;
    }

    Ok(())
}

pub fn persist_repo_metadata(runtime: &RuntimePaths, repo: &RepoContext) -> Result<(), String> {
    let connection = open_connection(&runtime.state_db)?;
    let values = [
        ("repo_root", repo.repo_root.display().to_string()),
        ("repo_name", repo.repo_name.clone()),
        ("git_branch", repo.git_branch.clone().unwrap_or_default()),
        (
            "is_git_repo",
            if repo.is_git_repo {
                "1".into()
            } else {
                "0".into()
            },
        ),
    ];

    for (key, value) in values {
        connection
            .execute(
                "INSERT INTO runtime_metadata(key, value, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET
                   value = excluded.value,
                   updated_at = excluded.updated_at",
                params![key, value, timestamp_now()],
            )
            .map_err(|error| format!("failed to persist runtime metadata `{key}`: {error}"))?;
    }

    Ok(())
}

pub fn load_repo_metadata(runtime: &RuntimePaths) -> Result<Option<RepoMetadataRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT key, value, updated_at
             FROM runtime_metadata
             WHERE key IN ('repo_root', 'repo_name', 'git_branch', 'is_git_repo')
             ORDER BY key ASC",
        )
        .map_err(|error| format!("failed to prepare runtime metadata query: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| format!("failed to query runtime metadata: {error}"))?;

    let entries = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to decode runtime metadata: {error}"))?;
    if entries.is_empty() {
        return Ok(None);
    }

    let mut repo_root = None;
    let mut repo_name = None;
    let mut git_branch = None;
    let mut is_git_repo = false;
    let mut captured_at = String::new();

    for (key, value, updated_at) in entries {
        captured_at = updated_at;
        match key.as_str() {
            "repo_root" => repo_root = Some(value),
            "repo_name" => repo_name = Some(value),
            "git_branch" if !value.trim().is_empty() => {
                git_branch = Some(value);
            }
            "git_branch" => {}
            "is_git_repo" => is_git_repo = value == "1",
            _ => {}
        }
    }

    match (repo_root, repo_name) {
        (Some(repo_root), Some(repo_name)) => Ok(Some(RepoMetadataRecord {
            repo_root,
            repo_name,
            git_branch,
            is_git_repo,
            captured_at,
        })),
        _ => Ok(None),
    }
}

pub fn next_task_id(runtime: &RuntimePaths) -> Result<String, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare("SELECT id FROM tasks ORDER BY id DESC LIMIT 1")
        .map_err(|error| format!("failed to prepare task id query: {error}"))?;

    let latest_id = statement
        .query_row([], |row| row.get::<_, String>(0))
        .optional()
        .map_err(|error| format!("failed to fetch last task id: {error}"))?;

    let next_number = latest_id
        .as_deref()
        .and_then(parse_task_number)
        .map_or(1, |number| number + 1);

    Ok(format!("TASK-{next_number:04}"))
}

pub fn next_intake_session_id(runtime: &RuntimePaths) -> Result<String, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare("SELECT id FROM intake_sessions ORDER BY id DESC LIMIT 1")
        .map_err(|error| format!("failed to prepare intake id query: {error}"))?;

    let latest_id = statement
        .query_row([], |row| row.get::<_, String>(0))
        .optional()
        .map_err(|error| format!("failed to fetch last intake id: {error}"))?;

    let next_number = latest_id
        .as_deref()
        .and_then(|value| value.strip_prefix("INTAKE-"))
        .and_then(|value| value.parse::<u32>().ok())
        .map_or(1, |number| number + 1);

    Ok(format!("INTAKE-{next_number:04}"))
}

pub fn create_intake_session(
    runtime: &RuntimePaths,
    initial_goal: &str,
) -> Result<IntakeSessionRecord, String> {
    let connection = open_connection(&runtime.state_db)?;
    let session = IntakeSessionRecord {
        id: next_intake_session_id(runtime)?,
        status: "awaiting_input".into(),
        initial_goal: initial_goal.to_string(),
        draft_title: None,
        draft_markdown: None,
        task_id: None,
        created_at: timestamp_now(),
        updated_at: timestamp_now(),
    };

    connection
        .execute(
            "INSERT INTO intake_sessions (
                id, status, initial_goal, draft_title, draft_markdown, task_id, created_at, updated_at
            ) VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4, ?5)",
            params![
                session.id,
                session.status,
                session.initial_goal,
                session.created_at,
                session.updated_at
            ],
        )
        .map_err(|error| format!("failed to create intake session {}: {error}", session.id))?;

    record_activity_event(
        runtime,
        ActivityEventCreate {
            scope_kind: "intake",
            scope_id: &session.id,
            task_id: None,
            event_kind: "intake_started",
            headline: "Orchestrator intake started",
            detail: Some(&session.initial_goal),
        },
    )?;

    Ok(session)
}

pub fn get_intake_session(
    runtime: &RuntimePaths,
    session_id: &str,
) -> Result<Option<IntakeSessionRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT id, status, initial_goal, draft_title, draft_markdown, task_id, created_at, updated_at
             FROM intake_sessions
             WHERE id = ?1",
        )
        .map_err(|error| format!("failed to prepare intake session lookup: {error}"))?;

    statement
        .query_row([session_id], |row| {
            Ok(IntakeSessionRecord {
                id: row.get(0)?,
                status: row.get(1)?,
                initial_goal: row.get(2)?,
                draft_title: row.get(3)?,
                draft_markdown: row.get(4)?,
                task_id: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .optional()
        .map_err(|error| format!("failed to load intake session {session_id}: {error}"))
}

pub fn list_intake_sessions(
    runtime: &RuntimePaths,
    limit: usize,
) -> Result<Vec<IntakeSessionRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT id, status, initial_goal, draft_title, draft_markdown, task_id, created_at, updated_at
             FROM intake_sessions
             ORDER BY updated_at DESC, id DESC
             LIMIT ?1",
        )
        .map_err(|error| format!("failed to prepare intake session listing query: {error}"))?;

    let rows = statement
        .query_map([limit as i64], |row| {
            Ok(IntakeSessionRecord {
                id: row.get(0)?,
                status: row.get(1)?,
                initial_goal: row.get(2)?,
                draft_title: row.get(3)?,
                draft_markdown: row.get(4)?,
                task_id: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|error| format!("failed to query intake sessions: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to decode intake sessions: {error}"))
}

pub fn update_intake_session(
    runtime: &RuntimePaths,
    session_id: &str,
    update: IntakeSessionUpdate<'_>,
) -> Result<(), String> {
    let connection = open_connection(&runtime.state_db)?;
    connection
        .execute(
            "UPDATE intake_sessions
             SET status = ?2,
                 draft_title = ?3,
                 draft_markdown = ?4,
                 task_id = ?5,
                 updated_at = ?6
             WHERE id = ?1",
            params![
                session_id,
                update.status,
                update.draft_title,
                update.draft_markdown,
                update.task_id,
                timestamp_now()
            ],
        )
        .map_err(|error| format!("failed to update intake session {session_id}: {error}"))?;
    Ok(())
}

pub fn insert_intake_message(
    runtime: &RuntimePaths,
    session_id: &str,
    author_kind: &str,
    message_kind: &str,
    body: &str,
) -> Result<(), String> {
    let connection = open_connection(&runtime.state_db)?;
    let created_at = timestamp_now();
    connection
        .execute(
            "INSERT INTO intake_messages (
                session_id, author_kind, message_kind, body, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![session_id, author_kind, message_kind, body, created_at],
        )
        .map_err(|error| format!("failed to insert intake message for {session_id}: {error}"))?;
    connection
        .execute(
            "UPDATE intake_sessions SET updated_at = ?2 WHERE id = ?1",
            params![session_id, timestamp_now()],
        )
        .map_err(|error| {
            format!("failed to update intake session timestamp {session_id}: {error}")
        })?;
    Ok(())
}

pub fn list_intake_messages(
    runtime: &RuntimePaths,
    session_id: &str,
) -> Result<Vec<IntakeMessageRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT id, session_id, author_kind, message_kind, body, created_at
             FROM intake_messages
             WHERE session_id = ?1
             ORDER BY created_at ASC, id ASC",
        )
        .map_err(|error| format!("failed to prepare intake message query: {error}"))?;

    let rows = statement
        .query_map([session_id], |row| {
            Ok(IntakeMessageRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                author_kind: row.get(2)?,
                message_kind: row.get(3)?,
                body: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|error| format!("failed to query intake messages for {session_id}: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to decode intake messages for {session_id}: {error}"))
}

pub fn insert_task(runtime: &RuntimePaths, task: &TaskRecord) -> Result<(), String> {
    let connection = open_connection(&runtime.state_db)?;
    let timestamp = timestamp_now();

    connection
        .execute(
            "INSERT INTO tasks (
                id, title, goal, state, current_stage, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?5)",
            params![task.id, task.title, task.goal, task.state, timestamp],
        )
        .map_err(|error| format!("failed to insert task {}: {error}", task.id))?;

    connection
        .execute(
            "INSERT INTO working_artifacts (
                id, task_id, artifact_kind, role, relative_path, media_type, required_for_stage, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                format!("{}-handoff", task.id),
                task.id,
                "orchestrator_handoff",
                "initial_handoff",
                task.handoff_path,
                "text/markdown",
                0,
                timestamp_now(),
            ],
        )
        .map_err(|error| format!("failed to insert handoff artifact for {}: {error}", task.id))?;

    record_activity_event(
        runtime,
        ActivityEventCreate {
            scope_kind: "task",
            scope_id: &task.id,
            task_id: Some(&task.id),
            event_kind: "task_created",
            headline: "Task created",
            detail: Some(&task.title),
        },
    )?;

    Ok(())
}

pub fn list_tasks(runtime: &RuntimePaths) -> Result<Vec<TaskRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT
                tasks.id,
                tasks.title,
                tasks.goal,
                tasks.state,
                tasks.blocked_reason_code,
                tasks.blocked_reason_text,
                tasks.current_stage,
                COALESCE(
                    MAX(CASE WHEN working_artifacts.role = 'task_workspace' THEN working_artifacts.relative_path END),
                    ''
                ) AS workspace_path,
                COALESCE(
                    MAX(CASE WHEN working_artifacts.role = 'initial_handoff' THEN working_artifacts.relative_path END),
                    ''
                ) AS handoff_path
            FROM tasks
            LEFT JOIN working_artifacts ON working_artifacts.task_id = tasks.id
            GROUP BY tasks.id, tasks.title, tasks.goal, tasks.state, tasks.blocked_reason_code, tasks.blocked_reason_text, tasks.current_stage
            ORDER BY tasks.id DESC",
        )
        .map_err(|error| format!("failed to prepare task listing query: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(TaskRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                goal: row.get(2)?,
                state: row.get(3)?,
                blocked_reason_code: row.get(4)?,
                blocked_reason_text: row.get(5)?,
                current_stage: row.get(6)?,
                workspace_path: row.get(7)?,
                handoff_path: row.get(8)?,
            })
        })
        .map_err(|error| format!("failed to query tasks: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to decode task rows: {error}"))
}

pub fn register_workspace_artifact(
    runtime: &RuntimePaths,
    task_id: &str,
    relative_path: &str,
) -> Result<(), String> {
    let connection = open_connection(&runtime.state_db)?;

    connection
        .execute(
            "INSERT INTO working_artifacts (
                id, task_id, artifact_kind, role, relative_path, media_type, required_for_stage, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                format!("{task_id}-workspace"),
                task_id,
                "task_workspace",
                "task_workspace",
                relative_path,
                "inode/directory",
                0,
                timestamp_now(),
            ],
        )
        .map_err(|error| format!("failed to register workspace artifact for {task_id}: {error}"))?;

    let workspace_dir = runtime
        .root
        .join(relative_path.trim_start_matches(".patron/"));
    if !workspace_dir.exists() {
        fs::create_dir_all(&workspace_dir).map_err(|error| {
            format!(
                "failed to create task workspace directory {}: {error}",
                workspace_dir.display()
            )
        })?;
    }

    Ok(())
}

pub fn get_task(runtime: &RuntimePaths, task_id: &str) -> Result<Option<TaskRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT
                tasks.id,
                tasks.title,
                tasks.goal,
                tasks.state,
                tasks.blocked_reason_code,
                tasks.blocked_reason_text,
                tasks.current_stage,
                COALESCE(
                    MAX(CASE WHEN working_artifacts.role = 'task_workspace' THEN working_artifacts.relative_path END),
                    ''
                ) AS workspace_path,
                COALESCE(
                    MAX(CASE WHEN working_artifacts.role = 'initial_handoff' THEN working_artifacts.relative_path END),
                    ''
                ) AS handoff_path
            FROM tasks
            LEFT JOIN working_artifacts ON working_artifacts.task_id = tasks.id
            WHERE tasks.id = ?1
            GROUP BY tasks.id, tasks.title, tasks.goal, tasks.state, tasks.blocked_reason_code, tasks.blocked_reason_text, tasks.current_stage",
        )
        .map_err(|error| format!("failed to prepare task lookup query: {error}"))?;

    statement
        .query_row([task_id], |row| {
            Ok(TaskRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                goal: row.get(2)?,
                state: row.get(3)?,
                blocked_reason_code: row.get(4)?,
                blocked_reason_text: row.get(5)?,
                current_stage: row.get(6)?,
                workspace_path: row.get(7)?,
                handoff_path: row.get(8)?,
            })
        })
        .optional()
        .map_err(|error| format!("failed to look up task {task_id}: {error}"))
}

pub fn create_stage_run(
    runtime: &RuntimePaths,
    task_id: &str,
    stage: &str,
) -> Result<StageRunRecord, String> {
    let connection = open_connection(&runtime.state_db)?;
    let attempt_number: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(attempt_number), 0) + 1 FROM stage_runs WHERE task_id = ?1 AND stage = ?2",
            params![task_id, stage],
            |row| row.get(0),
        )
        .map_err(|error| {
            format!("failed to compute stage attempt number for {task_id}/{stage}: {error}")
        })?;

    let run_id = format!("{task_id}-{stage}-{attempt_number:03}");
    let started_at = timestamp_now();

    connection
        .execute(
            "INSERT INTO stage_runs (
                id, task_id, stage, status, trigger_kind, attempt_number, runner_kind, started_at
            ) VALUES (?1, ?2, ?3, 'running', 'system', ?4, 'codex', ?5)",
            params![run_id, task_id, stage, attempt_number, started_at],
        )
        .map_err(|error| format!("failed to create stage run {run_id}: {error}"))?;

    let detail = format!("{} started for {}", stage, task_id);
    record_activity_event(
        runtime,
        ActivityEventCreate {
            scope_kind: "run",
            scope_id: &run_id,
            task_id: Some(task_id),
            event_kind: "stage_started",
            headline: "Stage started",
            detail: Some(&detail),
        },
    )?;

    Ok(StageRunRecord {
        id: run_id,
        task_id: task_id.to_string(),
        stage: stage.to_string(),
        status: "running".into(),
        attempt_number,
        started_at,
        finished_at: None,
        exit_code: None,
        error_summary: None,
    })
}

pub fn complete_stage_run(
    runtime: &RuntimePaths,
    run_id: &str,
    status: &str,
    exit_code: Option<i64>,
    error_summary: Option<&str>,
) -> Result<(), String> {
    let connection = open_connection(&runtime.state_db)?;
    connection
        .execute(
            "UPDATE stage_runs
             SET status = ?2, finished_at = ?3, exit_code = ?4, error_summary = ?5
             WHERE id = ?1",
            params![run_id, status, timestamp_now(), exit_code, error_summary],
        )
        .map_err(|error| format!("failed to complete stage run {run_id}: {error}"))?;

    let mut stage = String::new();
    let mut task_id = String::new();
    let _ = connection.query_row(
        "SELECT task_id, stage FROM stage_runs WHERE id = ?1",
        [run_id],
        |row| {
            task_id = row.get(0)?;
            stage = row.get(1)?;
            Ok(())
        },
    );
    let detail = format!(
        "{} {} with exit_code={}",
        stage,
        status,
        exit_code.map_or_else(|| "none".to_string(), |value| value.to_string())
    );
    record_activity_event(
        runtime,
        ActivityEventCreate {
            scope_kind: "run",
            scope_id: run_id,
            task_id: (!task_id.is_empty()).then_some(task_id.as_str()),
            event_kind: "stage_completed",
            headline: "Stage updated",
            detail: Some(&detail),
        },
    )?;
    Ok(())
}

pub fn transition_task_state(
    runtime: &RuntimePaths,
    task_id: &str,
    from: TaskState,
    to: TaskState,
    metadata: &TransitionMetadata,
) -> Result<(), String> {
    let connection = open_connection(&runtime.state_db)?;
    let current_stage = current_stage_for_state(to);
    let blocked_reason_code = if to == TaskState::Blocked {
        metadata.reason_code.as_deref()
    } else {
        None
    };
    let blocked_reason_text = if to == TaskState::Blocked {
        Some(metadata.reason_text.as_str())
    } else {
        None
    };
    connection
        .execute(
            "UPDATE tasks
             SET state = ?2, current_stage = ?3, updated_at = ?4, blocked_reason_code = ?5, blocked_reason_text = ?6
             WHERE id = ?1 AND state = ?7",
            params![
                task_id,
                to.as_str(),
                current_stage,
                metadata.occurred_at,
                blocked_reason_code,
                blocked_reason_text,
                from.as_str()
            ],
        )
        .map_err(|error| format!("failed to update task state for {task_id}: {error}"))?;

    connection
        .execute(
            "INSERT INTO state_transitions (
                task_id, from_state, to_state, actor_kind, actor_id, reason_code, reason_text, stage_run_id, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                task_id,
                from.as_str(),
                to.as_str(),
                metadata.actor.as_str(),
                metadata.actor_id.as_deref(),
                metadata.reason_code.as_deref(),
                metadata.reason_text,
                metadata.run_id.as_deref(),
                metadata.occurred_at
            ],
        )
        .map_err(|error| format!("failed to record state transition for {task_id}: {error}"))?;

    let detail = format!("{} -> {}", from.as_str(), to.as_str());
    record_activity_event(
        runtime,
        ActivityEventCreate {
            scope_kind: "task",
            scope_id: task_id,
            task_id: Some(task_id),
            event_kind: "state_transition",
            headline: "Task state changed",
            detail: Some(&detail),
        },
    )?;

    Ok(())
}

pub fn upsert_working_artifact(
    runtime: &RuntimePaths,
    artifact: WorkingArtifactUpsert<'_>,
) -> Result<(), String> {
    let connection = open_connection(&runtime.state_db)?;
    let artifact_id = format!("{}-{}", artifact.task_id, artifact.role);
    connection
        .execute(
            "INSERT INTO working_artifacts (
                id, task_id, stage_run_id, artifact_kind, role, relative_path, media_type, required_for_stage, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
                stage_run_id = excluded.stage_run_id,
                artifact_kind = excluded.artifact_kind,
                role = excluded.role,
                relative_path = excluded.relative_path,
                media_type = excluded.media_type,
                required_for_stage = excluded.required_for_stage",
            params![
                artifact_id,
                artifact.task_id,
                artifact.stage_run_id,
                artifact.artifact_kind,
                artifact.role,
                artifact.relative_path,
                artifact.media_type,
                if artifact.required_for_stage { 1 } else { 0 },
                timestamp_now()
            ],
        )
        .map_err(|error| format!("failed to upsert artifact {artifact_id}: {error}"))?;

    Ok(())
}

pub fn list_stage_runs(
    runtime: &RuntimePaths,
    task_id: &str,
) -> Result<Vec<StageRunRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT id, task_id, stage, status, attempt_number, started_at, finished_at, exit_code, error_summary
             FROM stage_runs
             WHERE task_id = ?1
             ORDER BY started_at DESC, id DESC",
        )
        .map_err(|error| format!("failed to prepare stage run query for {task_id}: {error}"))?;

    let rows = statement
        .query_map([task_id], |row| {
            Ok(StageRunRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                stage: row.get(2)?,
                status: row.get(3)?,
                attempt_number: row.get(4)?,
                started_at: row.get(5)?,
                finished_at: row.get(6)?,
                exit_code: row.get(7)?,
                error_summary: row.get(8)?,
            })
        })
        .map_err(|error| format!("failed to query stage runs for {task_id}: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to decode stage runs for {task_id}: {error}"))
}

pub fn list_open_stage_runs(runtime: &RuntimePaths) -> Result<Vec<StageRunRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT id, task_id, stage, status, attempt_number, started_at, finished_at, exit_code, error_summary
             FROM stage_runs
             WHERE status = 'running'
             ORDER BY started_at ASC, id ASC",
        )
        .map_err(|error| format!("failed to prepare open stage run query: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(StageRunRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                stage: row.get(2)?,
                status: row.get(3)?,
                attempt_number: row.get(4)?,
                started_at: row.get(5)?,
                finished_at: row.get(6)?,
                exit_code: row.get(7)?,
                error_summary: row.get(8)?,
            })
        })
        .map_err(|error| format!("failed to query open stage runs: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to decode open stage runs: {error}"))
}

pub fn list_recent_stage_runs(
    runtime: &RuntimePaths,
    limit: usize,
) -> Result<Vec<StageRunRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT id, task_id, stage, status, attempt_number, started_at, finished_at, exit_code, error_summary
             FROM stage_runs
             ORDER BY started_at DESC, id DESC
             LIMIT ?1",
        )
        .map_err(|error| format!("failed to prepare recent stage run query: {error}"))?;

    let rows = statement
        .query_map([limit as i64], |row| {
            Ok(StageRunRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                stage: row.get(2)?,
                status: row.get(3)?,
                attempt_number: row.get(4)?,
                started_at: row.get(5)?,
                finished_at: row.get(6)?,
                exit_code: row.get(7)?,
                error_summary: row.get(8)?,
            })
        })
        .map_err(|error| format!("failed to query recent stage runs: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to decode recent stage runs: {error}"))
}

pub fn list_state_transitions(
    runtime: &RuntimePaths,
    task_id: &str,
) -> Result<Vec<StateTransitionRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT id, task_id, from_state, to_state, actor_kind, actor_id, reason_code, reason_text, stage_run_id, created_at
             FROM state_transitions
             WHERE task_id = ?1
             ORDER BY created_at DESC, id DESC",
        )
        .map_err(|error| {
            format!("failed to prepare transition query for {task_id}: {error}")
        })?;

    let rows = statement
        .query_map([task_id], |row| {
            Ok(StateTransitionRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                from_state: row.get(2)?,
                to_state: row.get(3)?,
                actor_kind: row.get(4)?,
                actor_id: row.get(5)?,
                reason_code: row.get(6)?,
                reason_text: row.get(7)?,
                stage_run_id: row.get(8)?,
                created_at: row.get(9)?,
            })
        })
        .map_err(|error| format!("failed to query transitions for {task_id}: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to decode transitions for {task_id}: {error}"))
}

pub fn list_working_artifacts(
    runtime: &RuntimePaths,
    task_id: &str,
) -> Result<Vec<WorkingArtifactRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT id, task_id, stage_run_id, artifact_kind, role, relative_path, media_type, required_for_stage, created_at
             FROM working_artifacts
             WHERE task_id = ?1
             ORDER BY created_at DESC, id DESC",
        )
        .map_err(|error| format!("failed to prepare artifact query for {task_id}: {error}"))?;

    let rows = statement
        .query_map([task_id], |row| {
            Ok(WorkingArtifactRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                stage_run_id: row.get(2)?,
                artifact_kind: row.get(3)?,
                role: row.get(4)?,
                relative_path: row.get(5)?,
                media_type: row.get(6)?,
                required_for_stage: row.get::<_, i64>(7)? != 0,
                created_at: row.get(8)?,
            })
        })
        .map_err(|error| format!("failed to query artifacts for {task_id}: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to decode artifacts for {task_id}: {error}"))
}

pub fn get_working_artifact_by_role(
    runtime: &RuntimePaths,
    task_id: &str,
    role: &str,
) -> Result<Option<WorkingArtifactRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT id, task_id, stage_run_id, artifact_kind, role, relative_path, media_type, required_for_stage, created_at
             FROM working_artifacts
             WHERE task_id = ?1 AND role = ?2
             LIMIT 1",
        )
        .map_err(|error| {
            format!("failed to prepare artifact lookup for {task_id}/{role}: {error}")
        })?;

    statement
        .query_row(params![task_id, role], |row| {
            Ok(WorkingArtifactRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                stage_run_id: row.get(2)?,
                artifact_kind: row.get(3)?,
                role: row.get(4)?,
                relative_path: row.get(5)?,
                media_type: row.get(6)?,
                required_for_stage: row.get::<_, i64>(7)? != 0,
                created_at: row.get(8)?,
            })
        })
        .optional()
        .map_err(|error| format!("failed to look up artifact {task_id}/{role}: {error}"))
}

pub fn insert_human_action(
    runtime: &RuntimePaths,
    action: HumanActionCreate<'_>,
) -> Result<(), String> {
    let connection = open_connection(&runtime.state_db)?;
    connection
        .execute(
            "INSERT OR REPLACE INTO human_actions (
                id, task_id, action_type, status, requested_by, instructions, requested_at
            ) VALUES (?1, ?2, ?3, 'pending', ?4, ?5, ?6)",
            params![
                action.id,
                action.task_id,
                action.action_type,
                action.requested_by,
                action.instructions,
                timestamp_now()
            ],
        )
        .map_err(|error| format!("failed to insert human action {}: {error}", action.id))?;
    Ok(())
}

pub fn list_human_actions(
    runtime: &RuntimePaths,
    task_id: &str,
) -> Result<Vec<HumanActionRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT id, task_id, action_type, status, requested_by, instructions, requested_at, resolved_at, resolution_notes
             FROM human_actions
             WHERE task_id = ?1
             ORDER BY requested_at DESC, id DESC",
        )
        .map_err(|error| format!("failed to prepare human action query for {task_id}: {error}"))?;

    let rows = statement
        .query_map([task_id], |row| {
            Ok(HumanActionRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                action_type: row.get(2)?,
                status: row.get(3)?,
                requested_by: row.get(4)?,
                instructions: row.get(5)?,
                requested_at: row.get(6)?,
                resolved_at: row.get(7)?,
                resolution_notes: row.get(8)?,
            })
        })
        .map_err(|error| format!("failed to query human actions for {task_id}: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to decode human actions for {task_id}: {error}"))
}

pub fn record_activity_event(
    runtime: &RuntimePaths,
    event: ActivityEventCreate<'_>,
) -> Result<(), String> {
    let connection = open_connection(&runtime.state_db)?;
    connection
        .execute(
            "INSERT INTO activity_events (
                scope_kind, scope_id, task_id, event_kind, headline, detail, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.scope_kind,
                event.scope_id,
                event.task_id,
                event.event_kind,
                event.headline,
                event.detail,
                timestamp_now()
            ],
        )
        .map_err(|error| {
            format!(
                "failed to record activity event {}: {error}",
                event.event_kind
            )
        })?;
    Ok(())
}

pub fn list_recent_activity_events(
    runtime: &RuntimePaths,
    limit: usize,
) -> Result<Vec<ActivityEventRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT id, scope_kind, scope_id, task_id, event_kind, headline, detail, created_at
             FROM activity_events
             ORDER BY created_at DESC, id DESC
             LIMIT ?1",
        )
        .map_err(|error| format!("failed to prepare activity query: {error}"))?;

    let rows = statement
        .query_map([limit as i64], |row| {
            Ok(ActivityEventRecord {
                id: row.get(0)?,
                scope_kind: row.get(1)?,
                scope_id: row.get(2)?,
                task_id: row.get(3)?,
                event_kind: row.get(4)?,
                headline: row.get(5)?,
                detail: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|error| format!("failed to query activity events: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to decode activity events: {error}"))
}

pub fn list_task_activity_events(
    runtime: &RuntimePaths,
    task_id: &str,
    limit: usize,
) -> Result<Vec<ActivityEventRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT id, scope_kind, scope_id, task_id, event_kind, headline, detail, created_at
             FROM activity_events
             WHERE task_id = ?1
             ORDER BY created_at DESC, id DESC
             LIMIT ?2",
        )
        .map_err(|error| format!("failed to prepare task activity query for {task_id}: {error}"))?;

    let rows = statement
        .query_map(params![task_id, limit as i64], |row| {
            Ok(ActivityEventRecord {
                id: row.get(0)?,
                scope_kind: row.get(1)?,
                scope_id: row.get(2)?,
                task_id: row.get(3)?,
                event_kind: row.get(4)?,
                headline: row.get(5)?,
                detail: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|error| format!("failed to query task activity for {task_id}: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to decode task activity for {task_id}: {error}"))
}

pub fn get_latest_stage_run(
    runtime: &RuntimePaths,
    task_id: &str,
) -> Result<Option<StageRunRecord>, String> {
    let connection = open_connection(&runtime.state_db)?;
    let mut statement = connection
        .prepare(
            "SELECT id, task_id, stage, status, attempt_number, started_at, finished_at, exit_code, error_summary
             FROM stage_runs
             WHERE task_id = ?1
             ORDER BY started_at DESC, id DESC
             LIMIT 1",
        )
        .map_err(|error| format!("failed to prepare latest stage run query for {task_id}: {error}"))?;

    statement
        .query_row([task_id], |row| {
            Ok(StageRunRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                stage: row.get(2)?,
                status: row.get(3)?,
                attempt_number: row.get(4)?,
                started_at: row.get(5)?,
                finished_at: row.get(6)?,
                exit_code: row.get(7)?,
                error_summary: row.get(8)?,
            })
        })
        .optional()
        .map_err(|error| format!("failed to load latest run for {task_id}: {error}"))
}

fn open_connection(path: &Path) -> Result<Connection, String> {
    Connection::open(path)
        .map_err(|error| format!("failed to open sqlite database {}: {error}", path.display()))
}

fn parse_task_number(task_id: &str) -> Option<u32> {
    task_id.strip_prefix("TASK-")?.parse().ok()
}

fn current_stage_for_state(state: TaskState) -> Option<&'static str> {
    match state {
        TaskState::Planning => Some("planning"),
        TaskState::Developing => Some("development"),
        TaskState::Reviewing => Some("review"),
        TaskState::QaRunning => Some("qa"),
        TaskState::ReadyForDevelopment => Some("development"),
        TaskState::ReadyForReview => Some("review"),
        TaskState::ReadyForQa => Some("qa"),
        TaskState::ReadyForPr | TaskState::PrPrepared => Some("pr_preparation"),
        _ => None,
    }
}

fn timestamp_now() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("unix:{}", duration.as_secs()),
        Err(_) => "unix:0".to_string(),
    }
}
