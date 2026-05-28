use std::io;
use std::path::{Path, PathBuf};

use axum::{
    Form, Router,
    extract::Path as AxumPath,
    extract::Query,
    extract::State,
    http::{HeaderMap, HeaderValue},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};

use crate::{bootstrap::BootstrapStatus, db, orchestrator, qa, runner, ui};

#[derive(Default, serde::Deserialize)]
struct TaskPageQuery {
    queued: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    runtime: RuntimePaths,
    bootstrap: BootstrapStatus,
}

impl AppState {
    pub fn new(runtime: RuntimePaths, bootstrap: BootstrapStatus) -> Self {
        Self { runtime, bootstrap }
    }

    pub fn runtime(&self) -> &RuntimePaths {
        &self.runtime
    }

    pub fn bootstrap(&self) -> &BootstrapStatus {
        &self.bootstrap
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
    pub fn discover(repo_root: &Path) -> io::Result<Self> {
        let root = repo_root.join(".patron");

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

    pub fn repo_root(&self) -> &Path {
        self.root.parent().unwrap_or(self.root.as_path())
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/setup", get(setup))
        .route("/intake", get(intake_index).post(start_intake))
        .route("/intake/{session_id}", get(intake_detail))
        .route("/intake/{session_id}/reply", post(reply_intake))
        .route("/intake/{session_id}/approve", post(approve_intake))
        .route("/board", get(board))
        .route("/tasks", get(tasks_index).post(create_task))
        .route("/runs", get(runs_index))
        .route("/sample-app", get(sample_app))
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
    if !state.bootstrap().setup_ready() {
        return Html(ui::render_setup(ui::SetupView {
            bootstrap: state.bootstrap(),
        }));
    }

    let runtime = state.runtime();
    let task_snapshot = db::list_tasks(runtime).unwrap_or_default();
    let state_store = db::state_store_status(runtime);
    let active_runs = db::list_open_stage_runs(runtime).unwrap_or_default();
    let recent_runs = db::list_recent_stage_runs(runtime, 12).unwrap_or_default();
    let activity = db::list_recent_activity_events(runtime, 16).unwrap_or_default();
    let intake_sessions = db::list_intake_sessions(runtime, 8).unwrap_or_default();
    let body = ui::render_dashboard(ui::DashboardView {
        bootstrap: state.bootstrap(),
        runtime_root: &runtime.relative_to_repo(&runtime.root),
        state_store: &state_store,
        tasks: &task_snapshot,
        active_runs: &active_runs,
        recent_runs: &recent_runs,
        intake_sessions: &intake_sessions,
        activity: &activity,
        orchestrator_status: orchestrator::status_label(),
        runner_status: runner::status_label(),
        qa_status: qa::status_label(),
    });

    Html(body)
}

async fn setup(State(state): State<AppState>) -> Html<String> {
    Html(ui::render_setup(ui::SetupView {
        bootstrap: state.bootstrap(),
    }))
}

async fn board(State(state): State<AppState>) -> Html<String> {
    if !state.bootstrap().setup_ready() {
        return Html(ui::render_setup(ui::SetupView {
            bootstrap: state.bootstrap(),
        }));
    }
    let runtime = state.runtime();
    let task_snapshot = db::list_tasks(runtime).unwrap_or_default();
    let active_runs = db::list_open_stage_runs(runtime).unwrap_or_default();
    let activity = db::list_recent_activity_events(runtime, 20).unwrap_or_default();
    Html(ui::render_board(ui::BoardView {
        tasks: &task_snapshot,
        active_runs: &active_runs,
        activity: &activity,
    }))
}

async fn tasks_index(State(state): State<AppState>) -> Html<String> {
    if !state.bootstrap().setup_ready() {
        return Html(ui::render_setup(ui::SetupView {
            bootstrap: state.bootstrap(),
        }));
    }
    let runtime = state.runtime();
    let task_snapshot = db::list_tasks(runtime).unwrap_or_default();
    let active_runs = db::list_open_stage_runs(runtime).unwrap_or_default();
    let activity = db::list_recent_activity_events(runtime, 20).unwrap_or_default();
    Html(ui::render_tasks_index(ui::TaskListView {
        tasks: &task_snapshot,
        active_runs: &active_runs,
        activity: &activity,
    }))
}

async fn runs_index(State(state): State<AppState>) -> Html<String> {
    if !state.bootstrap().setup_ready() {
        return Html(ui::render_setup(ui::SetupView {
            bootstrap: state.bootstrap(),
        }));
    }
    let runtime = state.runtime();
    let task_snapshot = db::list_tasks(runtime).unwrap_or_default();
    let active_runs = db::list_open_stage_runs(runtime).unwrap_or_default();
    let recent_runs = db::list_recent_stage_runs(runtime, 64).unwrap_or_default();
    let activity = db::list_recent_activity_events(runtime, 24).unwrap_or_default();
    Html(ui::render_runs(ui::RunsView {
        tasks: &task_snapshot,
        active_runs: &active_runs,
        runs: &recent_runs,
        activity: &activity,
    }))
}

async fn intake_index(State(state): State<AppState>) -> Html<String> {
    if !state.bootstrap().setup_ready() {
        return Html(ui::render_setup(ui::SetupView {
            bootstrap: state.bootstrap(),
        }));
    }
    let runtime = state.runtime();
    let sessions = db::list_intake_sessions(runtime, 24).unwrap_or_default();
    let activity = db::list_recent_activity_events(runtime, 24).unwrap_or_default();
    Html(ui::render_intake_index(ui::IntakeIndexView {
        sessions: &sessions,
        activity: &activity,
    }))
}

async fn health() -> &'static str {
    "ok"
}

async fn task_detail(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
    Query(query): Query<TaskPageQuery>,
) -> Response {
    if !state.bootstrap().setup_ready() {
        return Html(ui::render_setup(ui::SetupView {
            bootstrap: state.bootstrap(),
        }))
        .into_response();
    }
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
    let latest_run = db::get_latest_stage_run(runtime, &task_id).unwrap_or_default();
    let active_runs = db::list_open_stage_runs(runtime).unwrap_or_default();
    let activity = db::list_task_activity_events(runtime, &task_id, 20).unwrap_or_default();
    let artifacts = db::list_working_artifacts(runtime, &task_id).unwrap_or_default();
    let human_actions = db::list_human_actions(runtime, &task_id).unwrap_or_default();

    let qa_report = read_artifact_text(runtime, &task_id, "qa_report_md");
    let qa_log = read_artifact_text(runtime, &task_id, "qa_log");
    let review_report = read_artifact_text(runtime, &task_id, "review_md");
    let pr_summary = read_artifact_text(runtime, &task_id, "pr_summary_md");
    let live_log = latest_run
        .as_ref()
        .and_then(|run| read_run_tail(runtime, &task_id, &run.id, 24));

    Html(ui::render_task_detail(ui::TaskDetailView {
        task: &task,
        queued_stage: query.queued.as_deref(),
        active_runs: &active_runs,
        activity: &activity,
        transitions: &transitions,
        stage_runs: &stage_runs,
        live_log: live_log.as_deref(),
        artifacts: &artifacts,
        human_actions: &human_actions,
        qa_report: qa_report.as_deref(),
        qa_log: qa_log.as_deref(),
        review_report: review_report.as_deref(),
        pr_summary: pr_summary.as_deref(),
    }))
    .into_response()
}

async fn intake_detail(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> Response {
    if !state.bootstrap().setup_ready() {
        return Html(ui::render_setup(ui::SetupView {
            bootstrap: state.bootstrap(),
        }))
        .into_response();
    }
    let runtime = state.runtime();
    let Some(session) = db::get_intake_session(runtime, &session_id).unwrap_or_default() else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Html(format!(
                "<h1>Intake session not found</h1><p>{session_id}</p>"
            )),
        )
            .into_response();
    };
    let messages = db::list_intake_messages(runtime, &session_id).unwrap_or_default();
    let activity = db::list_recent_activity_events(runtime, 24).unwrap_or_default();
    Html(ui::render_intake_detail(ui::IntakeDetailView {
        session: &session,
        messages: &messages,
        activity: &activity,
    }))
    .into_response()
}

async fn sample_app(State(state): State<AppState>) -> Response {
    let fixture_path = state
        .runtime()
        .repo_root()
        .join("fixtures/sample-app/index.html");
    match std::fs::read_to_string(&fixture_path) {
        Ok(contents) => Html(contents).into_response(),
        Err(error) => (
            axum::http::StatusCode::NOT_FOUND,
            Html(format!(
                "<h1>Sample app fixture not found</h1><p>{}: {error}</p>",
                fixture_path.display()
            )),
        )
            .into_response(),
    }
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

#[derive(serde::Deserialize)]
struct IntakeStartForm {
    goal: String,
}

#[derive(serde::Deserialize)]
struct IntakeReplyForm {
    message: String,
}

async fn create_task(State(state): State<AppState>, Form(form): Form<TaskCreateForm>) -> Response {
    if !state.bootstrap().setup_ready() {
        return (
            axum::http::StatusCode::PRECONDITION_FAILED,
            Html(ui::render_setup(ui::SetupView {
                bootstrap: state.bootstrap(),
            })),
        )
            .into_response();
    }
    match orchestrator::create_draft_task(state.runtime(), &form.goal) {
        Ok(task) => Redirect::to(&format!("/tasks/{}", task.id)).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>Task creation failed</h1><p>{error}</p>")),
        )
            .into_response(),
    }
}

async fn start_intake(
    State(state): State<AppState>,
    Form(form): Form<IntakeStartForm>,
) -> Response {
    if !state.bootstrap().setup_ready() {
        return Redirect::to("/setup").into_response();
    }
    match orchestrator::start_intake_session(state.runtime(), &form.goal) {
        Ok(session) => Redirect::to(&format!("/intake/{}", session.id)).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>Intake start failed</h1><p>{error}</p>")),
        )
            .into_response(),
    }
}

async fn reply_intake(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Form(form): Form<IntakeReplyForm>,
) -> Response {
    if !state.bootstrap().setup_ready() {
        return Redirect::to("/setup").into_response();
    }
    match orchestrator::reply_intake_session(state.runtime(), &session_id, &form.message) {
        Ok(session) => Redirect::to(&format!("/intake/{}", session.id)).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>Intake reply failed</h1><p>{error}</p>")),
        )
            .into_response(),
    }
}

async fn approve_intake(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> Response {
    if !state.bootstrap().setup_ready() {
        return Redirect::to("/setup").into_response();
    }
    match orchestrator::approve_intake_session(state.runtime(), &session_id) {
        Ok(task) => Redirect::to(&format!("/tasks/{}", task.id)).into_response(),
        Err(error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("<h1>Intake approval failed</h1><p>{error}</p>")),
        )
            .into_response(),
    }
}

async fn run_planning(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> Response {
    if !state.bootstrap().setup_ready() {
        return Redirect::to("/setup").into_response();
    }
    spawn_stage_work(
        state.runtime().clone(),
        task_id.clone(),
        "planning",
        |runtime, task_id| orchestrator::run_planning(&runtime, &task_id),
    );
    Redirect::to(&format!("/tasks/{task_id}?queued=planning")).into_response()
}

async fn run_development(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> Response {
    if !state.bootstrap().setup_ready() {
        return Redirect::to("/setup").into_response();
    }
    spawn_stage_work(
        state.runtime().clone(),
        task_id.clone(),
        "development",
        |runtime, task_id| orchestrator::run_development(&runtime, &task_id),
    );
    Redirect::to(&format!("/tasks/{task_id}?queued=development")).into_response()
}

async fn run_review(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> Response {
    if !state.bootstrap().setup_ready() {
        return Redirect::to("/setup").into_response();
    }
    spawn_stage_work(
        state.runtime().clone(),
        task_id.clone(),
        "review",
        |runtime, task_id| orchestrator::run_review(&runtime, &task_id),
    );
    Redirect::to(&format!("/tasks/{task_id}?queued=review")).into_response()
}

async fn run_fix_loop(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> Response {
    if !state.bootstrap().setup_ready() {
        return Redirect::to("/setup").into_response();
    }
    spawn_stage_work(
        state.runtime().clone(),
        task_id.clone(),
        "fix",
        |runtime, task_id| orchestrator::run_fix_loop(&runtime, &task_id),
    );
    Redirect::to(&format!("/tasks/{task_id}?queued=fix")).into_response()
}

async fn run_qa(State(state): State<AppState>, AxumPath(task_id): AxumPath<String>) -> Response {
    if !state.bootstrap().setup_ready() {
        return Redirect::to("/setup").into_response();
    }
    spawn_stage_work(
        state.runtime().clone(),
        task_id.clone(),
        "qa",
        |runtime, task_id| qa::run_qa(&runtime, &task_id),
    );
    Redirect::to(&format!("/tasks/{task_id}?queued=qa")).into_response()
}

async fn run_pr_preparation(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> Response {
    if !state.bootstrap().setup_ready() {
        return Redirect::to("/setup").into_response();
    }
    spawn_stage_work(
        state.runtime().clone(),
        task_id.clone(),
        "pr_preparation",
        |runtime, task_id| orchestrator::run_pr_preparation(&runtime, &task_id),
    );
    Redirect::to(&format!("/tasks/{task_id}?queued=pr_preparation")).into_response()
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

fn read_run_tail(
    runtime: &RuntimePaths,
    task_id: &str,
    run_id: &str,
    max_lines: usize,
) -> Option<String> {
    let log_path = runtime.runs_dir.join(task_id).join(format!("{run_id}.log"));
    let contents = std::fs::read_to_string(log_path).ok()?;
    let lines = contents.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(max_lines);
    Some(lines[start..].join("\n"))
}

fn spawn_stage_work<F>(runtime: RuntimePaths, task_id: String, stage_name: &'static str, work: F)
where
    F: FnOnce(RuntimePaths, String) -> Result<(), String> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        if let Err(error) = work(runtime, task_id.clone()) {
            eprintln!("background stage `{stage_name}` failed for {task_id}: {error}");
        }
    });
}
