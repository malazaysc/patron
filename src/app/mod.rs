use std::io;
use std::path::{Path, PathBuf};

use axum::{
    Form, Router,
    extract::Path as AxumPath,
    extract::State,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};

use crate::{db, orchestrator, qa, runner, ui};

#[derive(Clone)]
pub struct AppState {
    runtime: RuntimePaths,
}

impl AppState {
    pub fn new(runtime: RuntimePaths) -> Self {
        Self { runtime }
    }

    pub fn runtime(&self) -> &RuntimePaths {
        &self.runtime
    }
}

#[derive(Clone, Debug)]
pub struct RuntimePaths {
    pub root: PathBuf,
    pub state_db: PathBuf,
    pub logs_dir: PathBuf,
    pub qa_dir: PathBuf,
    pub qa_logs_dir: PathBuf,
    pub qa_screenshots_dir: PathBuf,
    pub qa_traces_dir: PathBuf,
    pub runs_dir: PathBuf,
    pub tasks_dir: PathBuf,
}

impl RuntimePaths {
    pub fn discover() -> io::Result<Self> {
        let root = std::env::current_dir()?.join(".patron");

        Ok(Self {
            logs_dir: root.join("logs"),
            qa_dir: root.join("qa"),
            qa_logs_dir: root.join("qa").join("logs"),
            qa_screenshots_dir: root.join("qa").join("screenshots"),
            qa_traces_dir: root.join("qa").join("traces"),
            state_db: root.join("state.db"),
            runs_dir: root.join("runs"),
            tasks_dir: root.join("tasks"),
            root,
        })
    }

    pub fn ensure_layout(&self) -> io::Result<()> {
        for directory in self.required_directories() {
            std::fs::create_dir_all(&directory).map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!(
                        "could not create runtime directory {}: {error}",
                        directory.display()
                    ),
                )
            })?;
        }

        if !self.state_db.exists() {
            std::fs::File::create(&self.state_db).map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!(
                        "could not create runtime database file {}: {error}",
                        self.state_db.display()
                    ),
                )
            })?;
        }

        Ok(())
    }

    pub fn required_directories(&self) -> [PathBuf; 8] {
        [
            self.root.clone(),
            self.tasks_dir.clone(),
            self.runs_dir.clone(),
            self.logs_dir.clone(),
            self.qa_dir.clone(),
            self.qa_logs_dir.clone(),
            self.qa_screenshots_dir.clone(),
            self.qa_traces_dir.clone(),
        ]
    }

    pub fn qa_evidence_directories(&self) -> [PathBuf; 3] {
        [
            self.qa_logs_dir.clone(),
            self.qa_screenshots_dir.clone(),
            self.qa_traces_dir.clone(),
        ]
    }

    pub fn relative_to_repo(&self, path: &Path) -> PathBuf {
        path.strip_prefix(self.repo_root())
            .unwrap_or(path)
            .to_path_buf()
    }

    fn repo_root(&self) -> &Path {
        self.root.parent().unwrap_or(self.root.as_path())
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/tasks", post(create_task))
        .route("/tasks/{task_id}/plan", post(run_planning))
        .route("/tasks/{task_id}/develop", post(run_development))
        .route("/tasks/{task_id}/review", post(run_review))
        .route("/tasks/{task_id}/fix", post(run_fix_loop))
        .with_state(state)
}

async fn index(State(state): State<AppState>) -> Html<String> {
    let runtime = state.runtime();
    let task_snapshot = db::list_tasks(runtime).unwrap_or_default();
    let runtime_directories = runtime
        .required_directories()
        .iter()
        .map(|path| runtime.relative_to_repo(path))
        .collect::<Vec<_>>();
    let qa_directories = runtime
        .qa_evidence_directories()
        .iter()
        .map(|path| runtime.relative_to_repo(path))
        .collect::<Vec<_>>();
    let state_store = db::state_store_status(runtime);
    let body = ui::render_home(ui::HomeView {
        runtime_root: &runtime.relative_to_repo(&runtime.root),
        runtime_directories: &runtime_directories,
        qa_directories: &qa_directories,
        state_store: &state_store,
        tasks: &task_snapshot,
        orchestrator_status: orchestrator::status_label(),
        runner_status: runner::status_label(),
        qa_status: qa::status_label(),
    });

    Html(body)
}

async fn health() -> &'static str {
    "ok"
}

#[derive(serde::Deserialize)]
struct TaskCreateForm {
    goal: String,
}

async fn create_task(State(state): State<AppState>, Form(form): Form<TaskCreateForm>) -> Response {
    match orchestrator::create_draft_task(state.runtime(), &form.goal) {
        Ok(_) => Redirect::to("/").into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>Task creation failed</h1><p>{error}</p>")),
        )
            .into_response(),
    }
}

async fn run_planning(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> Response {
    match orchestrator::run_planning(state.runtime(), &task_id) {
        Ok(_) => Redirect::to("/").into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>Planning failed</h1><p>{error}</p>")),
        )
            .into_response(),
    }
}

async fn run_development(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> Response {
    match orchestrator::run_development(state.runtime(), &task_id) {
        Ok(_) => Redirect::to("/").into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>Development failed</h1><p>{error}</p>")),
        )
            .into_response(),
    }
}

async fn run_review(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> Response {
    match orchestrator::run_review(state.runtime(), &task_id) {
        Ok(_) => Redirect::to("/").into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>Review failed</h1><p>{error}</p>")),
        )
            .into_response(),
    }
}

async fn run_fix_loop(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> Response {
    match orchestrator::run_fix_loop(state.runtime(), &task_id) {
        Ok(_) => Redirect::to("/").into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>Fix loop failed</h1><p>{error}</p>")),
        )
            .into_response(),
    }
}
