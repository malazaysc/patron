use std::fs;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::{
    app::RuntimePaths,
    db::{self, TaskRecord, WorkingArtifactUpsert},
    domain::task_lifecycle::{ActorKind, TaskState, TaskStateMachine, TransitionMetadata},
    runner::{self, RunnerCompletion, RunnerJob, RunnerOutcome},
};

pub fn status_label() -> &'static str {
    "playwright-backed qa runner available"
}

pub fn run_qa(runtime: &RuntimePaths, task_id: &str) -> Result<(), String> {
    let task =
        db::get_task(runtime, task_id)?.ok_or_else(|| format!("task {task_id} was not found"))?;
    let current_state = parse_task_state(&task.state)?;
    if current_state != TaskState::ReadyForQa {
        return Err(format!(
            "qa can only run for ready_for_qa tasks, found {}",
            task.state
        ));
    }

    let job = RunnerJob {
        task_id: task.id.clone(),
        stage: "qa".into(),
        summary: "Execute qa-steps.md scenarios and capture evidence".into(),
        repo_root: runtime.repo_root().display().to_string(),
        repo_name: runtime
            .repo_root()
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown-repo")
            .to_string(),
        git_branch: db::load_repo_metadata(runtime)
            .ok()
            .flatten()
            .and_then(|metadata| metadata.git_branch),
    };

    runner::execute_job(runtime, job, |run, log_path| {
        let qa_metadata = transition_metadata(
            ActorKind::Runner,
            "qa started",
            Some("qa_started"),
            Some(run.id.as_str()),
        );
        TaskStateMachine::validate_transition(
            TaskState::ReadyForQa,
            TaskState::QaRunning,
            &qa_metadata,
        )
        .map_err(|error| format!("invalid ready_for_qa->qa_running transition: {error:?}"))?;
        db::transition_task_state(
            runtime,
            &task.id,
            TaskState::ReadyForQa,
            TaskState::QaRunning,
            &qa_metadata,
        )?;

        let workspace_path = runtime.tasks_dir.join(&task.id);
        let task_md = workspace_path.join("task.md");
        let plan_md = workspace_path.join("plan.md");
        let qa_steps_md = workspace_path.join("qa-steps.md");
        let review_md = workspace_path.join("review.md");
        let development_summary_md = workspace_path.join("development-summary.md");
        validate_required_artifacts(&[
            &task_md,
            &plan_md,
            &qa_steps_md,
            &review_md,
            &development_summary_md,
        ])?;

        let task_input = fs::read_to_string(&task_md)
            .map_err(|error| format!("failed to read {}: {error}", task_md.display()))?;
        let plan_input = fs::read_to_string(&plan_md)
            .map_err(|error| format!("failed to read {}: {error}", plan_md.display()))?;
        let qa_steps_input = fs::read_to_string(&qa_steps_md)
            .map_err(|error| format!("failed to read {}: {error}", qa_steps_md.display()))?;
        let review_input = fs::read_to_string(&review_md)
            .map_err(|error| format!("failed to read {}: {error}", review_md.display()))?;

        let scenarios = parse_qa_steps(&qa_steps_input)?;
        let evidence = capture_browser_evidence(runtime, &task, &run.id)?;
        let scenario_results = execute_scenarios(
            &task,
            &task_input,
            &plan_input,
            &review_input,
            &scenarios,
            &workspace_path,
            &evidence,
        );
        let report = build_qa_report(&task, &scenario_results, &evidence);
        let qa_report_path = workspace_path.join("qa-report.md");
        fs::write(&qa_report_path, report).map_err(|error| {
            format!(
                "failed to write qa-report.md for {} at {}: {error}",
                task.id,
                qa_report_path.display()
            )
        })?;
        validate_required_artifacts(&[
            &qa_report_path,
            &evidence.log_path,
            &evidence.screenshot_path,
            &evidence.har_path,
        ])?;

        upsert_qa_artifacts(
            runtime,
            &task.id,
            run.id.as_str(),
            &qa_report_path,
            &evidence,
        )?;
        append_runner_log(
            log_path,
            &[
                format!("consumed {}", task_md.display()),
                format!("consumed {}", plan_md.display()),
                format!("consumed {}", qa_steps_md.display()),
                format!("consumed {}", review_md.display()),
                format!("generated {}", qa_report_path.display()),
                format!("generated {}", evidence.log_path.display()),
                format!("generated {}", evidence.screenshot_path.display()),
                format!("generated {}", evidence.har_path.display()),
                format!(
                    "qa outcome {}",
                    overall_status(&scenario_results, &evidence)
                ),
            ],
        )?;

        let has_findings =
            scenario_results.iter().any(|result| !result.passed) || !evidence.capture_succeeded;
        let target_state = if has_findings {
            TaskState::FixRequired
        } else {
            TaskState::ReadyForPr
        };
        let finished_metadata = transition_metadata(
            ActorKind::Runner,
            if has_findings {
                "qa recorded findings and routed to fix_required"
            } else {
                "qa passed and task is ready for pr preparation"
            },
            Some(if has_findings {
                "qa_failed"
            } else {
                "qa_passed"
            }),
            Some(run.id.as_str()),
        );
        TaskStateMachine::validate_transition(
            TaskState::QaRunning,
            target_state,
            &finished_metadata,
        )
        .map_err(|error| format!("invalid qa_running transition: {error:?}"))?;
        db::transition_task_state(
            runtime,
            &task.id,
            TaskState::QaRunning,
            target_state,
            &finished_metadata,
        )?;

        Ok(RunnerOutcome {
            completion: RunnerCompletion::Completed,
            exit_code: if has_findings { 1 } else { 0 },
            error_summary: has_findings.then(|| "qa recorded findings".to_string()),
        })
    })?;

    Ok(())
}

#[derive(Clone, Debug)]
struct QaScenario {
    heading: String,
    steps: Vec<String>,
    expected_results: Vec<String>,
}

#[derive(Clone, Debug)]
struct QaScenarioResult {
    heading: String,
    passed: bool,
    notes: Vec<String>,
}

#[derive(Clone, Debug)]
struct QaEvidence {
    log_path: PathBuf,
    screenshot_path: PathBuf,
    har_path: PathBuf,
    target_name: String,
    target_url: String,
    startup_log_path: Option<PathBuf>,
    capture_succeeded: bool,
    capture_notes: Vec<String>,
}

struct QaAppRuntime {
    target_name: String,
    target_url: String,
    selector: String,
    startup_log_path: Option<PathBuf>,
    child: Option<Child>,
}

fn execute_scenarios(
    task: &TaskRecord,
    task_md: &str,
    plan_md: &str,
    review_md: &str,
    scenarios: &[QaScenario],
    workspace_path: &Path,
    evidence: &QaEvidence,
) -> Vec<QaScenarioResult> {
    scenarios
        .iter()
        .map(|scenario| {
            execute_scenario(
                task,
                task_md,
                plan_md,
                review_md,
                scenario,
                workspace_path,
                evidence,
            )
        })
        .collect()
}

fn execute_scenario(
    task: &TaskRecord,
    task_md: &str,
    plan_md: &str,
    review_md: &str,
    scenario: &QaScenario,
    workspace_path: &Path,
    evidence: &QaEvidence,
) -> QaScenarioResult {
    let heading = scenario.heading.clone();
    let normalized_heading = heading.to_ascii_lowercase();

    if normalized_heading.contains("artifacts are available") {
        let files = ["task.md", "plan.md", "qa-steps.md"];
        let mut missing = Vec::new();
        for file_name in files {
            let artifact_path = workspace_path.join(file_name);
            if !artifact_path.exists() {
                missing.push(format!("missing {}", artifact_path.display()));
            }
        }

        return QaScenarioResult {
            heading,
            passed: missing.is_empty(),
            notes: if missing.is_empty() {
                vec!["all planning artifacts are present".into()]
            } else {
                missing
            },
        };
    }

    if normalized_heading.contains("reflects the requested goal") {
        let mut notes = Vec::new();
        let mut passed = true;

        if !task_md.contains(&task.goal) {
            passed = false;
            notes.push("task.md does not include the original goal".into());
        }
        if !plan_md.contains("## Steps") {
            passed = false;
            notes.push("plan.md is missing the expected steps section".into());
        }

        if passed {
            notes.push("task.md and plan.md preserve the requested goal and concrete steps".into());
        }

        return QaScenarioResult {
            heading,
            passed,
            notes,
        };
    }

    if normalized_heading.contains("ready for development")
        || normalized_heading.contains("ready for qa")
        || normalized_heading.contains("review package exists")
    {
        let mut notes = Vec::new();
        let mut passed = true;

        let development_summary_path = workspace_path.join("development-summary.md");
        let review_path = workspace_path.join("review.md");
        if !development_summary_path.exists() {
            passed = false;
            notes.push("development-summary.md is missing".into());
        }
        if !review_path.exists() {
            passed = false;
            notes.push("review.md is missing".into());
        }
        if !review_md.contains("- Status: pass") {
            passed = false;
            notes.push("review.md does not indicate a passing review outcome".into());
        }

        if passed {
            notes.push("review artifacts are present and the task is eligible for QA".into());
        }

        return QaScenarioResult {
            heading,
            passed,
            notes,
        };
    }

    if normalized_heading.contains("evidence") {
        let mut notes = Vec::new();
        let mut passed = evidence.capture_succeeded;

        if evidence.capture_succeeded {
            notes.push(format!(
                "browser evidence captured at {} and {}",
                evidence.screenshot_path.display(),
                evidence.har_path.display()
            ));
        } else {
            passed = false;
            notes.extend(evidence.capture_notes.clone());
        }

        return QaScenarioResult {
            heading,
            passed,
            notes,
        };
    }

    if normalized_heading.contains("creating ")
        || normalized_heading.contains("adding ")
        || normalized_heading.contains("verifying ")
    {
        let mut notes = Vec::new();
        let passed = evidence.capture_succeeded;
        notes.push(format!(
            "qa targeted `{}` at {}",
            evidence.target_name, evidence.target_url
        ));
        if !evidence.capture_notes.is_empty() {
            notes.extend(evidence.capture_notes.clone());
        }
        return QaScenarioResult {
            heading,
            passed,
            notes,
        };
    }

    if normalized_heading.contains("sample app route loads successfully") {
        let passed = evidence.capture_succeeded;
        return QaScenarioResult {
            heading,
            passed,
            notes: if passed {
                vec!["sample app rendered and browser evidence capture completed".into()]
            } else {
                vec!["sample app did not finish loading well enough for evidence capture".into()]
            },
        };
    }

    let mut notes = Vec::new();
    if !scenario.steps.is_empty() {
        notes.push(format!(
            "unsupported scenario steps: {}",
            scenario.steps.join(" | ")
        ));
    }
    if !scenario.expected_results.is_empty() {
        notes.push(format!(
            "expected results recorded: {}",
            scenario.expected_results.join(" | ")
        ));
    }
    if notes.is_empty() {
        notes.push("scenario type is not yet supported by the deterministic QA runner".into());
    }

    QaScenarioResult {
        heading,
        passed: false,
        notes,
    }
}

fn capture_browser_evidence(
    runtime: &RuntimePaths,
    task: &TaskRecord,
    run_id: &str,
) -> Result<QaEvidence, String> {
    let log_path = runtime.qa_logs_dir.join(format!("{run_id}.log"));
    let screenshot_path = runtime
        .qa_screenshots_dir
        .join(format!("{run_id}-board.png"));
    let har_path = runtime.qa_traces_dir.join(format!("{run_id}.har"));

    let mut app_runtime = start_qa_target(runtime, task, run_id)?;
    let target_name = app_runtime.target_name.clone();
    let target_url = app_runtime.target_url.clone();
    let selector = app_runtime.selector.clone();
    let output = Command::new("npx")
        .arg("playwright")
        .arg("screenshot")
        .arg("--browser")
        .arg("chromium")
        .arg("--wait-for-selector")
        .arg(&selector)
        .arg("--wait-for-timeout")
        .arg("750")
        .arg("--save-har")
        .arg(&har_path)
        .arg(&target_url)
        .arg(&screenshot_path)
        .output()
        .map_err(|error| format!("failed to invoke Playwright screenshot command: {error}"))?;

    let mut capture_notes = Vec::new();
    let mut log_contents = String::new();
    if !output.stdout.is_empty() {
        log_contents.push_str(&String::from_utf8_lossy(&output.stdout));
        if !log_contents.ends_with('\n') {
            log_contents.push('\n');
        }
    }
    if !output.stderr.is_empty() {
        log_contents.push_str(&String::from_utf8_lossy(&output.stderr));
        if !log_contents.ends_with('\n') {
            log_contents.push('\n');
        }
    }

    let screenshot_exists = screenshot_path.exists();
    let har_exists = har_path.exists();
    let capture_succeeded = output.status.success() && screenshot_exists && har_exists;

    if capture_succeeded {
        capture_notes.push(format!(
            "Playwright captured QA target `{}` successfully",
            target_name
        ));
    } else {
        if !output.status.success() {
            capture_notes.push(format!(
                "Playwright screenshot command exited with status {}",
                output
                    .status
                    .code()
                    .map_or_else(|| "signal".to_string(), |code| code.to_string())
            ));
        }
        if !screenshot_exists {
            capture_notes.push(format!(
                "expected screenshot was not created at {}",
                screenshot_path.display()
            ));
        }
        if !har_exists {
            capture_notes.push(format!(
                "expected HAR was not created at {}",
                har_path.display()
            ));
        }
    }

    if log_contents.trim().is_empty() {
        log_contents.push_str("Playwright command produced no stdout/stderr output.\n");
    }
    if !capture_notes.is_empty() {
        log_contents.push('\n');
        for note in &capture_notes {
            log_contents.push_str("- ");
            log_contents.push_str(note);
            log_contents.push('\n');
        }
    }

    fs::write(&log_path, log_contents).map_err(|error| {
        format!(
            "failed to write qa evidence log {}: {error}",
            log_path.display()
        )
    })?;

    stop_qa_target(&mut app_runtime);

    Ok(QaEvidence {
        log_path,
        screenshot_path,
        har_path,
        target_name,
        target_url,
        startup_log_path: app_runtime.startup_log_path,
        capture_succeeded,
        capture_notes,
    })
}

fn is_sample_app_task(task: &TaskRecord) -> bool {
    let haystack = format!(
        "{}\n{}",
        task.title.to_ascii_lowercase(),
        task.goal.to_ascii_lowercase()
    );
    haystack.contains("sample app") || haystack.contains("sample-app")
}

fn start_qa_target(
    runtime: &RuntimePaths,
    task: &TaskRecord,
    run_id: &str,
) -> Result<QaAppRuntime, String> {
    if is_sample_app_task(task) {
        return Ok(QaAppRuntime {
            target_name: "sample-app".into(),
            target_url: "http://127.0.0.1:3000/sample-app".into(),
            selector: "#sample-app.ready".into(),
            startup_log_path: None,
            child: None,
        });
    }

    let repo_root = runtime.repo_root();
    let manage_py = repo_root.join("manage.py");
    let venv_python = repo_root.join(".venv/bin/python");
    if manage_py.exists() {
        let python = if venv_python.exists() {
            venv_python
        } else {
            PathBuf::from("python3")
        };
        let startup_log_path = runtime.qa_logs_dir.join(format!("{run_id}-startup.log"));
        let log_file = fs::File::create(&startup_log_path).map_err(|error| {
            format!(
                "failed to create QA startup log {}: {error}",
                startup_log_path.display()
            )
        })?;
        let log_file_err = log_file
            .try_clone()
            .map_err(|error| format!("failed to clone QA startup log file: {error}"))?;
        let mut command = Command::new(&python);
        command
            .current_dir(repo_root)
            .arg("manage.py")
            .arg("runserver")
            .arg("127.0.0.1:8001")
            .arg("--noreload")
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_file_err));
        let child = command.spawn().map_err(|error| {
            format!(
                "failed to start Django QA target with {:?}: {error}",
                python
            )
        })?;
        wait_for_port("127.0.0.1", 8001, Duration::from_secs(20)).map_err(|error| {
            format!("django QA target did not become ready at http://127.0.0.1:8001/: {error}")
        })?;

        return Ok(QaAppRuntime {
            target_name: "django-app".into(),
            target_url: "http://127.0.0.1:8001/".into(),
            selector: "body".into(),
            startup_log_path: Some(startup_log_path),
            child: Some(child),
        });
    }

    Ok(QaAppRuntime {
        target_name: "patron-board".into(),
        target_url: "http://127.0.0.1:3000/board".into(),
        selector: "#task-board".into(),
        startup_log_path: None,
        child: None,
    })
}

fn stop_qa_target(runtime: &mut QaAppRuntime) {
    if let Some(child) = runtime.child.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn wait_for_port(host: &str, port: u16, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if TcpStream::connect((host, port)).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(format!("timed out waiting for {host}:{port}"))
}

fn build_qa_report(
    task: &TaskRecord,
    scenario_results: &[QaScenarioResult],
    evidence: &QaEvidence,
) -> String {
    let overall_status = overall_status(scenario_results, evidence);
    let scenarios = if scenario_results.is_empty() {
        "- No QA scenarios were found in qa-steps.md.".to_string()
    } else {
        scenario_results
            .iter()
            .map(|result| {
                let notes = if result.notes.is_empty() {
                    "- no notes recorded".to_string()
                } else {
                    result
                        .notes
                        .iter()
                        .map(|note| format!("- {note}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                format!(
                    "### {}\n- Status: {}\n{}\n",
                    result.heading,
                    if result.passed { "pass" } else { "fail" },
                    notes
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let evidence_notes = if evidence.capture_notes.is_empty() {
        "- No evidence notes recorded.".to_string()
    } else {
        evidence
            .capture_notes
            .iter()
            .map(|note| format!("- {note}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "# QA Report\n\n## Task\n- ID: `{}`\n- Title: {}\n\n## Outcome\n- Status: {}\n- Next state: `{}`\n\n## QA Target\n- Target name: `{}`\n- Target URL: `{}`\n{}\n## Scenario Results\n{}\n\n## Evidence\n- QA log: `{}`\n- Screenshot: `{}`\n- HAR: `{}`\n{}\n",
        task.id,
        task.title,
        overall_status,
        if overall_status == "pass" {
            "ready_for_pr"
        } else {
            "fix_required"
        },
        evidence.target_name,
        evidence.target_url,
        evidence
            .startup_log_path
            .as_ref()
            .map(|path| format!("- Startup log: `{}`\n\n", path.display()))
            .unwrap_or_default(),
        scenarios,
        evidence.log_path.display(),
        evidence.screenshot_path.display(),
        evidence.har_path.display(),
        evidence_notes
    )
}

fn upsert_qa_artifacts(
    runtime: &RuntimePaths,
    task_id: &str,
    run_id: &str,
    qa_report_path: &Path,
    evidence: &QaEvidence,
) -> Result<(), String> {
    let report_relative = format!(".patron/tasks/{task_id}/qa-report.md");
    db::upsert_working_artifact(
        runtime,
        WorkingArtifactUpsert {
            task_id,
            role: "qa_report_md",
            artifact_kind: "qa_report",
            relative_path: &report_relative,
            media_type: "text/markdown",
            required_for_stage: true,
            stage_run_id: Some(run_id),
        },
    )?;
    db::upsert_working_artifact(
        runtime,
        WorkingArtifactUpsert {
            task_id,
            role: "qa_log",
            artifact_kind: "qa_log",
            relative_path: &relative_qa_path(&evidence.log_path),
            media_type: "text/plain",
            required_for_stage: true,
            stage_run_id: Some(run_id),
        },
    )?;
    db::upsert_working_artifact(
        runtime,
        WorkingArtifactUpsert {
            task_id,
            role: "qa_screenshot",
            artifact_kind: "qa_screenshot",
            relative_path: &relative_qa_path(&evidence.screenshot_path),
            media_type: "image/png",
            required_for_stage: true,
            stage_run_id: Some(run_id),
        },
    )?;
    db::upsert_working_artifact(
        runtime,
        WorkingArtifactUpsert {
            task_id,
            role: "qa_har",
            artifact_kind: "qa_har",
            relative_path: &relative_qa_path(&evidence.har_path),
            media_type: "application/json",
            required_for_stage: true,
            stage_run_id: Some(run_id),
        },
    )?;
    if let Some(startup_log_path) = &evidence.startup_log_path {
        db::upsert_working_artifact(
            runtime,
            WorkingArtifactUpsert {
                task_id,
                role: "qa_startup_log",
                artifact_kind: "qa_startup_log",
                relative_path: &relative_qa_path(startup_log_path),
                media_type: "text/plain",
                required_for_stage: false,
                stage_run_id: Some(run_id),
            },
        )?;
    }

    validate_required_artifacts(&[qa_report_path])?;
    Ok(())
}

fn parse_qa_steps(input: &str) -> Result<Vec<QaScenario>, String> {
    let mut scenarios = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_steps = Vec::new();
    let mut current_expected = Vec::new();

    for line in input.lines() {
        if let Some(heading) = line.strip_prefix("## Scenario ") {
            if let Some(previous_heading) = current_heading.take() {
                scenarios.push(QaScenario {
                    heading: previous_heading,
                    steps: std::mem::take(&mut current_steps),
                    expected_results: std::mem::take(&mut current_expected),
                });
            }
            current_heading = Some(heading.trim().to_string());
            continue;
        }

        if line.starts_with("## Evidence Requirements") {
            break;
        }

        let trimmed = line.trim_start();
        if let Some(step) = trimmed.strip_prefix("- Expected result:") {
            current_expected.push(step.trim().to_string());
        } else if let Some(step) = trimmed.strip_prefix("- ") {
            current_steps.push(step.trim().to_string());
        }
    }

    if let Some(previous_heading) = current_heading.take() {
        scenarios.push(QaScenario {
            heading: previous_heading,
            steps: current_steps,
            expected_results: current_expected,
        });
    }

    if scenarios.is_empty() {
        return Err("qa-steps.md did not contain any scenario headings".into());
    }

    Ok(scenarios)
}

fn relative_qa_path(path: &Path) -> String {
    path.strip_prefix(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .unwrap_or(path)
        .display()
        .to_string()
}

fn overall_status(results: &[QaScenarioResult], evidence: &QaEvidence) -> &'static str {
    if results.iter().all(|result| result.passed) && evidence.capture_succeeded {
        "pass"
    } else {
        "fix_required"
    }
}

fn validate_required_artifacts(paths: &[&Path]) -> Result<(), String> {
    for path in paths {
        if !path.exists() {
            return Err(format!("required qa artifact missing: {}", path.display()));
        }
    }
    Ok(())
}

fn append_runner_log(log_path: &Path, lines: &[String]) -> Result<(), String> {
    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(log_path)
        .map_err(|error| {
            format!(
                "failed to reopen runner log {}: {error}",
                log_path.display()
            )
        })?;
    for line in lines {
        writeln!(file, "{line}").map_err(|error| {
            format!(
                "failed to append runner log {}: {error}",
                log_path.display()
            )
        })?;
    }
    Ok(())
}

fn transition_metadata(
    actor: ActorKind,
    reason_text: &str,
    reason_code: Option<&str>,
    run_id: Option<&str>,
) -> TransitionMetadata {
    TransitionMetadata {
        actor,
        actor_id: None,
        occurred_at: format!(
            "unix:{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_secs())
        ),
        reason_code: reason_code.map(ToOwned::to_owned),
        reason_text: reason_text.to_string(),
        run_id: run_id.map(ToOwned::to_owned),
        required_human_action: None,
    }
}

fn parse_task_state(state: &str) -> Result<TaskState, String> {
    match state {
        "draft" => Ok(TaskState::Draft),
        "ready_for_planning" => Ok(TaskState::ReadyForPlanning),
        "planning" => Ok(TaskState::Planning),
        "ready_for_development" => Ok(TaskState::ReadyForDevelopment),
        "developing" => Ok(TaskState::Developing),
        "ready_for_review" => Ok(TaskState::ReadyForReview),
        "reviewing" => Ok(TaskState::Reviewing),
        "ready_for_qa" => Ok(TaskState::ReadyForQa),
        "qa_running" => Ok(TaskState::QaRunning),
        "fix_required" => Ok(TaskState::FixRequired),
        "ready_for_pr" => Ok(TaskState::ReadyForPr),
        "pr_prepared" => Ok(TaskState::PrPrepared),
        "awaiting_human" => Ok(TaskState::AwaitingHuman),
        "done" => Ok(TaskState::Done),
        "blocked" => Ok(TaskState::Blocked),
        "failed" => Ok(TaskState::Failed),
        "cancelled" => Ok(TaskState::Cancelled),
        other => Err(format!("unknown task state: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{QaEvidence, build_qa_report, overall_status, parse_qa_steps};
    use crate::db::TaskRecord;

    #[test]
    fn parses_scenarios_from_qa_steps() {
        let scenarios = parse_qa_steps(
            "# QA Steps\n\n## Scenario 1: Task artifacts are available\n- Open the task workspace.\n- Expected result: artifacts exist.\n\n## Evidence Requirements\n- Capture evidence.\n",
        )
        .expect("qa steps should parse");

        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].heading, "1: Task artifacts are available");
        assert_eq!(scenarios[0].steps.len(), 1);
        assert_eq!(scenarios[0].expected_results.len(), 1);
    }

    #[test]
    fn overall_status_requires_evidence_and_passing_scenarios() {
        let results = vec![super::QaScenarioResult {
            heading: "Scenario".into(),
            passed: true,
            notes: vec![],
        }];
        let passing_evidence = QaEvidence {
            log_path: Path::new(".patron/qa/logs/demo.log").into(),
            screenshot_path: Path::new(".patron/qa/screenshots/demo.png").into(),
            har_path: Path::new(".patron/qa/traces/demo.har").into(),
            target_name: "patron-board".into(),
            target_url: "http://127.0.0.1:3000/board".into(),
            startup_log_path: None,
            capture_succeeded: true,
            capture_notes: vec![],
        };
        let failing_evidence = QaEvidence {
            capture_succeeded: false,
            ..passing_evidence.clone()
        };

        assert_eq!(overall_status(&results, &passing_evidence), "pass");
        assert_eq!(overall_status(&results, &failing_evidence), "fix_required");
    }

    #[test]
    fn qa_report_includes_evidence_paths() {
        let report = build_qa_report(
            &TaskRecord {
                id: "TASK-0010".into(),
                title: "QA report".into(),
                goal: "Verify qa report formatting".into(),
                state: "ready_for_qa".into(),
                blocked_reason_code: None,
                blocked_reason_text: None,
                current_stage: Some("qa".into()),
                workspace_path: ".patron/tasks/TASK-0010".into(),
                handoff_path: ".patron/tasks/TASK-0010/orchestrator-handoff.md".into(),
            },
            &[super::QaScenarioResult {
                heading: "1: Task artifacts are available".into(),
                passed: true,
                notes: vec!["all planning artifacts are present".into()],
            }],
            &QaEvidence {
                log_path: Path::new(".patron/qa/logs/demo.log").into(),
                screenshot_path: Path::new(".patron/qa/screenshots/demo.png").into(),
                har_path: Path::new(".patron/qa/traces/demo.har").into(),
                target_name: "patron-board".into(),
                target_url: "http://127.0.0.1:3000/board".into(),
                startup_log_path: None,
                capture_succeeded: true,
                capture_notes: vec!["Playwright captured the Patron home page successfully".into()],
            },
        );

        assert!(report.contains("# QA Report"));
        assert!(report.contains("ready_for_pr"));
        assert!(report.contains(".patron/qa/screenshots/demo.png"));
    }
}
