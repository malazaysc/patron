pub mod schema;

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::app::RuntimePaths;
use crate::domain::task_lifecycle::{TaskState, TransitionMetadata};

pub struct StateStoreStatus<'a> {
    pub engine: &'a str,
    pub initial_schema_bytes: usize,
    pub location: String,
    pub schema_version: i64,
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
    pub current_stage: Option<String>,
    pub workspace_path: String,
    pub handoff_path: String,
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

pub struct WorkingArtifactUpsert<'a> {
    pub task_id: &'a str,
    pub role: &'a str,
    pub artifact_kind: &'a str,
    pub relative_path: &'a str,
    pub media_type: &'a str,
    pub required_for_stage: bool,
    pub stage_run_id: Option<&'a str>,
}

pub fn initialize(runtime: &RuntimePaths) -> Result<(), String> {
    let connection = open_connection(&runtime.state_db)?;

    connection
        .execute_batch(initial_schema_sql())
        .map_err(|error| format!("failed to apply initial schema: {error}"))?;

    connection
        .execute(
            "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
            params![schema::CURRENT_SCHEMA_VERSION, "2026-05-23T00:00:00Z"],
        )
        .map_err(|error| format!("failed to seed schema_migrations: {error}"))?;

    Ok(())
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
            GROUP BY tasks.id, tasks.title, tasks.goal, tasks.state, tasks.current_stage
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
                current_stage: row.get(4)?,
                workspace_path: row.get(5)?,
                handoff_path: row.get(6)?,
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
            GROUP BY tasks.id, tasks.title, tasks.goal, tasks.state, tasks.current_stage",
        )
        .map_err(|error| format!("failed to prepare task lookup query: {error}"))?;

    statement
        .query_row([task_id], |row| {
            Ok(TaskRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                goal: row.get(2)?,
                state: row.get(3)?,
                current_stage: row.get(4)?,
                workspace_path: row.get(5)?,
                handoff_path: row.get(6)?,
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
    connection
        .execute(
            "UPDATE tasks
             SET state = ?2, current_stage = ?3, updated_at = ?4
             WHERE id = ?1 AND state = ?5",
            params![
                task_id,
                to.as_str(),
                current_stage,
                metadata.occurred_at,
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
