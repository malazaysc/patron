mod app;
mod bootstrap;
mod db;
mod domain;
mod orchestrator;
mod qa;
mod recovery;
mod runner;
mod ui;

use std::error::Error;
use std::net::SocketAddr;

use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cwd = bootstrap::current_dir()
        .map_err(|error| format!("failed to resolve current working directory: {error}"))?;
    let repo = bootstrap::detect_repo_context(&cwd);
    let runtime = app::RuntimePaths::discover(&repo.repo_root).map_err(|error| {
        format!("failed to resolve the Patron runtime root under .patron/: {error}")
    })?;

    let mut args = std::env::args().skip(1);
    if matches!(args.next().as_deref(), Some("init")) {
        let status = bootstrap::initialize_runtime(&runtime, &repo)?;
        println!("Patron init complete.");
        println!("{}", status.summary());
        return Ok(());
    }

    let bootstrap_status = bootstrap::inspect(&runtime, repo.clone());
    if runtime.state_db.exists() {
        db::initialize(&runtime)
            .map_err(|error| format!("failed to initialize the Patron SQLite state: {error}"))?;
        db::persist_repo_metadata(&runtime, &repo)
            .map_err(|error| format!("failed to persist repository metadata: {error}"))?;
    }
    if bootstrap_status.setup_ready() {
        recovery::reconcile_interrupted_runs(&runtime)
            .map_err(|error| format!("failed to reconcile interrupted Patron runs: {error}"))?;
    }

    let state = app::AppState::new(runtime, bootstrap_status);
    let router = app::build_router(state);
    let address = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(address).await?;

    println!("Patron listening on http://{address}");

    axum::serve(listener, router).await?;

    Ok(())
}
