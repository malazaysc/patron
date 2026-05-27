use crate::db::HumanActionRecord;
use crate::db::StageRunRecord;
use crate::db::StateStoreStatus;
use crate::db::StateTransitionRecord;
use crate::db::TaskRecord;
use crate::db::WorkingArtifactRecord;
use crate::domain::TASK_PIPELINE_STAGES;

pub struct HomeView<'a> {
    pub runtime_root: &'a std::path::Path,
    pub runtime_directories: &'a [std::path::PathBuf],
    pub qa_directories: &'a [std::path::PathBuf],
    pub state_store: &'a StateStoreStatus<'a>,
    pub tasks: &'a [TaskRecord],
    pub orchestrator_status: &'a str,
    pub runner_status: &'a str,
    pub qa_status: &'a str,
}

pub struct TaskDetailView<'a> {
    pub task: &'a TaskRecord,
    pub transitions: &'a [StateTransitionRecord],
    pub stage_runs: &'a [StageRunRecord],
    pub artifacts: &'a [WorkingArtifactRecord],
    pub human_actions: &'a [HumanActionRecord],
    pub qa_report: Option<&'a str>,
    pub qa_log: Option<&'a str>,
    pub review_report: Option<&'a str>,
    pub pr_summary: Option<&'a str>,
}

pub fn render_home(view: HomeView<'_>) -> String {
    let stages = TASK_PIPELINE_STAGES
        .iter()
        .map(|stage| format!("<li>{stage}</li>"))
        .collect::<Vec<_>>()
        .join("");
    let directories = view
        .runtime_directories
        .iter()
        .map(|path| format!("<li><code>{}</code></li>", path.display()))
        .collect::<Vec<_>>()
        .join("");
    let qa_directories = view
        .qa_directories
        .iter()
        .map(|path| format!("<li><code>{}</code></li>", path.display()))
        .collect::<Vec<_>>()
        .join("");
    let board = board_columns(view.tasks);

    format!(
        "<!doctype html>\
        <html lang=\"en\">\
        <head>\
          <meta charset=\"utf-8\">\
          <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
          <meta http-equiv=\"refresh\" content=\"5\">\
          <title>Patron</title>\
          <style>\
            body {{ font-family: ui-sans-serif, system-ui, sans-serif; margin: 0; background: #f6f0e3; color: #1f1b16; }}\
            main {{ max-width: 1440px; margin: 0 auto; padding: 24px; }}\
            .board {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 16px; align-items: start; }}\
            .column {{ background: #fff9ef; border: 1px solid #d7ccb8; border-radius: 14px; padding: 12px; box-shadow: 0 10px 24px rgba(76, 58, 22, 0.08); }}\
            .column h3 {{ margin: 0 0 12px 0; }}\
            .column.blocked {{ background: #fff2ef; border-color: #c65d43; }}\
            .column.awaiting-human {{ background: #fff8dc; border-color: #b5901a; }}\
            .task-card {{ background: #ffffff; border: 1px solid #e5ddc9; border-radius: 10px; padding: 10px; margin-bottom: 10px; }}\
            .task-card:last-child {{ margin-bottom: 0; }}\
            .task-card p {{ margin: 8px 0; }}\
            .task-card form {{ margin-top: 8px; }}\
            .pill {{ display: inline-block; padding: 2px 8px; border-radius: 999px; background: #eee4cf; font-size: 0.8rem; margin-right: 6px; }}\
            .pill.alert {{ background: #f6d5cf; color: #7a1b0e; }}\
            .pill.waiting {{ background: #f7e6a3; color: #5e4d0c; }}\
            textarea {{ width: min(100%, 880px); }}\
          </style>\
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
            <h2>Autopilot Board</h2>\
            <div class=\"board\">{}</div>\
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
        view.runtime_root.display(),
        view.state_store.engine,
        view.state_store.location,
        view.state_store.schema_version,
        view.state_store.initial_schema_bytes,
        directories,
        qa_directories,
        board,
        view.orchestrator_status,
        view.runner_status,
        view.qa_status,
        stages,
    )
}

fn board_columns(tasks: &[TaskRecord]) -> String {
    let states = [
        ("draft", "Draft"),
        ("ready_for_planning", "Ready For Planning"),
        ("planning", "Planning"),
        ("ready_for_development", "Ready For Development"),
        ("developing", "Developing"),
        ("ready_for_review", "Ready For Review"),
        ("reviewing", "Reviewing"),
        ("ready_for_qa", "Ready For QA"),
        ("qa_running", "QA Running"),
        ("fix_required", "Fix Required"),
        ("ready_for_pr", "Ready For PR"),
        ("pr_prepared", "PR Prepared"),
        ("awaiting_human", "Awaiting Human"),
        ("blocked", "Blocked"),
    ];

    states
        .iter()
        .map(|(state, label)| {
            let cards = tasks
                .iter()
                .filter(|task| task.state == *state)
                .map(render_task_card)
                .collect::<Vec<_>>()
                .join("");
            let body = if cards.is_empty() {
                "<p><small>No tasks in this state.</small></p>".to_string()
            } else {
                cards
            };
            let class_name = match *state {
                "blocked" => "column blocked",
                "awaiting_human" => "column awaiting-human",
                _ => "column",
            };
            format!(
                "<section class=\"{}\"><h3>{}</h3>{}</section>",
                class_name, label, body
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_task_card(task: &TaskRecord) -> String {
    let actions = task_action_buttons(task);
    let mut pills = Vec::new();
    if let Some(stage) = task.current_stage.as_deref() {
        pills.push(format!(
            "<span class=\"pill\">stage={}</span>",
            html_escape(stage)
        ));
    }
    if task.state == "blocked" {
        pills.push("<span class=\"pill alert\">blocked</span>".to_string());
    }
    if task.state == "awaiting_human" {
        pills.push("<span class=\"pill waiting\">human action required</span>".to_string());
    }

    let blocked_reason = task
        .blocked_reason_text
        .as_deref()
        .map(|reason| format!("<p><small>blocked: {}</small></p>", html_escape(reason)))
        .unwrap_or_default();

    format!(
        "<article id=\"task-{}\" class=\"task-card\"><strong><a href=\"/tasks/{}\">{}</a></strong><br><code>{}</code><p>{}</p><p>{}</p><small>workspace: <code>{}</code></small>{}{}</article>",
        task.id,
        task.id,
        html_escape(&task.title),
        task.id,
        pills.join(""),
        html_escape(&task.goal),
        html_escape(&task.workspace_path),
        blocked_reason,
        actions
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
        "ready_for_qa" => format!(
            "<form action=\"/tasks/{}/qa\" method=\"post\"><button type=\"submit\">Run QA</button></form>",
            task.id
        ),
        "ready_for_pr" => format!(
            "<form action=\"/tasks/{}/prepare-pr\" method=\"post\"><button type=\"submit\">Prepare PR Handoff</button></form>",
            task.id
        ),
        "fix_required" => format!(
            "<form action=\"/tasks/{}/fix\" method=\"post\"><button type=\"submit\">Run fix loop</button></form>",
            task.id
        ),
        _ => String::new(),
    }
}

pub fn render_task_detail(view: TaskDetailView<'_>) -> String {
    let transitions = if view.transitions.is_empty() {
        "<li>No state transitions recorded yet.</li>".to_string()
    } else {
        view.transitions
            .iter()
            .map(|transition| {
                format!(
                    "<li><code>{}</code>: {} -> <strong>{}</strong> via {}{}<br><small>{}</small>{}</li>",
                    html_escape(&transition.created_at),
                    transition.from_state.as_deref().unwrap_or("none"),
                    html_escape(&transition.to_state),
                    html_escape(&transition.actor_kind),
                    transition
                        .stage_run_id
                        .as_deref()
                        .map(|run_id| format!(" run=<code>{}</code>", html_escape(run_id)))
                        .unwrap_or_default(),
                    html_escape(&transition.reason_text),
                    transition
                        .reason_code
                        .as_deref()
                        .map(|reason| format!("<br><small>reason_code: <code>{}</code></small>", html_escape(reason)))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let stage_runs = if view.stage_runs.is_empty() {
        "<li>No stage runs recorded yet.</li>".to_string()
    } else {
        view.stage_runs
            .iter()
            .map(|run| {
                format!(
                    "<li><strong>{}</strong> attempt {} [{}] exit={}<br><small>started {}</small>{}{}</li>",
                    html_escape(&run.id),
                    run.attempt_number,
                    html_escape(&run.stage),
                    run.exit_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "n/a".to_string()),
                    html_escape(&run.started_at),
                    run.finished_at
                        .as_deref()
                        .map(|finished| format!("<br><small>finished {}</small>", html_escape(finished)))
                        .unwrap_or_default(),
                    run.error_summary
                        .as_deref()
                        .map(|summary| format!("<br><small>error: {}</small>", html_escape(summary)))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let artifacts = if view.artifacts.is_empty() {
        "<li>No artifacts recorded yet.</li>".to_string()
    } else {
        view.artifacts
            .iter()
            .map(|artifact| {
                let artifact_label = if artifact.media_type == "inode/directory" {
                    html_escape(&artifact.role)
                } else {
                    format!(
                        "<a href=\"/tasks/{}/artifacts/{}\">{}</a>",
                        view.task.id,
                        html_escape(&artifact.role),
                        html_escape(&artifact.role)
                    )
                };
                format!(
                    "<li>{} <code>{}</code>{}<br><small>{}</small></li>",
                    artifact_label,
                    html_escape(&artifact.media_type),
                    if artifact.required_for_stage {
                        " <strong>(required)</strong>"
                    } else {
                        ""
                    },
                    html_escape(&artifact.relative_path)
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let qa_report = view
        .qa_report
        .map(render_preformatted)
        .unwrap_or_else(|| "<p>No QA report has been generated yet.</p>".to_string());
    let qa_log = view
        .qa_log
        .map(render_preformatted)
        .unwrap_or_else(|| "<p>No QA log has been recorded yet.</p>".to_string());
    let review_report = view
        .review_report
        .map(render_preformatted)
        .unwrap_or_else(|| "<p>No review report has been generated yet.</p>".to_string());
    let pr_summary = view
        .pr_summary
        .map(render_preformatted)
        .unwrap_or_else(|| "<p>No PR summary has been prepared yet.</p>".to_string());
    let human_actions = if view.human_actions.is_empty() {
        "<li>No required human actions are currently recorded.</li>".to_string()
    } else {
        view.human_actions
            .iter()
            .map(|action| {
                format!(
                    "<li><strong>{}</strong> [{}] requested by {} at {}<br><small>{}</small>{}</li>",
                    html_escape(&action.action_type),
                    html_escape(&action.status),
                    html_escape(&action.requested_by),
                    html_escape(&action.requested_at),
                    html_escape(&action.instructions),
                    action
                        .resolution_notes
                        .as_deref()
                        .map(|notes| format!("<br><small>resolution: {}</small>", html_escape(notes)))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let qa_screenshot = view
        .artifacts
        .iter()
        .find(|artifact| artifact.role == "qa_screenshot")
        .map(|_| {
            format!(
                "<img src=\"/tasks/{}/artifacts/qa_screenshot\" alt=\"QA screenshot for {}\" style=\"max-width: 100%; border: 1px solid #ccc;\">",
                view.task.id,
                html_escape(&view.task.id)
            )
        })
        .unwrap_or_else(|| "<p>No QA screenshot has been captured yet.</p>".to_string());

    format!(
        "<!doctype html>\
        <html lang=\"en\">\
        <head>\
          <meta charset=\"utf-8\">\
          <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
          <title>{}</title>\
        </head>\
        <body>\
          <main>\
            <p><a href=\"/\">Back to task list</a></p>\
            <h1>{}</h1>\
            <p><code>{}</code> [{}]{} </p>\
            <p>{}</p>\
            <p>Workspace: <code>{}</code></p>\
            {}\
            {}\
            <h2>Required Human Actions</h2>\
            <ul>{}</ul>\
            <h2>QA Report</h2>\
            {}\
            <h2>QA Evidence</h2>\
            <p><a href=\"/tasks/{}/artifacts/qa_log\">Open QA log</a> | <a href=\"/tasks/{}/artifacts/qa_har\">Open QA HAR</a> | <a href=\"/tasks/{}/artifacts/qa_screenshot\">Open QA screenshot</a></p>\
            {}\
            <h3>QA Log</h3>\
            {}\
            <h2>Review</h2>\
            {}\
            <h2>PR Summary</h2>\
            {}\
            <h2>Artifacts</h2>\
            <ul>{}</ul>\
            <h2>State History</h2>\
            <ul>{}</ul>\
            <h2>Stage Runs</h2>\
            <ul>{}</ul>\
          </main>\
        </body>\
        </html>",
        html_escape(&view.task.title),
        html_escape(&view.task.title),
        view.task.id,
        view.task.state,
        view.task
            .current_stage
            .as_deref()
            .map(|stage| format!(" stage={}", html_escape(stage)))
            .unwrap_or_default(),
        html_escape(&view.task.goal),
        html_escape(&view.task.workspace_path),
        render_blocked_reason(view.task),
        task_action_buttons(view.task),
        human_actions,
        qa_report,
        view.task.id,
        view.task.id,
        view.task.id,
        qa_screenshot,
        qa_log,
        review_report,
        pr_summary,
        artifacts,
        transitions,
        stage_runs
    )
}

fn render_preformatted(value: &str) -> String {
    format!(
        "<pre style=\"white-space: pre-wrap; border: 1px solid #ccc; padding: 12px; overflow-x: auto;\">{}</pre>",
        html_escape(value)
    )
}

fn render_blocked_reason(task: &TaskRecord) -> String {
    match (
        task.blocked_reason_code.as_deref(),
        task.blocked_reason_text.as_deref(),
    ) {
        (Some(code), Some(text)) => format!(
            "<p><strong>Blocked reason</strong>: <code>{}</code> {}</p>",
            html_escape(code),
            html_escape(text)
        ),
        _ => String::new(),
    }
}
