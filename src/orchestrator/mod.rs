use std::fs;
use std::path::Path;

use crate::{
    app::RuntimePaths,
    db::{self, HumanActionCreate, TaskRecord, WorkingArtifactUpsert},
    domain::task_lifecycle::{
        ActorKind, HumanAction, TaskState, TaskStateMachine, TransitionMetadata,
    },
    runner::{self, RunnerCompletion, RunnerJob, RunnerOutcome},
};

pub fn status_label() -> &'static str {
    "draft intake, planning, development, review, fix-loop scaffolding, and PR preparation available; qa delegated to the qa subsystem"
}

pub fn create_draft_task(runtime: &RuntimePaths, goal: &str) -> Result<TaskRecord, String> {
    let trimmed_goal = goal.trim();
    if trimmed_goal.is_empty() {
        return Err("goal input cannot be empty".into());
    }

    let task_id = db::next_task_id(runtime)?;
    let workspace_relative = format!(".patron/tasks/{task_id}");
    let workspace_path = runtime.tasks_dir.join(&task_id);
    fs::create_dir_all(&workspace_path).map_err(|error| {
        format!(
            "failed to create task workspace {}: {error}",
            workspace_path.display()
        )
    })?;

    let handoff_path = workspace_path.join("orchestrator-handoff.md");
    let title = derive_title(trimmed_goal);
    let handoff = format!(
        "# Orchestrator Handoff\n\nTask ID: `{}`\nState: `{}`\n\n## Goal\n{}\n\n## Next Step\nConvert this draft task into a structured planning package.\n",
        task_id,
        TaskState::Draft.as_str(),
        trimmed_goal
    );
    fs::write(&handoff_path, handoff).map_err(|error| {
        format!(
            "failed to write orchestrator handoff file {}: {error}",
            handoff_path.display()
        )
    })?;

    let task = TaskRecord {
        id: task_id.clone(),
        title,
        goal: trimmed_goal.to_string(),
        state: TaskState::Draft.as_str().to_string(),
        blocked_reason_code: None,
        blocked_reason_text: None,
        current_stage: None,
        workspace_path: workspace_relative.clone(),
        handoff_path: format!("{workspace_relative}/orchestrator-handoff.md"),
    };

    db::insert_task(runtime, &task)?;
    db::register_workspace_artifact(runtime, &task_id, &workspace_relative)?;

    Ok(task)
}

pub fn run_planning(runtime: &RuntimePaths, task_id: &str) -> Result<(), String> {
    let task =
        db::get_task(runtime, task_id)?.ok_or_else(|| format!("task {task_id} was not found"))?;
    let current_state = parse_task_state(&task.state)?;
    if !matches!(
        current_state,
        TaskState::Draft | TaskState::ReadyForPlanning
    ) {
        return Err(format!(
            "planning can only run for draft or ready_for_planning tasks, found {}",
            task.state
        ));
    }

    if current_state == TaskState::Draft {
        let ready_metadata = transition_metadata(
            ActorKind::Orchestrator,
            "task approved for planning",
            Some("task_intake_complete"),
            None,
        );
        TaskStateMachine::validate_transition(
            TaskState::Draft,
            TaskState::ReadyForPlanning,
            &ready_metadata,
        )
        .map_err(|error| format!("invalid draft->ready_for_planning transition: {error:?}"))?;
        db::transition_task_state(
            runtime,
            &task.id,
            TaskState::Draft,
            TaskState::ReadyForPlanning,
            &ready_metadata,
        )?;
    }

    let job = RunnerJob {
        task_id: task.id.clone(),
        stage: "planning".into(),
        summary: "Generate task.md, plan.md, and qa-steps.md".into(),
    };

    runner::execute_job(runtime, job, |run, log_path| {
        let planning_metadata = transition_metadata(
            ActorKind::Runner,
            "planning started",
            Some("planning_started"),
            Some(run.id.as_str()),
        );
        TaskStateMachine::validate_transition(
            TaskState::ReadyForPlanning,
            TaskState::Planning,
            &planning_metadata,
        )
        .map_err(|error| format!("invalid ready_for_planning->planning transition: {error:?}"))?;
        db::transition_task_state(
            runtime,
            &task.id,
            TaskState::ReadyForPlanning,
            TaskState::Planning,
            &planning_metadata,
        )?;

        let workspace_path = runtime.tasks_dir.join(&task.id);
        let planning_package = build_planning_package(&task);
        let task_md = workspace_path.join("task.md");
        let plan_md = workspace_path.join("plan.md");
        let qa_steps_md = workspace_path.join("qa-steps.md");

        fs::write(&task_md, planning_package.task_md).map_err(|error| {
            format!(
                "failed to write task.md for {} at {}: {error}",
                task.id,
                task_md.display()
            )
        })?;
        fs::write(&plan_md, planning_package.plan_md).map_err(|error| {
            format!(
                "failed to write plan.md for {} at {}: {error}",
                task.id,
                plan_md.display()
            )
        })?;
        fs::write(&qa_steps_md, planning_package.qa_steps_md).map_err(|error| {
            format!(
                "failed to write qa-steps.md for {} at {}: {error}",
                task.id,
                qa_steps_md.display()
            )
        })?;

        validate_required_artifacts(&[&task_md, &plan_md, &qa_steps_md])?;

        let task_root = format!(".patron/tasks/{}", task.id);
        db::upsert_working_artifact(
            runtime,
            WorkingArtifactUpsert {
                task_id: &task.id,
                role: "task_md",
                artifact_kind: "task_document",
                relative_path: &format!("{task_root}/task.md"),
                media_type: "text/markdown",
                required_for_stage: true,
                stage_run_id: Some(run.id.as_str()),
            },
        )?;
        db::upsert_working_artifact(
            runtime,
            WorkingArtifactUpsert {
                task_id: &task.id,
                role: "plan_md",
                artifact_kind: "plan_document",
                relative_path: &format!("{task_root}/plan.md"),
                media_type: "text/markdown",
                required_for_stage: true,
                stage_run_id: Some(run.id.as_str()),
            },
        )?;
        db::upsert_working_artifact(
            runtime,
            WorkingArtifactUpsert {
                task_id: &task.id,
                role: "qa_steps_md",
                artifact_kind: "qa_steps_document",
                relative_path: &format!("{task_root}/qa-steps.md"),
                media_type: "text/markdown",
                required_for_stage: true,
                stage_run_id: Some(run.id.as_str()),
            },
        )?;

        append_planning_log(
            log_path,
            &[
                format!("generated {}", task_md.display()),
                format!("generated {}", plan_md.display()),
                format!("generated {}", qa_steps_md.display()),
            ],
        )?;

        let finished_metadata = transition_metadata(
            ActorKind::Runner,
            "planning completed and artifacts generated",
            Some("planning_completed"),
            Some(run.id.as_str()),
        );
        TaskStateMachine::validate_transition(
            TaskState::Planning,
            TaskState::ReadyForDevelopment,
            &finished_metadata,
        )
        .map_err(|error| {
            format!("invalid planning->ready_for_development transition: {error:?}")
        })?;
        db::transition_task_state(
            runtime,
            &task.id,
            TaskState::Planning,
            TaskState::ReadyForDevelopment,
            &finished_metadata,
        )?;

        Ok(RunnerOutcome {
            completion: RunnerCompletion::Completed,
            exit_code: 0,
            error_summary: None,
        })
    })?;

    Ok(())
}

pub fn run_development(runtime: &RuntimePaths, task_id: &str) -> Result<(), String> {
    let task =
        db::get_task(runtime, task_id)?.ok_or_else(|| format!("task {task_id} was not found"))?;
    let current_state = parse_task_state(&task.state)?;
    if current_state != TaskState::ReadyForDevelopment {
        return Err(format!(
            "development can only run for ready_for_development tasks, found {}",
            task.state
        ));
    }

    let job = RunnerJob {
        task_id: task.id.clone(),
        stage: "development".into(),
        summary: "Consume planning artifacts and generate a reviewable development summary".into(),
    };

    runner::execute_job(runtime, job, |run, log_path| {
        let development_metadata = transition_metadata(
            ActorKind::Runner,
            "development started",
            Some("development_started"),
            Some(run.id.as_str()),
        );
        TaskStateMachine::validate_transition(
            TaskState::ReadyForDevelopment,
            TaskState::Developing,
            &development_metadata,
        )
        .map_err(|error| {
            format!("invalid ready_for_development->developing transition: {error:?}")
        })?;
        db::transition_task_state(
            runtime,
            &task.id,
            TaskState::ReadyForDevelopment,
            TaskState::Developing,
            &development_metadata,
        )?;

        let workspace_path = runtime.tasks_dir.join(&task.id);
        let task_md = workspace_path.join("task.md");
        let plan_md = workspace_path.join("plan.md");
        let qa_steps_md = workspace_path.join("qa-steps.md");
        validate_required_artifacts(&[&task_md, &plan_md, &qa_steps_md])?;

        let task_input = fs::read_to_string(&task_md)
            .map_err(|error| format!("failed to read {}: {error}", task_md.display()))?;
        let plan_input = fs::read_to_string(&plan_md)
            .map_err(|error| format!("failed to read {}: {error}", plan_md.display()))?;
        let qa_input = fs::read_to_string(&qa_steps_md)
            .map_err(|error| format!("failed to read {}: {error}", qa_steps_md.display()))?;

        let development_summary =
            build_development_summary(&task, &task_input, &plan_input, &qa_input);
        let summary_path = workspace_path.join("development-summary.md");
        fs::write(&summary_path, development_summary).map_err(|error| {
            format!(
                "failed to write development-summary.md for {} at {}: {error}",
                task.id,
                summary_path.display()
            )
        })?;
        validate_required_artifacts(&[&summary_path])?;

        db::upsert_working_artifact(
            runtime,
            WorkingArtifactUpsert {
                task_id: &task.id,
                role: "development_summary_md",
                artifact_kind: "development_summary",
                relative_path: &format!(".patron/tasks/{}/development-summary.md", task.id),
                media_type: "text/markdown",
                required_for_stage: true,
                stage_run_id: Some(run.id.as_str()),
            },
        )?;

        append_planning_log(
            log_path,
            &[
                format!("consumed {}", task_md.display()),
                format!("consumed {}", plan_md.display()),
                format!("consumed {}", qa_steps_md.display()),
                format!("generated {}", summary_path.display()),
            ],
        )?;

        let finished_metadata = transition_metadata(
            ActorKind::Runner,
            "development summary generated and task ready for review",
            Some("development_completed"),
            Some(run.id.as_str()),
        );
        TaskStateMachine::validate_transition(
            TaskState::Developing,
            TaskState::ReadyForReview,
            &finished_metadata,
        )
        .map_err(|error| format!("invalid developing->ready_for_review transition: {error:?}"))?;
        db::transition_task_state(
            runtime,
            &task.id,
            TaskState::Developing,
            TaskState::ReadyForReview,
            &finished_metadata,
        )?;

        Ok(RunnerOutcome {
            completion: RunnerCompletion::Completed,
            exit_code: 0,
            error_summary: None,
        })
    })?;

    Ok(())
}

pub fn run_review(runtime: &RuntimePaths, task_id: &str) -> Result<(), String> {
    let task =
        db::get_task(runtime, task_id)?.ok_or_else(|| format!("task {task_id} was not found"))?;
    let current_state = parse_task_state(&task.state)?;
    if current_state != TaskState::ReadyForReview {
        return Err(format!(
            "review can only run for ready_for_review tasks, found {}",
            task.state
        ));
    }

    let job = RunnerJob {
        task_id: task.id.clone(),
        stage: "review".into(),
        summary: "Assess development output and produce review.md".into(),
    };

    runner::execute_job(runtime, job, |run, log_path| {
        let review_metadata = transition_metadata(
            ActorKind::Runner,
            "review started",
            Some("review_started"),
            Some(run.id.as_str()),
        );
        TaskStateMachine::validate_transition(
            TaskState::ReadyForReview,
            TaskState::Reviewing,
            &review_metadata,
        )
        .map_err(|error| format!("invalid ready_for_review->reviewing transition: {error:?}"))?;
        db::transition_task_state(
            runtime,
            &task.id,
            TaskState::ReadyForReview,
            TaskState::Reviewing,
            &review_metadata,
        )?;

        let workspace_path = runtime.tasks_dir.join(&task.id);
        let task_md = workspace_path.join("task.md");
        let plan_md = workspace_path.join("plan.md");
        let development_summary_md = workspace_path.join("development-summary.md");
        validate_required_artifacts(&[&task_md, &plan_md, &development_summary_md])?;

        let task_input = fs::read_to_string(&task_md)
            .map_err(|error| format!("failed to read {}: {error}", task_md.display()))?;
        let plan_input = fs::read_to_string(&plan_md)
            .map_err(|error| format!("failed to read {}: {error}", plan_md.display()))?;
        let development_input = fs::read_to_string(&development_summary_md).map_err(|error| {
            format!(
                "failed to read {}: {error}",
                development_summary_md.display()
            )
        })?;

        let review_result =
            review_development_outputs(&task_input, &plan_input, &development_input);
        let review_md = build_review_document(&task, &review_result);
        let review_path = workspace_path.join("review.md");
        fs::write(&review_path, review_md).map_err(|error| {
            format!(
                "failed to write review.md for {} at {}: {error}",
                task.id,
                review_path.display()
            )
        })?;
        validate_required_artifacts(&[&review_path])?;

        db::upsert_working_artifact(
            runtime,
            WorkingArtifactUpsert {
                task_id: &task.id,
                role: "review_md",
                artifact_kind: "review_document",
                relative_path: &format!(".patron/tasks/{}/review.md", task.id),
                media_type: "text/markdown",
                required_for_stage: true,
                stage_run_id: Some(run.id.as_str()),
            },
        )?;

        append_planning_log(
            log_path,
            &[
                format!("consumed {}", task_md.display()),
                format!("consumed {}", plan_md.display()),
                format!("consumed {}", development_summary_md.display()),
                format!("generated {}", review_path.display()),
                format!("review outcome {}", review_result.outcome_label()),
            ],
        )?;

        let target_state = if review_result.has_findings {
            TaskState::FixRequired
        } else {
            TaskState::ReadyForQa
        };
        let finished_metadata = transition_metadata(
            ActorKind::Runner,
            if review_result.has_findings {
                "review found actionable issues and routed to fix_required"
            } else {
                "review passed and task is ready for qa"
            },
            Some(if review_result.has_findings {
                "review_failed"
            } else {
                "review_passed"
            }),
            Some(run.id.as_str()),
        );
        TaskStateMachine::validate_transition(
            TaskState::Reviewing,
            target_state,
            &finished_metadata,
        )
        .map_err(|error| format!("invalid reviewing transition: {error:?}"))?;
        db::transition_task_state(
            runtime,
            &task.id,
            TaskState::Reviewing,
            target_state,
            &finished_metadata,
        )?;

        Ok(RunnerOutcome {
            completion: RunnerCompletion::Completed,
            exit_code: if review_result.has_findings { 1 } else { 0 },
            error_summary: review_result
                .has_findings
                .then(|| "review recorded findings".to_string()),
        })
    })?;

    Ok(())
}

pub fn run_fix_loop(runtime: &RuntimePaths, task_id: &str) -> Result<(), String> {
    let task =
        db::get_task(runtime, task_id)?.ok_or_else(|| format!("task {task_id} was not found"))?;
    let current_state = parse_task_state(&task.state)?;
    if current_state != TaskState::FixRequired {
        return Err(format!(
            "fix loop can only run for fix_required tasks, found {}",
            task.state
        ));
    }

    let workspace_path = runtime.tasks_dir.join(&task.id);
    let review_path = workspace_path.join("review.md");
    let qa_report_path = workspace_path.join("qa-report.md");
    let fix_log_path = workspace_path.join("fix-log.md");

    let failure_context = if review_path.exists() {
        FixLoopSource {
            source_stage: "review",
            source_path: review_path.clone(),
            contents: fs::read_to_string(&review_path)
                .map_err(|error| format!("failed to read {}: {error}", review_path.display()))?,
        }
    } else if qa_report_path.exists() {
        FixLoopSource {
            source_stage: "qa",
            source_path: qa_report_path.clone(),
            contents: fs::read_to_string(&qa_report_path)
                .map_err(|error| format!("failed to read {}: {error}", qa_report_path.display()))?,
        }
    } else {
        return Err(
            "fix loop requires either review.md or qa-report.md as failure context".to_string(),
        );
    };

    let entry = build_fix_log_entry(&task, &failure_context);
    append_fix_log(&fix_log_path, &entry)?;
    db::upsert_working_artifact(
        runtime,
        WorkingArtifactUpsert {
            task_id: &task.id,
            role: "fix_log_md",
            artifact_kind: "fix_log",
            relative_path: &format!(".patron/tasks/{}/fix-log.md", task.id),
            media_type: "text/markdown",
            required_for_stage: true,
            stage_run_id: None,
        },
    )?;

    let fix_metadata = transition_metadata(
        ActorKind::Runner,
        &format!(
            "fix loop prepared from {} findings and task returned to development",
            failure_context.source_stage
        ),
        Some("fix_loop_prepared"),
        None,
    );
    TaskStateMachine::validate_transition(
        TaskState::FixRequired,
        TaskState::ReadyForDevelopment,
        &fix_metadata,
    )
    .map_err(|error| {
        format!("invalid fix_required->ready_for_development transition: {error:?}")
    })?;
    db::transition_task_state(
        runtime,
        &task.id,
        TaskState::FixRequired,
        TaskState::ReadyForDevelopment,
        &fix_metadata,
    )?;

    Ok(())
}

pub fn run_pr_preparation(runtime: &RuntimePaths, task_id: &str) -> Result<(), String> {
    let task =
        db::get_task(runtime, task_id)?.ok_or_else(|| format!("task {task_id} was not found"))?;
    let current_state = parse_task_state(&task.state)?;
    if current_state != TaskState::ReadyForPr {
        return Err(format!(
            "pr preparation can only run for ready_for_pr tasks, found {}",
            task.state
        ));
    }

    let job = RunnerJob {
        task_id: task.id.clone(),
        stage: "pr_preparation".into(),
        summary: "Generate pr-summary.md and hand off to human review".into(),
    };

    runner::execute_job(runtime, job, |run, log_path| {
        let workspace_path = runtime.tasks_dir.join(&task.id);
        let task_md = workspace_path.join("task.md");
        let plan_md = workspace_path.join("plan.md");
        let review_md = workspace_path.join("review.md");
        let qa_report_md = workspace_path.join("qa-report.md");
        validate_required_artifacts(&[&task_md, &plan_md, &review_md, &qa_report_md])?;

        let ready_metadata = transition_metadata(
            ActorKind::Runner,
            "pr preparation started",
            Some("pr_preparation_started"),
            Some(run.id.as_str()),
        );
        TaskStateMachine::validate_transition(
            TaskState::ReadyForPr,
            TaskState::PrPrepared,
            &ready_metadata,
        )
        .map_err(|error| format!("invalid ready_for_pr->pr_prepared transition: {error:?}"))?;
        db::transition_task_state(
            runtime,
            &task.id,
            TaskState::ReadyForPr,
            TaskState::PrPrepared,
            &ready_metadata,
        )?;

        let review_input = fs::read_to_string(&review_md)
            .map_err(|error| format!("failed to read {}: {error}", review_md.display()))?;
        let qa_report_input = fs::read_to_string(&qa_report_md)
            .map_err(|error| format!("failed to read {}: {error}", qa_report_md.display()))?;
        let pr_summary = build_pr_summary(&task, &review_input, &qa_report_input);
        let pr_summary_path = workspace_path.join("pr-summary.md");
        fs::write(&pr_summary_path, pr_summary).map_err(|error| {
            format!(
                "failed to write pr-summary.md for {} at {}: {error}",
                task.id,
                pr_summary_path.display()
            )
        })?;
        validate_required_artifacts(&[&pr_summary_path])?;

        db::upsert_working_artifact(
            runtime,
            WorkingArtifactUpsert {
                task_id: &task.id,
                role: "pr_summary_md",
                artifact_kind: "pr_summary",
                relative_path: &format!(".patron/tasks/{}/pr-summary.md", task.id),
                media_type: "text/markdown",
                required_for_stage: true,
                stage_run_id: Some(run.id.as_str()),
            },
        )?;

        db::insert_human_action(
            runtime,
            HumanActionCreate {
                id: &format!("{}-review-pr", task.id),
                task_id: &task.id,
                action_type: HumanAction::ReviewPr.as_str(),
                requested_by: "patron",
                instructions: "Review the prepared PR summary, validate the linked QA evidence, and open or approve the final PR.",
            },
        )?;

        append_planning_log(
            log_path,
            &[
                format!("consumed {}", task_md.display()),
                format!("consumed {}", plan_md.display()),
                format!("consumed {}", review_md.display()),
                format!("consumed {}", qa_report_md.display()),
                format!("generated {}", pr_summary_path.display()),
                "requested human action review_pr".to_string(),
            ],
        )?;

        let awaiting_metadata = transition_metadata_with_action(
            ActorKind::Runner,
            "pr summary generated and task handed off for human review",
            Some("pr_prepared"),
            Some(run.id.as_str()),
            HumanAction::ReviewPr,
        );
        TaskStateMachine::validate_transition(
            TaskState::PrPrepared,
            TaskState::AwaitingHuman,
            &awaiting_metadata,
        )
        .map_err(|error| format!("invalid pr_prepared->awaiting_human transition: {error:?}"))?;
        db::transition_task_state(
            runtime,
            &task.id,
            TaskState::PrPrepared,
            TaskState::AwaitingHuman,
            &awaiting_metadata,
        )?;

        Ok(RunnerOutcome {
            completion: RunnerCompletion::Completed,
            exit_code: 0,
            error_summary: None,
        })
    })?;

    Ok(())
}

fn derive_title(goal: &str) -> String {
    let single_line = goal
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(goal);
    let trimmed = single_line.trim();
    if trimmed.chars().count() <= 72 {
        return trimmed.to_string();
    }

    let shortened = trimmed.chars().take(69).collect::<String>();
    format!("{shortened}...")
}

struct PlanningPackage {
    task_md: String,
    plan_md: String,
    qa_steps_md: String,
}

struct ReviewResult {
    has_findings: bool,
    findings: Vec<String>,
}

struct FixLoopSource {
    source_stage: &'static str,
    source_path: std::path::PathBuf,
    contents: String,
}

impl ReviewResult {
    fn outcome_label(&self) -> &'static str {
        if self.has_findings {
            "fix_required"
        } else {
            "pass"
        }
    }
}

fn build_development_summary(
    task: &TaskRecord,
    task_md: &str,
    plan_md: &str,
    qa_steps_md: &str,
) -> String {
    format!(
        "# Development Summary\n\n## Task\n- ID: `{}`\n- Title: {}\n\n## Inputs Consumed\n- `task.md`\n- `plan.md`\n- `qa-steps.md`\n\n## Goal Snapshot\n{}\n\n## Development Contract\n- Development must run from the planning artifacts, not free-form memory.\n- The next implementation step should produce concrete code changes against the repository.\n- The resulting work must be reviewable before QA begins.\n\n## Review Readiness\n- Planning inputs were present at execution time.\n- A development summary has been generated for reviewers.\n- The task can now move into the review stage.\n\n## Planning Signals\n### task.md excerpt\n{}\n\n### plan.md excerpt\n{}\n\n### qa-steps.md excerpt\n{}\n",
        task.id,
        task.title,
        task.goal,
        excerpt(task_md),
        excerpt(plan_md),
        excerpt(qa_steps_md)
    )
}

fn build_review_document(task: &TaskRecord, result: &ReviewResult) -> String {
    let findings = if result.findings.is_empty() {
        "- No findings.".to_string()
    } else {
        result
            .findings
            .iter()
            .map(|finding| format!("- {finding}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "# Review\n\n## Task\n- ID: `{}`\n- Title: {}\n\n## Outcome\n- Status: {}\n\n## Findings\n{}\n\n## Review Notes\n- Review runs after development and before QA.\n- Findings should route deterministically into `fix_required`.\n- Passing review should route the task to `ready_for_qa`.\n",
        task.id,
        task.title,
        result.outcome_label(),
        findings
    )
}

fn build_fix_log_entry(task: &TaskRecord, source: &FixLoopSource) -> String {
    format!(
        "## Fix Loop Entry\n\n- Task: `{}`\n- Source stage: `{}`\n- Source artifact: `{}`\n- Next state: `ready_for_development`\n\n### Failure Context Excerpt\n{}\n\n### Fix Guidance\n- Re-enter development using the latest planning artifacts.\n- Address the failure context before re-running review or QA.\n- Preserve the previous artifacts for traceability.\n\n",
        task.id,
        source.source_stage,
        source.source_path.display(),
        excerpt(&source.contents)
    )
}

fn build_pr_summary(task: &TaskRecord, review_md: &str, qa_report_md: &str) -> String {
    format!(
        "# PR Summary\n\n## Task\n- ID: `{}`\n- Title: {}\n\n## Goal\n{}\n\n## Review Outcome\n{}\n\n## QA Outcome\n{}\n\n## Ready For Human Review\n- Confirm the branch changes match the task goal.\n- Validate the linked QA evidence before creating or approving the PR.\n- Note any residual risk or follow-up work in the PR description.\n",
        task.id,
        task.title,
        task.goal,
        excerpt(review_md),
        excerpt(qa_report_md)
    )
}

fn review_development_outputs(
    task_md: &str,
    plan_md: &str,
    development_summary_md: &str,
) -> ReviewResult {
    let mut findings = Vec::new();

    if !development_summary_md.contains("Inputs Consumed") {
        findings.push("development summary is missing the required inputs section".to_string());
    }

    if !development_summary_md.contains("Review Readiness") {
        findings.push("development summary is missing the review readiness section".to_string());
    }

    if !task_md.contains("# Task") {
        findings.push("task.md is missing the expected task header".to_string());
    }

    if !plan_md.contains("# Plan") {
        findings.push("plan.md is missing the expected plan header".to_string());
    }

    ReviewResult {
        has_findings: !findings.is_empty(),
        findings,
    }
}

fn excerpt(value: &str) -> String {
    value.lines().take(8).collect::<Vec<_>>().join("\n")
}

fn build_planning_package(task: &TaskRecord) -> PlanningPackage {
    let task_md = format!(
        "# Task\n\n## ID\n`{}`\n\n## Title\n{}\n\n## Problem\n{}\n\n## Scope\n- Build the smallest implementation slice that satisfies the requested goal.\n- Keep the implementation local-first and repository-scoped.\n- Preserve deterministic task progression for later stages.\n\n## Constraints\n- macOS-only local runtime\n- single repository scope\n- runtime state must stay under `/.patron/`\n- avoid hidden automation or untracked side effects\n\n## Acceptance Criteria\n- A planning package exists for this task.\n- The task has `task.md`, `plan.md`, and `qa-steps.md` in its runtime workspace.\n- The task is ready for the development stage.\n\n## Dependencies\n- Rust application scaffold\n- SQLite runtime state\n- `.patron/` task workspace\n\n## Human Approvals\n- Task intake already completed\n- PR review still required later in the pipeline\n",
        task.id, task.title, task.goal
    );

    let plan_md = format!(
        "# Plan\n\n## Objective\nPrepare this task for implementation with a bounded development slice.\n\n## Steps\n1. Confirm the existing scaffold and current task context.\n2. Implement the smallest viable change set for the requested goal.\n3. Validate the behavior with code-level checks before QA.\n4. Hand the task to review with clear artifacts and state updates.\n\n## Development Notes\n- Use the task workspace under `/.patron/tasks/{}` as the source of runtime planning context.\n- Keep artifacts human-readable and stage-specific.\n- Avoid relying on chat history for downstream execution.\n",
        task.id
    );

    let qa_steps_md = format!(
        "# QA Steps\n\n## Scenario 1: Task artifacts are available\n- Open the task workspace for `{}`.\n- Confirm that `task.md`, `plan.md`, and `qa-steps.md` exist.\n- Expected result: all three planning artifacts are present and readable.\n\n## Scenario 2: Planning output reflects the requested goal\n- Read `task.md` and `plan.md`.\n- Confirm the original goal is captured and the plan describes concrete next steps.\n- Expected result: the planning package is aligned with the original goal and is understandable by a human.\n\n## Scenario 3: Review package exists for QA handoff\n- Confirm that `development-summary.md` and `review.md` exist in the task workspace.\n- Confirm that `review.md` recorded a passing review outcome before QA started.\n- Expected result: the task has review artifacts and is ready for QA execution.\n\n## Scenario 4: Browser evidence is captured during QA\n- Open the Patron UI in a browser-driven QA pass.\n- Capture a screenshot and HAR file while the task is visible on the board.\n- Expected result: QA leaves behind inspectable browser evidence for the active task.\n\n## Evidence Requirements\n- Capture the presence of the planning artifacts.\n- Preserve the QA browser screenshot, HAR file, and QA log.\n- Record the final QA outcome and any missing evidence in `qa-report.md`.\n",
        task.id
    );

    PlanningPackage {
        task_md,
        plan_md,
        qa_steps_md,
    }
}

fn validate_required_artifacts(paths: &[&Path]) -> Result<(), String> {
    for path in paths {
        if !path.exists() {
            return Err(format!(
                "required planning artifact missing: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn append_planning_log(log_path: &Path, lines: &[String]) -> Result<(), String> {
    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(log_path)
        .map_err(|error| {
            format!(
                "failed to reopen planning log {}: {error}",
                log_path.display()
            )
        })?;
    for line in lines {
        writeln!(file, "{line}").map_err(|error| {
            format!(
                "failed to append planning log {}: {error}",
                log_path.display()
            )
        })?;
    }
    Ok(())
}

fn append_fix_log(log_path: &Path, entry: &str) -> Result<(), String> {
    use std::io::Write;

    if !log_path.exists() {
        fs::write(log_path, "# Fix Log\n\n").map_err(|error| {
            format!(
                "failed to initialize fix log {}: {error}",
                log_path.display()
            )
        })?;
    }

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(log_path)
        .map_err(|error| format!("failed to reopen fix log {}: {error}", log_path.display()))?;
    file.write_all(entry.as_bytes())
        .map_err(|error| format!("failed to append fix log {}: {error}", log_path.display()))
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

fn transition_metadata_with_action(
    actor: ActorKind,
    reason_text: &str,
    reason_code: Option<&str>,
    run_id: Option<&str>,
    required_human_action: HumanAction,
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
        required_human_action: Some(required_human_action),
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
    use super::{
        FixLoopSource, TaskRecord, build_development_summary, build_fix_log_entry,
        build_planning_package, derive_title, review_development_outputs,
    };

    #[test]
    fn derive_title_uses_first_non_empty_line() {
        let title = derive_title("\n\nBuild the initial planner flow\nMore detail");
        assert_eq!(title, "Build the initial planner flow");
    }

    #[test]
    fn derive_title_truncates_long_input() {
        let title = derive_title(
            "This is a deliberately very long goal line that should be truncated to keep titles readable in the UI",
        );
        assert!(title.ends_with("..."));
        assert!(title.len() <= 72);
    }

    #[test]
    fn planning_package_contains_human_readable_qa_scenarios() {
        let package = build_planning_package(&TaskRecord {
            id: "TASK-0001".into(),
            title: "Build draft intake".into(),
            goal: "Build the draft task intake flow".into(),
            state: "draft".into(),
            blocked_reason_code: None,
            blocked_reason_text: None,
            current_stage: None,
            workspace_path: ".patron/tasks/TASK-0001".into(),
            handoff_path: ".patron/tasks/TASK-0001/orchestrator-handoff.md".into(),
        });

        assert!(package.task_md.contains("# Task"));
        assert!(package.plan_md.contains("# Plan"));
        assert!(package.qa_steps_md.contains("## Scenario 1"));
        assert!(package.qa_steps_md.contains("Expected result"));
    }

    #[test]
    fn development_summary_mentions_planning_inputs() {
        let summary = build_development_summary(
            &TaskRecord {
                id: "TASK-0002".into(),
                title: "Runner-backed development".into(),
                goal: "Generate a reviewable development output".into(),
                state: "ready_for_development".into(),
                blocked_reason_code: None,
                blocked_reason_text: None,
                current_stage: Some("development".into()),
                workspace_path: ".patron/tasks/TASK-0002".into(),
                handoff_path: ".patron/tasks/TASK-0002/orchestrator-handoff.md".into(),
            },
            "# Task\nexample",
            "# Plan\nexample",
            "# QA Steps\nexample",
        );

        assert!(summary.contains("Inputs Consumed"));
        assert!(summary.contains("task.md"));
        assert!(summary.contains("review stage"));
    }

    #[test]
    fn review_outputs_pass_when_required_sections_exist() {
        let result = review_development_outputs(
            "# Task\ncontent",
            "# Plan\ncontent",
            "## Inputs Consumed\nstuff\n## Review Readiness\nready",
        );

        assert!(!result.has_findings);
        assert!(result.findings.is_empty());
    }

    #[test]
    fn review_outputs_findings_when_sections_are_missing() {
        let result = review_development_outputs("# Task\ncontent", "# Plan\ncontent", "missing");

        assert!(result.has_findings);
        assert!(!result.findings.is_empty());
    }

    #[test]
    fn fix_log_entry_preserves_failure_context() {
        let entry = build_fix_log_entry(
            &TaskRecord {
                id: "TASK-0099".into(),
                title: "Broken review".into(),
                goal: "Exercise the fix loop".into(),
                state: "fix_required".into(),
                blocked_reason_code: None,
                blocked_reason_text: None,
                current_stage: Some("review".into()),
                workspace_path: ".patron/tasks/TASK-0099".into(),
                handoff_path: ".patron/tasks/TASK-0099/orchestrator-handoff.md".into(),
            },
            &FixLoopSource {
                source_stage: "review",
                source_path: ".patron/tasks/TASK-0099/review.md".into(),
                contents: "# Review\n\n## Findings\n- Missing section".into(),
            },
        );

        assert!(entry.contains("Source stage: `review`"));
        assert!(entry.contains("Missing section"));
        assert!(entry.contains("ready_for_development"));
    }
}
