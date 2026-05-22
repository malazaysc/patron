use crate::db::StateStoreStatus;
use crate::db::TaskRecord;
use crate::domain::TASK_PIPELINE_STAGES;

pub fn render_home(
    runtime_root: &std::path::Path,
    runtime_directories: &[std::path::PathBuf],
    qa_directories: &[std::path::PathBuf],
    state_store: &StateStoreStatus<'_>,
    tasks: &[TaskRecord],
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
    let task_rows = if tasks.is_empty() {
        "<li>No draft tasks yet.</li>".to_string()
    } else {
        tasks.iter()
            .map(|task| {
                format!(
                    "<li><strong>{}</strong> <code>{}</code> [{}]{}<br><small>{}</small><br><small>workspace: <code>{}</code></small>{}</li>",
                    html_escape(&task.title),
                    task.id,
                    task.state,
                    task.current_stage
                        .as_deref()
                        .map(|stage| format!(" stage={stage}"))
                        .unwrap_or_default(),
                    html_escape(&task.goal),
                    task.workspace_path,
                    task_action_buttons(task)
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

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
            <p>State store: <code>{}</code> at <code>{}</code> (schema v{}, migration bytes {})</p>\
            <p>Bootstrap directories created on first run:</p>\
            <ul>{}</ul>\
            <p>QA evidence directories:</p>\
            <ul>{}</ul>\
            <h2>Create draft task</h2>\
            <form action=\"/tasks\" method=\"post\">\
              <label for=\"goal\">Goal</label><br>\
              <textarea id=\"goal\" name=\"goal\" rows=\"6\" cols=\"80\" placeholder=\"Describe the task goal\"></textarea><br>\
              <button type=\"submit\">Create draft task</button>\
            </form>\
            <h2>Tasks</h2>\
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
        state_store.schema_version,
        state_store.initial_schema_bytes,
        directories,
        qa_directories,
        task_rows,
        orchestrator_status,
        runner_status,
        qa_status,
        stages,
    )
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn task_action_buttons(task: &TaskRecord) -> String {
    match task.state.as_str() {
        "draft" | "ready_for_planning" => format!(
            "<form action=\"/tasks/{}/plan\" method=\"post\"><button type=\"submit\">Run planning</button></form>",
            task.id
        ),
        "ready_for_development" => format!(
            "<form action=\"/tasks/{}/develop\" method=\"post\"><button type=\"submit\">Run development</button></form>",
            task.id
        ),
        "ready_for_review" => format!(
            "<form action=\"/tasks/{}/review\" method=\"post\"><button type=\"submit\">Run review</button></form>",
            task.id
        ),
        _ => String::new(),
    }
}
