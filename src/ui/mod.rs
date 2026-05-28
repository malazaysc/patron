use std::collections::HashMap;

use crate::bootstrap::BootstrapStatus;
use crate::db::{
    HumanActionRecord, StageRunRecord, StateStoreStatus, StateTransitionRecord, TaskRecord,
    WorkingArtifactRecord,
};
use crate::domain::TASK_PIPELINE_STAGES;

pub struct SetupView<'a> {
    pub bootstrap: &'a BootstrapStatus,
}

pub struct DashboardView<'a> {
    pub bootstrap: &'a BootstrapStatus,
    pub runtime_root: &'a std::path::Path,
    pub state_store: &'a StateStoreStatus<'a>,
    pub tasks: &'a [TaskRecord],
    pub recent_runs: &'a [StageRunRecord],
    pub orchestrator_status: &'a str,
    pub runner_status: &'a str,
    pub qa_status: &'a str,
}

pub struct BoardView<'a> {
    pub tasks: &'a [TaskRecord],
}

pub struct TaskListView<'a> {
    pub tasks: &'a [TaskRecord],
}

pub struct RunsView<'a> {
    pub tasks: &'a [TaskRecord],
    pub runs: &'a [StageRunRecord],
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

pub fn render_setup(view: SetupView<'_>) -> String {
    let blockers = if view.bootstrap.blockers.is_empty() {
        "<li>No blocking issues detected.</li>".to_string()
    } else {
        view.bootstrap
            .blockers
            .iter()
            .map(|entry| format!("<li>{}</li>", html_escape(entry)))
            .collect::<Vec<_>>()
            .join("")
    };
    let warnings = if view.bootstrap.warnings.is_empty() {
        "<li>No warnings detected.</li>".to_string()
    } else {
        view.bootstrap
            .warnings
            .iter()
            .map(|entry| format!("<li>{}</li>", html_escape(entry)))
            .collect::<Vec<_>>()
            .join("")
    };
    let requirements = view
        .bootstrap
        .requirements
        .iter()
        .map(|requirement| {
            format!(
                "<li><strong>{}</strong> <span class=\"status-badge status-{}\">{}</span><br><small>{}</small></li>",
                html_escape(requirement.name),
                if requirement.ok { "ready-for-development" } else { "blocked" },
                if requirement.ok { "ok" } else { "missing" },
                html_escape(&requirement.detail)
            )
        })
        .collect::<Vec<_>>()
        .join("");

    let body = format!(
        "<section class=\"hero\">\
           <div>\
             <p class=\"eyebrow\">Setup</p>\
             <h1>Initialize Patron for this repository.</h1>\
             <p class=\"lede\">Patron no longer creates runtime state implicitly on server start. Run the explicit init step, then reload the app.</p>\
             <div class=\"hero-actions\">\
               <code>patron init</code>\
               <a class=\"button secondary\" href=\"https://github.com/malazaysc/patron/blob/main/README.md\">Open Getting Started Guide</a>\
             </div>\
           </div>\
           <div class=\"hero-panel\">\
             <div class=\"metric\"><span class=\"metric-value\">{}</span><span class=\"metric-label\">Blockers</span></div>\
             <div class=\"metric\"><span class=\"metric-value\">{}</span><span class=\"metric-label\">Warnings</span></div>\
             <div class=\"metric\"><span class=\"metric-value\">{}</span><span class=\"metric-label\">Repo Ready</span></div>\
             <div class=\"metric\"><span class=\"metric-value\">{}</span><span class=\"metric-label\">Runtime Ready</span></div>\
           </div>\
         </section>\
         <section class=\"dashboard-grid\">\
           <div class=\"dashboard-main\">\
             <section class=\"panel\">\
               <div class=\"panel-header\"><div><p class=\"eyebrow\">Repository</p><h2>{}</h2></div></div>\
               <ul class=\"plain-list\">\
                 <li><strong>Repo root</strong><br><small><code>{}</code></small></li>\
                 <li><strong>Branch</strong><br><small><code>{}</code></small></li>\
                 <li><strong>Runtime root</strong><br><small><code>{}</code></small></li>\
               </ul>\
             </section>\
             <section class=\"panel\">\
               <div class=\"panel-header\"><div><p class=\"eyebrow\">Checks</p><h2>Requirements</h2></div></div>\
               <ul class=\"plain-list\">{}</ul>\
             </section>\
           </div>\
           <aside class=\"dashboard-side\">\
             <section class=\"panel attention-panel\">\
               <div class=\"panel-header\"><div><p class=\"eyebrow\">Action</p><h2>Blockers</h2></div></div>\
               <ul class=\"plain-list\">{}</ul>\
             </section>\
             <section class=\"panel\">\
               <div class=\"panel-header\"><div><p class=\"eyebrow\">Heads Up</p><h2>Warnings</h2></div></div>\
               <ul class=\"plain-list\">{}</ul>\
             </section>\
           </aside>\
         </section>",
        view.bootstrap.blockers.len(),
        view.bootstrap.warnings.len(),
        if view.bootstrap.repo.is_git_repo {
            "yes"
        } else {
            "no"
        },
        if view.bootstrap.initialized {
            "yes"
        } else {
            "no"
        },
        html_escape(&view.bootstrap.repo.repo_name),
        html_escape(&view.bootstrap.repo.repo_root.display().to_string()),
        html_escape(
            view.bootstrap
                .repo
                .git_branch
                .as_deref()
                .unwrap_or("unknown")
        ),
        html_escape(&view.bootstrap.runtime_root.display().to_string()),
        requirements,
        blockers,
        warnings
    );

    app_shell("setup", "Setup", &body, "")
}

pub fn render_dashboard(view: DashboardView<'_>) -> String {
    let total_tasks = view.tasks.len();
    let blocked = count_state(view.tasks, "blocked");
    let awaiting_human = count_state(view.tasks, "awaiting_human");
    let active = view
        .tasks
        .iter()
        .filter(|task| {
            matches!(
                task.state.as_str(),
                "planning" | "developing" | "reviewing" | "qa_running"
            )
        })
        .count();
    let ready = view
        .tasks
        .iter()
        .filter(|task| {
            matches!(
                task.state.as_str(),
                "ready_for_planning"
                    | "ready_for_development"
                    | "ready_for_review"
                    | "ready_for_qa"
                    | "ready_for_pr"
            )
        })
        .count();

    let hero = format!(
        "<section class=\"hero\">\
            <div>\
              <p class=\"eyebrow\">Visual Autopilot</p>\
              <h1>Keep the repo moving without losing the plot.</h1>\
              <p class=\"lede\">Patron now has a dedicated dashboard, board, runs feed, and task detail views so you can understand the system at a glance instead of deciphering one long page.</p>\
              <div class=\"hero-actions\">\
                <a class=\"button primary\" href=\"/board\">Open Workflow Board</a>\
                <a class=\"button secondary\" href=\"/runs\">Inspect Runs</a>\
              </div>\
            </div>\
            <div class=\"hero-panel\">\
              <div class=\"metric\"><span class=\"metric-value\">{}</span><span class=\"metric-label\">Total Tasks</span></div>\
              <div class=\"metric\"><span class=\"metric-value\">{}</span><span class=\"metric-label\">Ready Queues</span></div>\
              <div class=\"metric\"><span class=\"metric-value\">{}</span><span class=\"metric-label\">Active Stages</span></div>\
              <div class=\"metric alert\"><span class=\"metric-value\">{}</span><span class=\"metric-label\">Needs Attention</span></div>\
            </div>\
          </section>",
        total_tasks,
        ready,
        active,
        blocked + awaiting_human
    );

    let intake = "<section class=\"panel intake-panel\">\
            <div class=\"panel-header\">\
              <div><p class=\"eyebrow\">New Work</p><h2>Create Task</h2></div>\
              <p class=\"section-copy\">Start from a free-form goal. Patron will convert it into a structured delivery pipeline.</p>\
            </div>\
            <form action=\"/tasks\" method=\"post\" class=\"intake-form\">\
              <label for=\"goal\">Goal</label>\
              <textarea id=\"goal\" name=\"goal\" rows=\"8\" placeholder=\"Describe the outcome you want and any constraints that matter.\"></textarea>\
              <div class=\"form-footer\">\
                <p>Planning begins immediately in v1; plan approval is not required by default.</p>\
                <button class=\"button primary\" type=\"submit\">Create Draft Task</button>\
              </div>\
            </form>\
          </section>"
        .to_string();

    let attention = format!(
        "<section class=\"panel attention-grid\">\
          {}\
          {}\
        </section>",
        attention_panel(
            "Blocked",
            "These tasks need manual intervention or recovery before the pipeline can continue.",
            &filter_tasks(view.tasks, &["blocked"]),
            "blocked"
        ),
        attention_panel(
            "Awaiting Human",
            "These tasks are staged and ready for a person to review, approve, or merge.",
            &filter_tasks(view.tasks, &["awaiting_human"]),
            "awaiting"
        )
    );

    let recent_activity = render_recent_runs_panel(view.recent_runs, view.tasks, 8);
    let system = system_status_panel(
        view.bootstrap,
        view.runtime_root.display().to_string(),
        view.state_store,
        view.orchestrator_status,
        view.runner_status,
        view.qa_status,
    );
    let pipeline = TASK_PIPELINE_STAGES
        .iter()
        .map(|stage| format!("<li>{}</li>", html_escape(stage)))
        .collect::<Vec<_>>()
        .join("");

    app_shell(
        "dashboard",
        "Dashboard",
        &format!(
            "{}<div class=\"dashboard-grid\">\
               <div class=\"dashboard-main\">{}{}\
               </div>\
               <aside class=\"dashboard-side\">{}{}<section class=\"panel\"><div class=\"panel-header\"><div><p class=\"eyebrow\">Pipeline</p><h2>Current Stages</h2></div></div><ol class=\"plain-list\">{}</ol></section>\
               </aside>\
             </div>",
            hero, intake, attention, recent_activity, system, pipeline
        ),
        "",
    )
}

pub fn render_board(view: BoardView<'_>) -> String {
    let board = render_lane_board(view.tasks, true);
    let body = format!(
        "<section class=\"page-header\">\
            <div><p class=\"eyebrow\">Workflow</p><h1>Board</h1></div>\
            <p class=\"section-copy\">Drag cards to reorder within each lane. Workflow state changes remain explicit and button-driven so the orchestration stays deterministic.</p>\
         </section>\
         <section class=\"board-toolbar\">\
            <div class=\"toolbar-chip\">Blocked: {}</div>\
            <div class=\"toolbar-chip\">Awaiting Human: {}</div>\
            <div class=\"toolbar-chip\">Ready For QA: {}</div>\
            <div class=\"toolbar-chip\">Ready For PR: {}</div>\
         </section>\
         <section id=\"task-board\" class=\"board-page\">{}</section>",
        count_state(view.tasks, "blocked"),
        count_state(view.tasks, "awaiting_human"),
        count_state(view.tasks, "ready_for_qa"),
        count_state(view.tasks, "ready_for_pr"),
        board
    );

    app_shell("board", "Board", &body, &board_drag_script())
}

pub fn render_tasks_index(view: TaskListView<'_>) -> String {
    let spotlight = filter_tasks(
        view.tasks,
        &[
            "blocked",
            "awaiting_human",
            "fix_required",
            "ready_for_pr",
            "ready_for_qa",
        ],
    );
    let backlog = filter_tasks(
        view.tasks,
        &[
            "draft",
            "ready_for_planning",
            "planning",
            "ready_for_development",
            "developing",
            "ready_for_review",
            "reviewing",
            "pr_prepared",
        ],
    );

    let body = format!(
        "<section class=\"page-header\">\
            <div><p class=\"eyebrow\">Tasks</p><h1>Task Explorer</h1></div>\
            <p class=\"section-copy\">Browse tasks by urgency and jump straight into the detail page when you need the full story.</p>\
         </section>\
         <section class=\"panel list-panel\">\
           <div class=\"panel-header\"><div><h2>Needs Attention</h2></div><span class=\"count-pill\">{}</span></div>\
           <div class=\"task-grid\">{}</div>\
         </section>\
         <section class=\"panel list-panel\">\
           <div class=\"panel-header\"><div><h2>Everything Else</h2></div><span class=\"count-pill\">{}</span></div>\
           <div class=\"task-grid\">{}</div>\
         </section>",
        spotlight.len(),
        render_task_grid(&spotlight, true),
        backlog.len(),
        render_task_grid(&backlog, false)
    );

    app_shell("tasks", "Tasks", &body, "")
}

pub fn render_runs(view: RunsView<'_>) -> String {
    let titles = task_title_map(view.tasks);
    let timeline = if view.runs.is_empty() {
        "<div class=\"empty-state\">No stage runs yet.</div>".to_string()
    } else {
        view.runs
            .iter()
            .map(|run| {
                let title = titles
                    .get(&run.task_id)
                    .cloned()
                    .unwrap_or_else(|| run.task_id.clone());
                format!(
                    "<article class=\"run-card status-{}\">\
                       <div class=\"run-card-head\">\
                         <div>\
                           <p class=\"eyebrow\">{}</p>\
                           <h3><a href=\"/tasks/{}\">{}</a></h3>\
                         </div>\
                         <span class=\"status-badge status-{}\">{}</span>\
                       </div>\
                       <p><code>{}</code> • attempt {}</p>\
                       <p>Started {}{}</p>\
                       {}\
                     </article>",
                    css_state(&run.status),
                    html_escape(&run.stage),
                    run.task_id,
                    html_escape(&title),
                    css_state(&run.status),
                    html_escape(&run.status),
                    html_escape(&run.id),
                    run.attempt_number,
                    html_escape(&run.started_at),
                    run.finished_at
                        .as_deref()
                        .map(|finished| format!(" • finished {}", html_escape(finished)))
                        .unwrap_or_default(),
                    run.error_summary
                        .as_deref()
                        .map(|summary| format!(
                            "<p class=\"error-copy\">{}</p>",
                            html_escape(summary)
                        ))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let body = format!(
        "<section class=\"page-header\">\
            <div><p class=\"eyebrow\">Execution</p><h1>Runs</h1></div>\
            <p class=\"section-copy\">A global feed of runner activity across planning, development, review, QA, recovery, and PR preparation.</p>\
         </section>\
         <section class=\"run-grid\">{}</section>",
        timeline
    );

    app_shell("runs", "Runs", &body, "")
}

pub fn render_task_detail(view: TaskDetailView<'_>) -> String {
    let action_html = task_action_buttons(view.task);
    let actions = if action_html.is_empty() {
        "<span class=\"muted\">No immediate automated action is available from this state.</span>"
            .to_string()
    } else {
        action_html
    };
    let tab_nav = [
        ("overview", "Overview"),
        ("evidence", "QA Evidence"),
        ("artifacts", "Artifacts"),
        ("history", "History"),
    ]
    .iter()
    .map(|(id, label)| {
        format!(
            "<button class=\"tab-trigger{}\" type=\"button\" data-tab-target=\"{}\">{}</button>",
            if *id == "overview" { " active" } else { "" },
            id,
            label
        )
    })
    .collect::<Vec<_>>()
    .join("");

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
    let transitions = render_transitions(view.transitions);
    let stage_runs = render_stage_runs(view.stage_runs);
    let artifacts = render_artifact_links(view.task.id.as_str(), view.artifacts);
    let qa_report = view.qa_report.map(render_preformatted).unwrap_or_else(|| {
        "<p class=\"empty-inline\">No QA report has been generated yet.</p>".to_string()
    });
    let qa_log = view.qa_log.map(render_preformatted).unwrap_or_else(|| {
        "<p class=\"empty-inline\">No QA log has been recorded yet.</p>".to_string()
    });
    let review_report = view
        .review_report
        .map(render_preformatted)
        .unwrap_or_else(|| {
            "<p class=\"empty-inline\">No review report has been generated yet.</p>".to_string()
        });
    let pr_summary = view.pr_summary.map(render_preformatted).unwrap_or_else(|| {
        "<p class=\"empty-inline\">No PR summary has been prepared yet.</p>".to_string()
    });
    let qa_screenshot = view
        .artifacts
        .iter()
        .find(|artifact| artifact.role == "qa_screenshot")
        .map(|_| {
            format!(
                "<img src=\"/tasks/{}/artifacts/qa_screenshot\" alt=\"QA screenshot for {}\" class=\"evidence-image\">",
                view.task.id,
                html_escape(&view.task.id)
            )
        })
        .unwrap_or_else(|| "<p class=\"empty-inline\">No QA screenshot has been captured yet.</p>".to_string());

    let body = format!(
        "<section class=\"page-header detail-header\">\
            <div>\
              <p class=\"eyebrow\">Task Detail</p>\
              <h1>{}</h1>\
              <p class=\"task-meta\"><code>{}</code> <span class=\"status-badge status-{}\">{}</span>{}</p>\
              <p class=\"section-copy\">{}</p>\
            </div>\
            <aside class=\"detail-actions\">\
              <div class=\"meta-stack\">\
                <span class=\"meta-label\">Workspace</span><code>{}</code>\
              </div>\
              {}\
              <div class=\"action-stack\">{}</div>\
            </aside>\
         </section>\
         <section class=\"tab-bar\">{}</section>\
         <section class=\"tab-panel active\" data-tab-panel=\"overview\">\
           <div class=\"detail-grid\">\
             <div class=\"panel\">\
               <div class=\"panel-header\"><div><h2>Current Handoff</h2></div></div>\
               <ul class=\"plain-list\">{}</ul>\
             </div>\
             <div class=\"panel\">\
               <div class=\"panel-header\"><div><h2>Review</h2></div></div>\
               {}\
             </div>\
             <div class=\"panel\">\
               <div class=\"panel-header\"><div><h2>PR Summary</h2></div></div>\
               {}\
             </div>\
           </div>\
         </section>\
         <section class=\"tab-panel\" data-tab-panel=\"evidence\">\
           <div class=\"detail-grid\">\
             <div class=\"panel\">\
               <div class=\"panel-header\"><div><h2>QA Report</h2></div></div>\
               {}\
             </div>\
             <div class=\"panel\">\
               <div class=\"panel-header\"><div><h2>QA Screenshot</h2></div></div>\
               <p><a href=\"/tasks/{}/artifacts/qa_screenshot\">Open raw screenshot</a> • <a href=\"/tasks/{}/artifacts/qa_har\">Open HAR</a> • <a href=\"/tasks/{}/artifacts/qa_log\">Open QA log</a></p>\
               {}\
             </div>\
             <div class=\"panel\">\
               <div class=\"panel-header\"><div><h2>QA Log</h2></div></div>\
               {}\
             </div>\
           </div>\
         </section>\
         <section class=\"tab-panel\" data-tab-panel=\"artifacts\">\
           <div class=\"panel\">\
             <div class=\"panel-header\"><div><h2>Artifacts</h2></div></div>\
             <ul class=\"artifact-list\">{}</ul>\
           </div>\
         </section>\
         <section class=\"tab-panel\" data-tab-panel=\"history\">\
           <div class=\"detail-grid\">\
             <div class=\"panel\">\
               <div class=\"panel-header\"><div><h2>State History</h2></div></div>\
               <ul class=\"plain-list\">{}</ul>\
             </div>\
             <div class=\"panel\">\
               <div class=\"panel-header\"><div><h2>Stage Runs</h2></div></div>\
               <ul class=\"plain-list\">{}</ul>\
             </div>\
           </div>\
         </section>",
        html_escape(&view.task.title),
        view.task.id,
        css_state(&view.task.state),
        human_label(&view.task.state),
        view.task
            .current_stage
            .as_deref()
            .map(|stage| format!(
                " <span class=\"stage-pill\">stage={}</span>",
                html_escape(stage)
            ))
            .unwrap_or_default(),
        html_escape(&view.task.goal),
        html_escape(&view.task.workspace_path),
        render_blocked_reason(view.task),
        actions,
        tab_nav,
        human_actions,
        review_report,
        pr_summary,
        qa_report,
        view.task.id,
        view.task.id,
        view.task.id,
        qa_screenshot,
        qa_log,
        artifacts,
        transitions,
        stage_runs
    );

    app_shell("tasks", &view.task.title, &body, &task_tabs_script())
}

fn app_shell(active: &str, title: &str, body: &str, page_script: &str) -> String {
    format!(
        "<!doctype html>\
         <html lang=\"en\">\
         <head>\
           <meta charset=\"utf-8\">\
           <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
           <title>{}</title>\
           <style>{}</style>\
         </head>\
         <body>\
           <div class=\"app-shell\">\
             <aside class=\"sidebar\">\
               <a class=\"brand\" href=\"/\">Patron</a>\
               <p class=\"brand-copy\">Local-first delivery autopilot for one repository, one board, one understandable workflow.</p>\
               <nav class=\"nav\">\
                 {}\
               </nav>\
             </aside>\
             <div class=\"workspace\">\
               <header class=\"topbar\">\
                 <div>\
                   <p class=\"eyebrow\">Patron</p>\
                   <h1>{}</h1>\
                 </div>\
                 <div class=\"topbar-actions\">\
                   <a class=\"button ghost\" href=\"/sample-app\">Sample App</a>\
                   <a class=\"button ghost\" href=\"/board\">Board</a>\
                   <a class=\"button ghost\" href=\"/runs\">Runs</a>\
                 </div>\
               </header>\
               <main class=\"page\">{}\
               </main>\
             </div>\
           </div>\
           <script>{}</script>\
         </body>\
         </html>",
        html_escape(title),
        base_styles(),
        nav_links(active),
        html_escape(title),
        body,
        page_script
    )
}

fn base_styles() -> &'static str {
    r#"
    :root {
      --bg: #f4ecdf;
      --paper: #fff8ef;
      --panel: #fffdf8;
      --ink: #1f1a14;
      --muted: #6e6255;
      --line: #d8cbb9;
      --line-strong: #a99478;
      --accent: #b14d2d;
      --accent-soft: #f4d9cd;
      --gold: #d8b24d;
      --green: #4e7f57;
      --green-soft: #dbead9;
      --red: #9a3f2b;
      --red-soft: #f8d8d1;
      --shadow: 0 16px 42px rgba(88, 61, 24, 0.08);
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: "Avenir Next", "Segoe UI", sans-serif;
      background:
        radial-gradient(circle at top left, rgba(216,178,77,0.18), transparent 28%),
        linear-gradient(180deg, #f9f2e8 0%, var(--bg) 55%);
      color: var(--ink);
    }
    a { color: inherit; }
    .app-shell {
      min-height: 100vh;
      display: grid;
      grid-template-columns: 280px minmax(0, 1fr);
    }
    .sidebar {
      border-right: 1px solid var(--line);
      background: linear-gradient(180deg, rgba(255,250,242,0.96), rgba(250,240,225,0.92));
      padding: 28px 22px;
      position: sticky;
      top: 0;
      height: 100vh;
    }
    .brand {
      display: inline-block;
      font-size: 1.7rem;
      font-weight: 800;
      text-decoration: none;
      letter-spacing: -0.04em;
      margin-bottom: 8px;
    }
    .brand-copy {
      color: var(--muted);
      line-height: 1.5;
      margin: 0 0 24px 0;
    }
    .nav {
      display: grid;
      gap: 8px;
    }
    .nav-link {
      display: flex;
      align-items: center;
      justify-content: space-between;
      text-decoration: none;
      background: transparent;
      border: 1px solid transparent;
      border-radius: 14px;
      padding: 12px 14px;
      color: var(--muted);
      font-weight: 600;
    }
    .nav-link:hover,
    .nav-link.active {
      background: var(--panel);
      border-color: var(--line);
      color: var(--ink);
      box-shadow: var(--shadow);
    }
    .workspace {
      min-width: 0;
      padding: 20px 24px 40px;
    }
    .topbar {
      display: flex;
      align-items: end;
      justify-content: space-between;
      gap: 24px;
      margin-bottom: 24px;
    }
    .topbar h1,
    .page-header h1,
    .hero h1 {
      margin: 0;
      font-size: clamp(1.8rem, 2.5vw, 3rem);
      line-height: 1.02;
      letter-spacing: -0.04em;
    }
    .eyebrow {
      margin: 0 0 8px 0;
      font-size: 0.78rem;
      text-transform: uppercase;
      letter-spacing: 0.18em;
      color: var(--accent);
      font-weight: 700;
    }
    .page { display: grid; gap: 20px; }
    .page-header,
    .hero,
    .panel,
    .run-card {
      background: rgba(255, 251, 245, 0.92);
      border: 1px solid var(--line);
      border-radius: 22px;
      box-shadow: var(--shadow);
    }
    .page-header {
      padding: 24px;
      display: flex;
      align-items: end;
      justify-content: space-between;
      gap: 24px;
    }
    .hero {
      padding: 28px;
      display: grid;
      grid-template-columns: minmax(0, 1.6fr) minmax(280px, 0.9fr);
      gap: 22px;
      overflow: hidden;
      position: relative;
    }
    .hero::after {
      content: "";
      position: absolute;
      inset: auto -60px -100px auto;
      width: 280px;
      height: 280px;
      border-radius: 999px;
      background: radial-gradient(circle, rgba(177,77,45,0.24), transparent 65%);
      pointer-events: none;
    }
    .hero-panel {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 12px;
    }
    .metric {
      background: linear-gradient(180deg, rgba(255,255,255,0.82), rgba(248,238,223,0.96));
      border: 1px solid var(--line);
      border-radius: 18px;
      padding: 16px;
      min-height: 120px;
      display: flex;
      flex-direction: column;
      justify-content: end;
    }
    .metric.alert { border-color: rgba(154,63,43,0.32); background: linear-gradient(180deg, rgba(255,248,244,0.92), rgba(248,216,209,0.92)); }
    .metric-value {
      font-size: 2rem;
      font-weight: 800;
      letter-spacing: -0.05em;
    }
    .metric-label {
      color: var(--muted);
      margin-top: 8px;
    }
    .hero-actions,
    .topbar-actions,
    .form-footer,
    .board-toolbar {
      display: flex;
      flex-wrap: wrap;
      gap: 10px;
      align-items: center;
    }
    .button,
    button {
      appearance: none;
      border: 0;
      border-radius: 999px;
      padding: 12px 16px;
      font-weight: 700;
      text-decoration: none;
      cursor: pointer;
      background: var(--ink);
      color: white;
    }
    .button.primary { background: var(--accent); }
    .button.secondary { background: white; color: var(--ink); border: 1px solid var(--line-strong); }
    .button.ghost { background: transparent; color: var(--ink); border: 1px solid var(--line); }
    .panel {
      padding: 20px;
      display: grid;
      gap: 16px;
    }
    .panel-header {
      display: flex;
      align-items: start;
      justify-content: space-between;
      gap: 18px;
    }
    .panel-header h2,
    .panel-header h3 {
      margin: 0;
      font-size: 1.2rem;
      letter-spacing: -0.03em;
    }
    .section-copy,
    .lede,
    .muted,
    .empty-inline,
    .empty-state,
    .brand-copy {
      color: var(--muted);
    }
    .dashboard-grid,
    .detail-grid {
      display: grid;
      grid-template-columns: minmax(0, 1.5fr) minmax(320px, 0.9fr);
      gap: 20px;
    }
    .dashboard-main,
    .dashboard-side,
    .attention-grid,
    .task-grid,
    .run-grid {
      display: grid;
      gap: 20px;
    }
    .attention-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .intake-form { display: grid; gap: 12px; }
    textarea {
      width: 100%;
      border: 1px solid var(--line);
      border-radius: 18px;
      background: white;
      padding: 16px;
      font: inherit;
      min-height: 180px;
      resize: vertical;
    }
    .form-footer { justify-content: space-between; color: var(--muted); }
    .board-page {
      overflow-x: auto;
      padding-bottom: 8px;
    }
    .lane-board {
      display: grid;
      grid-template-columns: repeat(8, minmax(260px, 1fr));
      gap: 16px;
      min-width: 1240px;
    }
    .lane {
      background: linear-gradient(180deg, rgba(255,251,245,0.96), rgba(250,240,227,0.96));
      border: 1px solid var(--line);
      border-radius: 20px;
      padding: 14px;
      display: grid;
      gap: 14px;
      min-height: 340px;
      box-shadow: var(--shadow);
    }
    .lane.alert { border-color: rgba(154,63,43,0.34); background: linear-gradient(180deg, rgba(255,246,244,0.98), rgba(253,232,227,0.98)); }
    .lane.waiting { border-color: rgba(216,178,77,0.5); background: linear-gradient(180deg, rgba(255,249,232,0.98), rgba(249,238,193,0.92)); }
    .lane-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
    }
    .lane-head h3 { margin: 0; font-size: 1rem; }
    .count-pill,
    .toolbar-chip,
    .status-badge,
    .stage-pill {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      border-radius: 999px;
      padding: 5px 10px;
      font-size: 0.82rem;
      font-weight: 700;
      border: 1px solid transparent;
    }
    .toolbar-chip { background: var(--paper); border-color: var(--line); }
    .status-badge,
    .stage-pill { background: #f0e6d6; border-color: var(--line); }
    .status-ready-for-pr,
    .status-ready-for-qa,
    .status-ready-for-review,
    .status-ready-for-development { background: var(--green-soft); color: #27552d; border-color: rgba(78,127,87,0.28); }
    .status-fix-required,
    .status-blocked { background: var(--red-soft); color: var(--red); border-color: rgba(154,63,43,0.3); }
    .status-awaiting-human,
    .status-pr-prepared { background: #f6e8b8; color: #6e570f; border-color: rgba(184,144,26,0.3); }
    .status-qa-running,
    .status-reviewing,
    .status-developing,
    .status-planning { background: #e9ddfb; color: #5a3a8d; border-color: rgba(112,77,176,0.22); }
    .lane-cards {
      display: grid;
      gap: 12px;
      min-height: 120px;
      padding: 4px;
      border-radius: 18px;
      transition: background 140ms ease, border-color 140ms ease, box-shadow 140ms ease;
    }
    .lane-cards.drop-active {
      background: rgba(188, 95, 52, 0.08);
      box-shadow: inset 0 0 0 1px rgba(188, 95, 52, 0.18);
    }
    .task-card {
      background: white;
      border: 1px solid #eadfcd;
      border-radius: 18px;
      padding: 14px;
      display: grid;
      gap: 10px;
      box-shadow: 0 10px 20px rgba(97, 75, 37, 0.06);
    }
    .task-card.compact { gap: 8px; }
    .task-card.draggable { cursor: grab; }
    .task-card.dragging { opacity: 0.4; transform: rotate(1deg); }
    .task-card.drag-source { box-shadow: 0 18px 28px rgba(123, 69, 36, 0.18); }
    .task-card h3,
    .task-card h4 { margin: 0; font-size: 1rem; letter-spacing: -0.02em; }
    .card-meta { display: flex; flex-wrap: wrap; gap: 8px; }
    .card-title-row {
      display: flex;
      align-items: start;
      justify-content: space-between;
      gap: 12px;
    }
    .drag-handle {
      border: 1px dashed #d9c5a7;
      border-radius: 999px;
      color: var(--muted);
      display: inline-flex;
      align-items: center;
      gap: 6px;
      flex: 0 0 auto;
      font-size: 0.75rem;
      font-weight: 700;
      letter-spacing: 0.08em;
      padding: 5px 9px;
      text-transform: uppercase;
      user-select: none;
    }
    .task-card p { margin: 0; line-height: 1.45; }
    .task-card small,
    .plain-list small { color: var(--muted); }
    .task-card form,
    .action-stack { display: flex; flex-wrap: wrap; gap: 8px; }
    .task-card form button,
    .action-stack button { padding: 9px 12px; font-size: 0.9rem; }
    .list-panel,
    .attention-panel { gap: 14px; }
    .task-grid { grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); }
    .detail-header {
      grid-template-columns: minmax(0, 1.4fr) minmax(280px, 0.7fr);
      align-items: start;
    }
    .detail-actions { display: grid; gap: 16px; }
    .meta-stack { display: grid; gap: 6px; }
    .meta-label { color: var(--muted); font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.14em; }
    .task-meta { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
    .tab-bar {
      display: flex;
      flex-wrap: wrap;
      gap: 10px;
      padding: 4px 0;
    }
    .tab-trigger {
      background: transparent;
      color: var(--muted);
      border: 1px solid var(--line);
    }
    .tab-trigger.active { background: var(--ink); color: white; border-color: var(--ink); }
    .tab-panel { display: none; }
    .tab-panel.active { display: block; }
    .plain-list,
    .artifact-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 12px; }
    .plain-list li,
    .artifact-list li { padding-bottom: 12px; border-bottom: 1px solid #efe3d0; }
    .plain-list li:last-child,
    .artifact-list li:last-child { border-bottom: 0; padding-bottom: 0; }
    .run-grid { grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); }
    .run-card { padding: 18px; display: grid; gap: 12px; }
    .run-card-head {
      display: flex;
      align-items: start;
      justify-content: space-between;
      gap: 12px;
    }
    .run-card h3 { margin: 0; font-size: 1rem; }
    .error-copy { color: var(--red); font-weight: 600; }
    .evidence-image {
      width: 100%;
      border: 1px solid var(--line);
      border-radius: 18px;
      background: white;
    }
    code,
    pre {
      font-family: "SFMono-Regular", "Menlo", monospace;
    }
    pre {
      margin: 0;
      background: #fff;
      border: 1px solid #eadfcd;
      border-radius: 18px;
      padding: 16px;
      white-space: pre-wrap;
      overflow-x: auto;
    }
    .empty-state {
      padding: 24px;
      border: 1px dashed var(--line-strong);
      border-radius: 18px;
      background: rgba(255,255,255,0.58);
    }
    @media (max-width: 1080px) {
      .app-shell { grid-template-columns: 1fr; }
      .sidebar {
        position: static;
        height: auto;
        border-right: 0;
        border-bottom: 1px solid var(--line);
      }
      .dashboard-grid,
      .detail-grid,
      .hero,
      .page-header,
      .detail-header {
        grid-template-columns: 1fr;
      }
      .workspace { padding: 18px; }
      .lane-board { min-width: 0; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); }
      .attention-grid { grid-template-columns: 1fr; }
    }
    "#
}

fn nav_links(active: &str) -> String {
    let items = [
        ("setup", "/setup", "Setup"),
        ("dashboard", "/", "Dashboard"),
        ("board", "/board", "Board"),
        ("tasks", "/tasks", "Tasks"),
        ("runs", "/runs", "Runs"),
    ];

    items
        .iter()
        .map(|(id, href, label)| {
            format!(
                "<a class=\"nav-link{}\" href=\"{}\"><span>{}</span><span aria-hidden=\"true\">→</span></a>",
                if *id == active { " active" } else { "" },
                href,
                label
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_lane_board(tasks: &[TaskRecord], reorderable: bool) -> String {
    lane_states()
        .iter()
        .map(|(state, label, class_name)| {
            let cards = tasks
                .iter()
                .filter(|task| task.state == *state)
                .map(|task| render_board_task_card(task, reorderable))
                .collect::<Vec<_>>()
                .join("");
            format!(
                "<section class=\"lane {}\" data-lane=\"{}\">\
                   <div class=\"lane-head\">\
                     <div><p class=\"eyebrow\">{}</p><h3>{}</h3></div>\
                     <span class=\"count-pill\">{}</span>\
                   </div>\
                   <div class=\"lane-cards\" data-sort-lane=\"{}\">{}\
                   </div>\
                 </section>",
                class_name,
                state,
                state.to_uppercase().replace('_', " "),
                label,
                tasks.iter().filter(|task| task.state == *state).count(),
                state,
                if cards.is_empty() {
                    "<div class=\"empty-state\">No tasks here.</div>".to_string()
                } else {
                    cards
                }
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn lane_states() -> [(&'static str, &'static str, &'static str); 8] {
    [
        ("draft", "Draft Intake", ""),
        ("ready_for_development", "Ready For Development", ""),
        ("ready_for_review", "Ready For Review", ""),
        ("ready_for_qa", "Ready For QA", ""),
        ("fix_required", "Fix Required", "alert"),
        ("ready_for_pr", "Ready For PR", ""),
        ("awaiting_human", "Awaiting Human", "waiting"),
        ("blocked", "Blocked", "alert"),
    ]
}

fn render_board_task_card(task: &TaskRecord, draggable: bool) -> String {
    let action_html = task_action_buttons(task);
    let stage = task
        .current_stage
        .as_deref()
        .map(|stage| {
            format!(
                "<span class=\"stage-pill\">stage={}</span>",
                html_escape(stage)
            )
        })
        .unwrap_or_default();
    let blocked_reason = task
        .blocked_reason_text
        .as_deref()
        .map(|reason| format!("<p><small>{}</small></p>", html_escape(reason)))
        .unwrap_or_default();
    format!(
        "<article id=\"task-{}\" class=\"task-card compact{}\" {}>\
           <div class=\"card-meta\">\
             <span class=\"status-badge status-{}\">{}</span>{}\
           </div>\
           <div class=\"card-title-row\">\
             <div>\
               <h3><a href=\"/tasks/{}\">{}</a></h3>\
               <p><code>{}</code></p>\
             </div>\
             {}\
           </div>\
           <p>{}</p>\
           {}\
           {}\
         </article>",
        task.id,
        if draggable { " draggable" } else { "" },
        if draggable { "draggable=\"true\"" } else { "" },
        css_state(&task.state),
        human_label(&task.state),
        stage,
        task.id,
        html_escape(&task.title),
        task.id,
        if draggable {
            "<span class=\"drag-handle\" aria-hidden=\"true\">Reorder</span>"
        } else {
            ""
        },
        html_escape(&task.goal),
        blocked_reason,
        action_html
    )
}

fn render_task_grid(tasks: &[&TaskRecord], emphasize: bool) -> String {
    if tasks.is_empty() {
        return "<div class=\"empty-state\">No tasks to show in this section.</div>".to_string();
    }

    tasks
        .iter()
        .map(|task| {
            let action_html = task_action_buttons(task);
            format!(
                "<article class=\"task-card{}\">\
                   <div class=\"card-meta\">\
                     <span class=\"status-badge status-{}\">{}</span>\
                     {}\
                   </div>\
                   <div><h3><a href=\"/tasks/{}\">{}</a></h3><p><code>{}</code></p></div>\
                   <p>{}</p>\
                   {}\
                   {}\
                 </article>",
                if emphasize { "" } else { " compact" },
                css_state(&task.state),
                human_label(&task.state),
                task.current_stage
                    .as_deref()
                    .map(|stage| format!(
                        "<span class=\"stage-pill\">stage={}</span>",
                        html_escape(stage)
                    ))
                    .unwrap_or_default(),
                task.id,
                html_escape(&task.title),
                task.id,
                html_escape(&task.goal),
                task.blocked_reason_text
                    .as_deref()
                    .map(|reason| format!("<p><small>{}</small></p>", html_escape(reason)))
                    .unwrap_or_default(),
                action_html
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn attention_panel(title: &str, copy: &str, tasks: &[&TaskRecord], _style: &str) -> String {
    format!(
        "<section class=\"panel attention-panel\">\
           <div class=\"panel-header\">\
             <div><p class=\"eyebrow\">Attention</p><h2>{}</h2></div>\
             <span class=\"count-pill\">{}</span>\
           </div>\
           <p class=\"section-copy\">{}</p>\
           <div class=\"task-grid\">{}</div>\
         </section>",
        title,
        tasks.len(),
        copy,
        render_task_grid(tasks, true)
    )
}

fn render_recent_runs_panel(runs: &[StageRunRecord], tasks: &[TaskRecord], limit: usize) -> String {
    let titles = task_title_map(tasks);
    let items = runs
        .iter()
        .take(limit)
        .map(|run| {
            format!(
                "<li>\
                   <strong><a href=\"/tasks/{}\">{}</a></strong> • {} • <span class=\"status-badge status-{}\">{}</span><br>\
                   <small><code>{}</code> started {}{}</small>\
                 </li>",
                run.task_id,
                html_escape(
                    titles
                        .get(&run.task_id)
                        .map(String::as_str)
                        .unwrap_or(&run.task_id)
                ),
                html_escape(&run.stage),
                css_state(&run.status),
                html_escape(&run.status),
                html_escape(&run.id),
                html_escape(&run.started_at),
                run.finished_at
                    .as_deref()
                    .map(|finished| format!(" • finished {}", html_escape(finished)))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        "<section class=\"panel\">\
           <div class=\"panel-header\">\
             <div><p class=\"eyebrow\">Timeline</p><h2>Recent Runs</h2></div>\
             <a class=\"button ghost\" href=\"/runs\">Open All Runs</a>\
           </div>\
           <ul class=\"plain-list\">{}</ul>\
         </section>",
        if items.is_empty() {
            "<li>No recorded runs yet.</li>".to_string()
        } else {
            items
        }
    )
}

fn system_status_panel(
    bootstrap: &BootstrapStatus,
    runtime_root: String,
    state_store: &StateStoreStatus<'_>,
    orchestrator_status: &str,
    runner_status: &str,
    qa_status: &str,
) -> String {
    format!(
        "<section class=\"panel\">\
           <div class=\"panel-header\"><div><p class=\"eyebrow\">System</p><h2>Runtime</h2></div></div>\
           <ul class=\"plain-list\">\
              <li><strong>Root</strong><br><small><code>{}</code></small></li>\
              <li><strong>Repository</strong><br><small>{} • branch <code>{}</code></small></li>\
              <li><strong>State Store</strong><br><small>{} at <code>{}</code> • schema v{} • {} migration bytes</small></li>\
              <li><strong>Orchestrator</strong><br><small>{}</small></li>\
              <li><strong>Runner</strong><br><small>{}</small></li>\
              <li><strong>QA</strong><br><small>{}</small></li>\
           </ul>\
         </section>",
        html_escape(&runtime_root),
        html_escape(&bootstrap.repo.repo_name),
        html_escape(bootstrap.repo.git_branch.as_deref().unwrap_or("unknown")),
        html_escape(state_store.engine),
        html_escape(&state_store.location),
        state_store.schema_version,
        state_store.initial_schema_bytes,
        html_escape(orchestrator_status),
        html_escape(runner_status),
        html_escape(qa_status),
    )
}

fn render_transitions(transitions: &[StateTransitionRecord]) -> String {
    if transitions.is_empty() {
        return "<li>No state transitions recorded yet.</li>".to_string();
    }

    transitions
        .iter()
        .map(|transition| {
            format!(
                "<li><code>{}</code>: {} → <strong>{}</strong> via {}{}<br><small>{}</small>{}</li>",
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
}

fn render_stage_runs(runs: &[StageRunRecord]) -> String {
    if runs.is_empty() {
        return "<li>No stage runs recorded yet.</li>".to_string();
    }

    runs.iter()
        .map(|run| {
            format!(
                "<li><strong>{}</strong> attempt {} [{}] <span class=\"status-badge status-{}\">{}</span><br><small>started {}{}</small>{}</li>",
                html_escape(&run.id),
                run.attempt_number,
                html_escape(&run.stage),
                css_state(&run.status),
                html_escape(&run.status),
                html_escape(&run.started_at),
                run.finished_at
                    .as_deref()
                    .map(|finished| format!(" • finished {}", html_escape(finished)))
                    .unwrap_or_default(),
                run.error_summary
                    .as_deref()
                    .map(|summary| format!("<br><small>{}</small>", html_escape(summary)))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

fn render_artifact_links(task_id: &str, artifacts: &[WorkingArtifactRecord]) -> String {
    if artifacts.is_empty() {
        return "<li>No artifacts recorded yet.</li>".to_string();
    }

    artifacts
        .iter()
        .map(|artifact| {
            let label = if artifact.media_type == "inode/directory" {
                html_escape(&artifact.role)
            } else {
                format!(
                    "<a href=\"/tasks/{}/artifacts/{}\">{}</a>",
                    task_id,
                    html_escape(&artifact.role),
                    html_escape(&artifact.role)
                )
            };
            format!(
                "<li>{} <code>{}</code>{}<br><small>{}</small></li>",
                label,
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
}

fn board_drag_script() -> String {
    r#"
      (() => {
        const storageKey = "patron-board-order-v1";
        const lanes = [...document.querySelectorAll("[data-sort-lane]")];
        const boardState = JSON.parse(localStorage.getItem(storageKey) || "{}");
        let dragged = null;
        let sourceLane = null;
        let activeLane = null;

        const persist = () => {
          const snapshot = {};
          lanes.forEach((lane) => {
            snapshot[lane.dataset.sortLane] = [...lane.querySelectorAll(".task-card")]
              .map((card) => card.id);
          });
          localStorage.setItem(storageKey, JSON.stringify(snapshot));
        };

        const applySavedOrder = () => {
          lanes.forEach((lane) => {
            const saved = boardState[lane.dataset.sortLane];
            if (!Array.isArray(saved)) return;
            saved.forEach((cardId) => {
              const card = lane.querySelector(`#${CSS.escape(cardId)}`);
              if (card) lane.appendChild(card);
            });
          });
        };

        const setActiveLane = (lane) => {
          if (activeLane && activeLane !== lane) {
            activeLane.classList.remove("drop-active");
          }
          activeLane = lane;
          if (activeLane) {
            activeLane.classList.add("drop-active");
          }
        };

        const clearActiveLane = () => {
          if (activeLane) {
            activeLane.classList.remove("drop-active");
          }
          activeLane = null;
        };

        const closestCard = (lane, y) => {
          const cards = [...lane.querySelectorAll(".task-card:not(.dragging)")];
          return cards.reduce((closest, card) => {
            const box = card.getBoundingClientRect();
            const offset = y - box.top - box.height / 2;
            if (offset < 0 && offset > closest.offset) {
              return { offset, element: card };
            }
            return closest;
          }, { offset: Number.NEGATIVE_INFINITY, element: null }).element;
        };

        applySavedOrder();

        lanes.forEach((lane) => {
          lane.querySelectorAll(".task-card.draggable").forEach((card) => {
            card.addEventListener("dragstart", (event) => {
              dragged = card;
              sourceLane = lane.dataset.sortLane;
              if (event.dataTransfer) {
                event.dataTransfer.effectAllowed = "move";
                event.dataTransfer.setData("text/plain", card.id);
              }
              card.classList.add("dragging");
              card.classList.add("drag-source");
            });
            card.addEventListener("dragend", () => {
              if (dragged) {
                dragged.classList.remove("dragging");
                dragged.classList.remove("drag-source");
                persist();
              }
              clearActiveLane();
              dragged = null;
              sourceLane = null;
            });
          });

          lane.addEventListener("dragenter", (event) => {
            if (!dragged || sourceLane !== lane.dataset.sortLane) return;
            event.preventDefault();
            setActiveLane(lane);
          });

          lane.addEventListener("dragover", (event) => {
            if (!dragged || sourceLane !== lane.dataset.sortLane) return;
            event.preventDefault();
            setActiveLane(lane);
            const afterElement = closestCard(lane, event.clientY);
            if (!afterElement) {
              lane.appendChild(dragged);
            } else {
              lane.insertBefore(dragged, afterElement);
            }
          });

          lane.addEventListener("dragleave", (event) => {
            if (!activeLane || event.currentTarget !== activeLane) return;
            const next = event.relatedTarget;
            if (next && lane.contains(next)) return;
            clearActiveLane();
          });

          lane.addEventListener("drop", (event) => {
            if (!dragged || sourceLane !== lane.dataset.sortLane) return;
            event.preventDefault();
            const afterElement = closestCard(lane, event.clientY);
            if (!afterElement) {
              lane.appendChild(dragged);
            } else {
              lane.insertBefore(dragged, afterElement);
            }
            persist();
            clearActiveLane();
          });
        });
      })();
    "#
    .to_string()
}

fn task_tabs_script() -> String {
    r#"
      (() => {
        const triggers = [...document.querySelectorAll("[data-tab-target]")];
        const panels = [...document.querySelectorAll("[data-tab-panel]")];
        const activate = (target) => {
          triggers.forEach((trigger) => {
            trigger.classList.toggle("active", trigger.dataset.tabTarget === target);
          });
          panels.forEach((panel) => {
            panel.classList.toggle("active", panel.dataset.tabPanel === target);
          });
        };
        triggers.forEach((trigger) => {
          trigger.addEventListener("click", () => activate(trigger.dataset.tabTarget));
        });
      })();
    "#
    .to_string()
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

fn human_label(state: &str) -> String {
    state
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn css_state(state: &str) -> String {
    state.replace('_', "-")
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn count_state(tasks: &[TaskRecord], state: &str) -> usize {
    tasks.iter().filter(|task| task.state == state).count()
}

fn filter_tasks<'a>(tasks: &'a [TaskRecord], states: &[&str]) -> Vec<&'a TaskRecord> {
    tasks
        .iter()
        .filter(|task| states.iter().any(|state| *state == task.state))
        .collect()
}

fn task_title_map(tasks: &[TaskRecord]) -> HashMap<String, String> {
    tasks
        .iter()
        .map(|task| (task.id.clone(), task.title.clone()))
        .collect()
}

fn render_preformatted(value: &str) -> String {
    format!("<pre>{}</pre>", html_escape(value))
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
