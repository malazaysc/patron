use crate::db::StateStoreStatus;
use crate::domain::TASK_PIPELINE_STAGES;

pub fn render_home(
    runtime_root: &std::path::Path,
    runtime_directories: &[std::path::PathBuf],
    qa_directories: &[std::path::PathBuf],
    state_store: &StateStoreStatus<'_>,
    orchestrator_status: &str,
    runner_status: &str,
    qa_status: &str,
) -> String {
    let stages = TASK_PIPELINE_STAGES
        .iter()
        .map(|stage| format!("<li>{stage}</li>"))
        .collect::<Vec<_>>()
        .join("");
    let directories = runtime_directories
        .iter()
        .map(|path| format!("<li><code>{}</code></li>", path.display()))
        .collect::<Vec<_>>()
        .join("");
    let qa_directories = qa_directories
        .iter()
        .map(|path| format!("<li><code>{}</code></li>", path.display()))
        .collect::<Vec<_>>()
        .join("");

    format!(
        "<!doctype html>\
        <html lang=\"en\">\
        <head>\
          <meta charset=\"utf-8\">\
          <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
          <title>Patron</title>\
        </head>\
        <body>\
          <main>\
            <h1>Patron</h1>\
            <p>Local-first software delivery harness scaffold.</p>\
            <h2>Runtime</h2>\
            <p>Working state root: <code>{}</code></p>\
            <p>State store: <code>{}</code> at <code>{}</code></p>\
            <p>Bootstrap directories created on first run:</p>\
            <ul>{}</ul>\
            <p>QA evidence directories:</p>\
            <ul>{}</ul>\
            <h2>Subsystems</h2>\
            <ul>\
              <li>Orchestrator: {}</li>\
              <li>Runner: {}</li>\
              <li>QA: {}</li>\
            </ul>\
            <h2>Planned pipeline</h2>\
            <ol>{}</ol>\
            <p>Health endpoint: <code>/health</code></p>\
          </main>\
        </body>\
        </html>",
        runtime_root.display(),
        state_store.engine,
        state_store.location,
        directories,
        qa_directories,
        orchestrator_status,
        runner_status,
        qa_status,
        stages,
    )
}
