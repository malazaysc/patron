use std::io;
use std::path::{Path, PathBuf};

use axum::{
    Form, Router,
    extract::Path as AxumPath,
    extract::State,
    http::{HeaderMap, HeaderValue},
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
        .route("/board", get(board))
        .route("/tasks", get(tasks_index).post(create_task))
        .route("/runs", get(runs_index))
        .route("/health", get(health))
        .route("/tasks/{task_id}", get(task_detail))
        .route("/tasks/{task_id}/artifacts/{role}", get(task_artifact))
        .route("/tasks/{task_id}/plan", post(run_planning))
        .route("/tasks/{task_id}/develop", post(run_development))
        .route("/tasks/{task_id}/review", post(run_review))
        .route("/tasks/{task_id}/qa", post(run_qa))
        .route("/tasks/{task_id}/prepare-pr", post(run_pr_preparation))
        .route("/tasks/{task_id}/fix", post(run_fix_loop))
        .with_state(state)
}

async fn index(State(state): State<AppState>) -> Html<String> {
    let runtime = state.runtime();
    let task_snapshot = db::list_tasks(runtime).unwrap_or_default();
    let state_store = db::state_store_status(runtime);
    let recent_runs = db::list_recent_stage_runs(runtime, 12).unwrap_or_default();
    let body = ui::render_dashboard(ui::DashboardView {
        runtime_root: &runtime.relative_to_repo(&runtime.root),
        state_store: &state_store,
        tasks: &task_snapshot,
        recent_runs: &recent_runs,
        orchestrator_status: orchestrator::status_label(),
        runner_status: runner::status_label(),
        qa_status: qa::status_label(),
    });

    Html(body)
}

async fn board(State(state): State<AppState>) -> Html<String> {
    let runtime = state.runtime();
    let task_snapshot = db::list_tasks(runtime).unwrap_or_default();
    Html(ui::render_board(ui::BoardView {
        tasks: &task_snapshot,
    }))
}

async fn tasks_index(State(state): State<AppState>) -> Html<String> {
    let runtime = state.runtime();
    let task_snapshot = db::list_tasks(runtime).unwrap_or_default();
    Html(ui::render_tasks_index(ui::TaskListView {
        tasks: &task_snapshot,
    }))
}

async fn runs_index(State(state): State<AppState>) -> Html<String> {
    let runtime = state.runtime();
    let task_snapshot = db::list_tasks(runtime).unwrap_or_default();
    let recent_runs = db::list_recent_stage_runs(runtime, 64).unwrap_or_default();
    Html(ui::render_runs(ui::RunsView {
        tasks: &task_snapshot,
        runs: &recent_runs,
    }))
}

async fn health() -> &'static str {
    "ok"
}

async fn task_detail(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> Response {
    let runtime = state.runtime();
    let Some(task) = db::get_task(runtime, &task_id).unwrap_or_default() else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Html(format!("<h1>Task not found</h1><p>{task_id}</p>")),
        )
            .into_response();
    };
    let transitions = db::list_state_transitions(runtime, &task_id).unwrap_or_default();
    let stage_runs = db::list_stage_runs(runtime, &task_id).unwrap_or_default();
    let artifacts = db::list_working_artifacts(runtime, &task_id).unwrap_or_default();
    let human_actions = db::list_human_actions(runtime, &task_id).unwrap_or_default();

    let qa_report = read_artifact_text(runtime, &task_id, "qa_report_md");
    let qa_log = read_artifact_text(runtime, &task_id, "qa_log");
    let review_report = read_artifact_text(runtime, &task_id, "review_md");
    let pr_summary = read_artifact_text(runtime, &task_id, "pr_summary_md");

    Html(ui::render_task_detail(ui::TaskDetailView {
        task: &task,
        transitions: &transitions,
        stage_runs: &stage_runs,
        artifacts: &artifacts,
        human_actions: &human_actions,
        qa_report: qa_report.as_deref(),
        qa_log: qa_log.as_deref(),
        review_report: review_report.as_deref(),
        pr_summary: pr_summary.as_deref(),
    }))
    .into_response()
}

async fn task_artifact(
    State(state): State<AppState>,
    AxumPath((task_id, role)): AxumPath<(String, String)>,
) -> Response {
    let runtime = state.runtime();
    let Some(artifact) =
        db::get_working_artifact_by_role(runtime, &task_id, &role).unwrap_or_default()
    else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Html(format!(
                "<h1>Artifact not found</h1><p>{task_id}/{role}</p>"
            )),
        )
            .into_response();
    };

    let full_path = runtime
        .root
        .join(artifact.relative_path.trim_start_matches(".patron/"));
    match std::fs::read(&full_path) {
        Ok(bytes) => {
            let mut headers = HeaderMap::new();
            if let Ok(content_type) = HeaderValue::from_str(&artifact.media_type) {
                headers.insert(axum::http::header::CONTENT_TYPE, content_type);
            }
            (headers, bytes).into_response()
        }
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!(
                "<h1>Artifact read failed</h1><p>{}: {error}</p>",
                full_path.display()
            )),
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize)]
struct TaskCreateForm {
    goal: String,
}

async fn create_task(State(state): State<AppState>, Form(form): Form<TaskCreateForm>) -> Response {
    match orchestrator::create_draft_task(state.runtime(), &form.goal) {
        Ok(task) => Redirect::to(&format!("/tasks/{}", task.id)).into_response(),
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
        Ok(_) => Redirect::to(&format!("/tasks/{task_id}")).into_response(),
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
        Ok(_) => Redirect::to(&format!("/tasks/{task_id}")).into_response(),
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
        Ok(_) => Redirect::to(&format!("/tasks/{task_id}")).into_response(),
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
        Ok(_) => Redirect::to(&format!("/tasks/{task_id}")).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>Fix loop failed</h1><p>{error}</p>")),
        )
            .into_response(),
    }
}

async fn run_qa(State(state): State<AppState>, AxumPath(task_id): AxumPath<String>) -> Response {
    match qa::run_qa(state.runtime(), &task_id) {
        Ok(_) => Redirect::to(&format!("/tasks/{task_id}")).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>QA failed</h1><p>{error}</p>")),
        )
            .into_response(),
    }
}

async fn run_pr_preparation(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> Response {
    match orchestrator::run_pr_preparation(state.runtime(), &task_id) {
        Ok(_) => Redirect::to(&format!("/tasks/{task_id}")).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>PR preparation failed</h1><p>{error}</p>")),
        )
            .into_response(),
    }
}

fn read_artifact_text(runtime: &RuntimePaths, task_id: &str, role: &str) -> Option<String> {
    let artifact = db::get_working_artifact_by_role(runtime, task_id, role)
        .ok()
        .flatten()?;
    let full_path = runtime
        .root
        .join(artifact.relative_path.trim_start_matches(".patron/"));
    std::fs::read_to_string(full_path).ok()
}
