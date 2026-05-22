use std::io;
use std::path::{Path, PathBuf};

use axum::{Router, extract::State, response::Html, routing::get};

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
    pub runs_dir: PathBuf,
    pub tasks_dir: PathBuf,
}

impl RuntimePaths {
    pub fn discover() -> io::Result<Self> {
        let root = std::env::current_dir()?.join(".patron");

        Ok(Self {
            state_db: root.join("state.db"),
            runs_dir: root.join("runs"),
            tasks_dir: root.join("tasks"),
            root,
        })
    }

    pub fn ensure_layout(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(&self.runs_dir)?;
        std::fs::create_dir_all(&self.tasks_dir)?;

        if !self.state_db.exists() {
            std::fs::File::create(&self.state_db)?;
        }

        Ok(())
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
        .with_state(state)
}

async fn index(State(state): State<AppState>) -> Html<String> {
    let runtime = state.runtime();
    let body = ui::render_home(
        &runtime.relative_to_repo(&runtime.root),
        &db::state_store_status(runtime),
        &orchestrator::status_label(),
        &runner::status_label(),
        &qa::status_label(),
    );

    Html(body)
}

async fn health() -> &'static str {
    "ok"
}
