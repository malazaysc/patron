mod app;
mod db;
mod domain;
mod orchestrator;
mod qa;
mod runner;
mod ui;

use std::error::Error;
use std::net::SocketAddr;

use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let runtime = app::RuntimePaths::discover().map_err(|error| {
        format!("failed to resolve the Patron runtime root under .patron/: {error}")
    })?;
    runtime
        .ensure_layout()
        .map_err(|error| format!("failed to initialize the Patron runtime layout: {error}"))?;
    db::initialize(&runtime)
        .map_err(|error| format!("failed to initialize the Patron SQLite state: {error}"))?;

    let state = app::AppState::new(runtime);
    let router = app::build_router(state);
    let address = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(address).await?;

    println!("Patron listening on http://{address}");

    axum::serve(listener, router).await?;

    Ok(())
}
