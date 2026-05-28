mod app;
mod bootstrap;
mod cli;
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
    let command = match cli::parse() {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}");
            eprintln!();
            eprintln!("{}", cli::help_text());
            std::process::exit(2);
        }
    };

    let cwd = bootstrap::current_dir()
        .map_err(|error| format!("failed to resolve current working directory: {error}"))?;
    let repo = bootstrap::detect_repo_context(&cwd);
    let runtime = app::RuntimePaths::discover(&repo.repo_root).map_err(|error| {
        format!("failed to resolve the Patron runtime root under .patron/: {error}")
    })?;

    match command {
        cli::Command::Help => {
            println!("{}", cli::help_text());
            return Ok(());
        }
        cli::Command::Version => {
            println!("patron {}", cli::version_text());
            return Ok(());
        }
        cli::Command::Doctor => {
            let status = bootstrap::inspect(&runtime, repo);
            println!("{}", status.summary());
            return Ok(());
        }
        cli::Command::Init { init_git } => {
            if init_git {
                let (_, status) = bootstrap::initialize_runtime_with_git(&cwd)?;
                println!("Patron init complete.");
                println!("{}", status.summary());
                return Ok(());
            }
            let status = bootstrap::initialize_runtime(&runtime, &repo)?;
            println!("Patron init complete.");
            println!("{}", status.summary());
            return Ok(());
        }
        cli::Command::Serve => {}
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
