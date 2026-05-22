pub mod schema;

use std::fs;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::app::RuntimePaths;

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
    pub workspace_path: String,
    pub handoff_path: String,
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

    connection
        .execute(
            "INSERT INTO tasks (
                id, title, goal, state, current_stage, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?5)",
            params![
                task.id,
                task.title,
                task.goal,
                task.state,
                "2026-05-23T00:00:00Z"
            ],
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
                "2026-05-23T00:00:00Z"
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
            GROUP BY tasks.id, tasks.title, tasks.goal, tasks.state
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
                workspace_path: row.get(4)?,
                handoff_path: row.get(5)?,
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
                "2026-05-23T00:00:00Z"
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

fn open_connection(path: &Path) -> Result<Connection, String> {
    Connection::open(path)
        .map_err(|error| format!("failed to open sqlite database {}: {error}", path.display()))
}

fn parse_task_number(task_id: &str) -> Option<u32> {
    task_id.strip_prefix("TASK-")?.parse().ok()
}
