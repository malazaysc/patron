use std::fs;
use std::path::Path;

use crate::{
    app::RuntimePaths,
    db::{
        self, ActivityEventCreate, HumanActionCreate, IntakeMessageRecord, IntakeSessionRecord,
        IntakeSessionUpdate, TaskRecord, WorkingArtifactUpsert,
    },
    domain::task_lifecycle::{
        ActorKind, HumanAction, TaskState, TaskStateMachine, TransitionMetadata,
    },
    runner::{self, RunnerCompletion, RunnerJob, RunnerOutcome},
};

pub fn status_label() -> &'static str {
    "draft intake, planning, development, review, fix-loop scaffolding, and PR preparation available; qa delegated to the qa subsystem"
}

pub fn create_draft_task(runtime: &RuntimePaths, goal: &str) -> Result<TaskRecord, String> {
    let title = derive_title(goal.trim());
    let handoff = format!(
        "# Orchestrator Handoff\n\n## Goal\n{}\n\n## Next Step\nConvert this draft task into a structured planning package.\n",
        goal.trim()
    );
    create_task_with_handoff(runtime, &title, goal, &handoff)
}

pub fn start_intake_session(
    runtime: &RuntimePaths,
    goal: &str,
) -> Result<IntakeSessionRecord, String> {
    let trimmed_goal = goal.trim();
    if trimmed_goal.is_empty() {
        return Err("goal input cannot be empty".into());
    }

    let session = db::create_intake_session(runtime, trimmed_goal)?;
    db::insert_intake_message(runtime, &session.id, "user", "goal", trimmed_goal)?;
    db::record_activity_event(
        runtime,
        ActivityEventCreate {
            scope_kind: "intake",
            scope_id: &session.id,
            task_id: None,
            event_kind: "user_goal_received",
            headline: "Goal received",
            detail: Some(trimmed_goal),
        },
    )?;
    advance_intake_session(runtime, &session.id)
}

pub fn reply_intake_session(
    runtime: &RuntimePaths,
    session_id: &str,
    reply: &str,
) -> Result<IntakeSessionRecord, String> {
    let trimmed_reply = reply.trim();
    if trimmed_reply.is_empty() {
        return Err("reply input cannot be empty".into());
    }
    let session = db::get_intake_session(runtime, session_id)?
        .ok_or_else(|| format!("intake session {session_id} was not found"))?;
    if session.status == "task_created" {
        return Err("this intake session has already been approved into a task".into());
    }
    db::insert_intake_message(runtime, session_id, "user", "answer", trimmed_reply)?;
    db::record_activity_event(
        runtime,
        ActivityEventCreate {
            scope_kind: "intake",
            scope_id: session_id,
            task_id: None,
            event_kind: "user_reply_received",
            headline: "Intake reply received",
            detail: Some(trimmed_reply),
        },
    )?;
    advance_intake_session(runtime, session_id)
}

pub fn approve_intake_session(
    runtime: &RuntimePaths,
    session_id: &str,
) -> Result<TaskRecord, String> {
    let session = db::get_intake_session(runtime, session_id)?
        .ok_or_else(|| format!("intake session {session_id} was not found"))?;
    if session.status != "draft_ready" {
        return Err("the intake draft is not ready for approval yet".into());
    }
    let draft_markdown = session
        .draft_markdown
        .as_deref()
        .ok_or_else(|| "draft content is missing".to_string())?;
    let draft_title = session
        .draft_title
        .as_deref()
        .ok_or_else(|| "draft title is missing".to_string())?;
    let handoff = format!(
        "# Orchestrator Handoff\n\n## Intake Session\n`{}`\n\n{}\n",
        session.id, draft_markdown
    );
    let task = create_task_with_handoff(runtime, draft_title, &session.initial_goal, &handoff)?;
    db::update_intake_session(
        runtime,
        session_id,
        IntakeSessionUpdate {
            status: "task_created",
            draft_title: session.draft_title.as_deref(),
            draft_markdown: session.draft_markdown.as_deref(),
            task_id: Some(task.id.as_str()),
        },
    )?;
    db::insert_intake_message(
        runtime,
        session_id,
        "system",
        "event",
        &format!(
            "Draft approved and materialized as task `{}`. Next step: run planning.",
            task.id
        ),
    )?;
    db::record_activity_event(
        runtime,
        ActivityEventCreate {
            scope_kind: "intake",
            scope_id: session_id,
            task_id: Some(task.id.as_str()),
            event_kind: "intake_approved",
            headline: "Intake approved into task",
            detail: Some(task.id.as_str()),
        },
    )?;
    Ok(task)
}

fn create_task_with_handoff(
    runtime: &RuntimePaths,
    title: &str,
    goal: &str,
    handoff_body: &str,
) -> Result<TaskRecord, String> {
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
    let handoff = format!(
        "# Task ID\n`{}`\n\n# State\n`{}`\n\n{}\n",
        task_id,
        TaskState::Draft.as_str(),
        handoff_body.trim()
    );
    fs::write(&handoff_path, handoff).map_err(|error| {
        format!(
            "failed to write orchestrator handoff file {}: {error}",
            handoff_path.display()
        )
    })?;

    let task = TaskRecord {
        id: task_id.clone(),
        title: title.to_string(),
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

fn advance_intake_session(
    runtime: &RuntimePaths,
    session_id: &str,
) -> Result<IntakeSessionRecord, String> {
    let session = db::get_intake_session(runtime, session_id)?
        .ok_or_else(|| format!("intake session {session_id} was not found"))?;
    let messages = db::list_intake_messages(runtime, session_id)?;
    let user_turns = messages
        .iter()
        .filter(|message| message.author_kind == "user")
        .count();
    let user_context = collect_user_context(&messages);
    let follow_ups =
        determine_follow_up_questions(&session.initial_goal, &user_context, user_turns);

    if !follow_ups.is_empty() {
        let prompt = format!(
            "Before I turn this into a delivery task, I need a bit more structure:\n\n{}",
            follow_ups
                .iter()
                .enumerate()
                .map(|(index, question)| format!("{}. {}", index + 1, question))
                .collect::<Vec<_>>()
                .join("\n")
        );
        db::insert_intake_message(runtime, session_id, "orchestrator", "follow_up", &prompt)?;
        db::record_activity_event(
            runtime,
            ActivityEventCreate {
                scope_kind: "intake",
                scope_id: session_id,
                task_id: None,
                event_kind: "follow_up_requested",
                headline: "Orchestrator requested follow-up",
                detail: Some(&prompt),
            },
        )?;
        db::update_intake_session(
            runtime,
            session_id,
            IntakeSessionUpdate {
                status: "awaiting_input",
                draft_title: None,
                draft_markdown: None,
                task_id: session.task_id.as_deref(),
            },
        )?;
    } else {
        let draft = build_intake_draft(&session.initial_goal, &user_context);
        db::insert_intake_message(
            runtime,
            session_id,
            "orchestrator",
            "draft",
            &format!(
                "I turned the conversation into a draft task package. Review it below, refine it if needed, or approve it to create a pipeline task.\n\nProposed title: {}",
                draft.title
            ),
        )?;
        db::record_activity_event(
            runtime,
            ActivityEventCreate {
                scope_kind: "intake",
                scope_id: session_id,
                task_id: None,
                event_kind: "draft_ready",
                headline: "Task-definition draft ready",
                detail: Some(&draft.title),
            },
        )?;
        db::update_intake_session(
            runtime,
            session_id,
            IntakeSessionUpdate {
                status: "draft_ready",
                draft_title: Some(&draft.title),
                draft_markdown: Some(&draft.markdown),
                task_id: session.task_id.as_deref(),
            },
        )?;
    }

    db::get_intake_session(runtime, session_id)?
        .ok_or_else(|| format!("intake session {session_id} disappeared during update"))
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
        let planning_context = analyze_repo_for_planning(runtime.repo_root(), &task);
        let planning_package = build_planning_package(&task, &planning_context);
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
                format!(
                    "repo analysis: state={} frameworks={} entries={}",
                    planning_context.repo_state,
                    planning_context.frameworks.join(", "),
                    planning_context.top_level_entries.join(", ")
                ),
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

struct RepoPlanningContext {
    repo_name: String,
    repo_state: &'static str,
    top_level_entries: Vec<String>,
    notable_files: Vec<String>,
    frameworks: Vec<String>,
    implementation_focus: Vec<String>,
    qa_focus: Vec<String>,
}

struct IntakeDraft {
    title: String,
    markdown: String,
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

fn collect_user_context(messages: &[IntakeMessageRecord]) -> String {
    messages
        .iter()
        .filter(|message| message.author_kind == "user")
        .map(|message| message.body.trim())
        .collect::<Vec<_>>()
        .join("\n")
}

fn determine_follow_up_questions(goal: &str, context: &str, user_turns: usize) -> Vec<String> {
    if user_turns >= 2 {
        return Vec::new();
    }

    let haystack = format!(
        "{}\n{}",
        goal.to_ascii_lowercase(),
        context.to_ascii_lowercase()
    );
    let mut questions = Vec::new();

    if !contains_any(
        &haystack,
        &[
            "django", "rust", "htmx", "alpine", "sqlite", "postgres", "react", "vue", "tailwind",
        ],
    ) {
        questions.push(
            "What technical constraints or stack choices should Patron preserve while planning this work?"
                .to_string(),
        );
    }

    if !contains_any(
        &haystack,
        &[
            "should",
            "must",
            "allow",
            "acceptance",
            "done",
            "verify",
            "qa",
            "test",
        ],
    ) {
        questions.push(
            "What should be true when this task is considered successful from a user-behavior perspective?"
                .to_string(),
        );
    }

    if !contains_any(
        &haystack,
        &[
            "page", "screen", "contact", "note", "task", "workflow", "board", "form", "entity",
            "model",
        ],
    ) {
        questions.push(
            "What are the main objects or workflows this task should cover in v1?".to_string(),
        );
    }

    questions.truncate(3);
    questions
}

fn build_intake_draft(goal: &str, context: &str) -> IntakeDraft {
    let title = derive_title(goal);
    let scope = build_scope_bullets(goal, context);
    let constraints = build_constraint_bullets(goal, context);
    let acceptance = build_acceptance_bullets(goal, context);
    let qa = build_qa_bullets(goal, context);
    let assumptions = build_assumption_bullets(goal, context);
    let markdown = format!(
        "# Draft Task Package\n\n## Goal\n{}\n\n## Scope\n{}\n\n## Constraints\n{}\n\n## Acceptance Criteria\n{}\n\n## QA Scenarios\n{}\n\n## Assumptions\n{}\n",
        goal.trim(),
        scope,
        constraints,
        acceptance,
        qa,
        assumptions
    );

    IntakeDraft { title, markdown }
}

fn build_scope_bullets(goal: &str, context: &str) -> String {
    let mut bullets = vec![
        "Build the smallest implementation slice that satisfies the requested outcome.".to_string(),
        "Keep the work deterministic, observable, and easy to review.".to_string(),
    ];
    if contains_any(
        &format!("{}\n{}", goal, context).to_ascii_lowercase(),
        &["contact", "note", "task"],
    ) {
        bullets.push(
            "Model the primary records and the key user flows around creating, viewing, and relating them."
                .to_string(),
        );
    }
    bullets
        .into_iter()
        .map(|entry| format!("- {entry}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_constraint_bullets(goal: &str, context: &str) -> String {
    let haystack = format!("{}\n{}", goal, context).to_ascii_lowercase();
    let mut bullets = Vec::new();
    for keyword in ["django", "htmx", "alpine", "sqlite", "sqlite3", "macos"] {
        if haystack.contains(keyword) {
            bullets.push(format!(
                "- Preserve the `{keyword}` constraint from the intake."
            ));
        }
    }
    if bullets.is_empty() {
        bullets.push(
            "- Preserve the repository and stack constraints described during intake.".to_string(),
        );
    }
    bullets.push("- Prefer simple, maintainable behavior over hidden automation.".to_string());
    bullets.join("\n")
}

fn build_acceptance_bullets(goal: &str, context: &str) -> String {
    let summary = summarize_behavior(goal, context);
    [
        format!("- The resulting change clearly satisfies: {summary}."),
        "- The workflow is understandable by a human reviewer without replaying chat context."
            .to_string(),
        "- QA can validate the behavior with explicit human-readable steps.".to_string(),
    ]
    .join("\n")
}

fn build_qa_bullets(goal: &str, context: &str) -> String {
    let subject = summarize_behavior(goal, context);
    [
        format!("- Exercise the main happy-path workflow for {subject}."),
        "- Verify that the visible UI/state changes match the intended outcome.".to_string(),
        "- Capture enough evidence for a human to trust the result.".to_string(),
    ]
    .join("\n")
}

fn build_assumption_bullets(goal: &str, context: &str) -> String {
    let mut bullets = Vec::new();
    if determine_follow_up_questions(goal, context, 2).is_empty() && context.trim().is_empty() {
        bullets
            .push("- No extra clarifications were captured beyond the initial goal.".to_string());
    }
    bullets.push("- Patron should preserve missing details as explicit assumptions instead of inventing hidden requirements.".to_string());
    bullets.join("\n")
}

fn summarize_behavior(goal: &str, context: &str) -> String {
    let joined = format!("{} {}", goal.trim(), context.trim());
    joined
        .split_whitespace()
        .take(18)
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
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

fn build_planning_package(task: &TaskRecord, context: &RepoPlanningContext) -> PlanningPackage {
    let sample_app_task = is_sample_app_task(task);
    let repo_context_md = render_repo_context_markdown(context);
    let scope_md = render_scope_markdown(task, context);
    let dependency_md = render_dependency_markdown(context);
    let step_lines = render_plan_steps(task, context)
        .into_iter()
        .enumerate()
        .map(|(index, step)| format!("{}. {step}", index + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let implementation_focus_md = context
        .implementation_focus
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n");
    let task_md = format!(
        "# Task\n\n## ID\n`{}`\n\n## Title\n{}\n\n## Problem\n{}\n\n## Repository Context\n{}\n\n## Scope\n{}\n\n## Constraints\n- macOS-only local runtime\n- single repository scope\n- runtime state must stay under `/.patron/`\n- avoid hidden automation or untracked side effects\n\n## Acceptance Criteria\n- A planning package exists for this task.\n- The task has `task.md`, `plan.md`, and `qa-steps.md` in its runtime workspace.\n- The task is ready for the development stage.\n- The plan names concrete repository or application targets instead of relying on generic placeholders.\n\n## Dependencies\n{}\n\n## Human Approvals\n- Task intake already completed\n- PR review still required later in the pipeline\n",
        task.id, task.title, task.goal, repo_context_md, scope_md, dependency_md
    );

    let plan_md = format!(
        "# Plan\n\n## Objective\nPrepare this task for implementation with a bounded development slice.\n\n## Steps\n{}\n\n## Implementation Focus\n{}\n\n## Development Notes\n- Use the task workspace under `/.patron/tasks/{}` as the source of runtime planning context.\n- Keep artifacts human-readable and stage-specific.\n- Avoid relying on chat history for downstream execution.\n- Planning must reflect the current repository state instead of assuming a generic scaffold.\n{}\n",
        step_lines,
        implementation_focus_md,
        task.id,
        if sample_app_task {
            "\n## Sample App Target\n- This task is intended for the built-in sample app at `/sample-app`.\n- Development and QA should verify target-app behavior instead of Patron UI internals."
        } else {
            ""
        }
    );

    let qa_steps_md = if sample_app_task {
        render_sample_app_qa_steps(task)
    } else {
        render_repo_aware_qa_steps(task, context)
    };

    PlanningPackage {
        task_md,
        plan_md,
        qa_steps_md,
    }
}

fn is_sample_app_task(task: &TaskRecord) -> bool {
    let haystack = format!(
        "{}\n{}",
        task.title.to_ascii_lowercase(),
        task.goal.to_ascii_lowercase()
    );
    haystack.contains("sample app") || haystack.contains("sample-app")
}

fn analyze_repo_for_planning(repo_root: &Path, task: &TaskRecord) -> RepoPlanningContext {
    let top_level_entries = collect_repo_entries(repo_root, 1, 8);
    let notable_files = collect_repo_entries(repo_root, 2, 10);
    let frameworks = detect_frameworks(repo_root, task, &notable_files);
    let implementation_focus = infer_implementation_focus(task, &frameworks, &notable_files);
    let qa_focus = infer_qa_focus(task, &frameworks);
    let repo_name = repo_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown-repo")
        .to_string();
    let repo_state = if notable_files.is_empty() {
        "empty"
    } else {
        "existing"
    };

    RepoPlanningContext {
        repo_name,
        repo_state,
        top_level_entries,
        notable_files,
        frameworks,
        implementation_focus,
        qa_focus,
    }
}

fn collect_repo_entries(repo_root: &Path, depth: usize, limit: usize) -> Vec<String> {
    let mut entries = Vec::new();
    visit_repo_entries(repo_root, repo_root, depth, limit, &mut entries);
    entries.sort();
    entries.truncate(limit);
    entries
}

fn visit_repo_entries(
    root: &Path,
    current: &Path,
    remaining_depth: usize,
    limit: usize,
    entries: &mut Vec<String>,
) {
    if remaining_depth == 0 || entries.len() >= limit {
        return;
    }

    let Ok(read_dir) = fs::read_dir(current) else {
        return;
    };

    for entry in read_dir.flatten() {
        if entries.len() >= limit {
            break;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if should_skip_repo_entry(&name) {
            continue;
        }

        if let Ok(relative) = path.strip_prefix(root) {
            entries.push(relative.display().to_string());
        }

        if path.is_dir() {
            visit_repo_entries(
                root,
                &path,
                remaining_depth.saturating_sub(1),
                limit,
                entries,
            );
        }
    }
}

fn should_skip_repo_entry(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".patron" | "target" | "node_modules" | "__pycache__" | ".venv" | "venv"
    )
}

fn detect_frameworks(repo_root: &Path, task: &TaskRecord, notable_files: &[String]) -> Vec<String> {
    let mut frameworks = Vec::new();
    let goal = format!("{}\n{}", task.title, task.goal).to_ascii_lowercase();
    let has = |needle: &str| notable_files.iter().any(|entry| entry.contains(needle));

    if goal.contains("django")
        || has("manage.py")
        || has("requirements.txt")
        || has("pyproject.toml")
    {
        frameworks.push("Django".to_string());
    }
    if goal.contains("htmx") || has("templates") {
        frameworks.push("HTMX".to_string());
    }
    if goal.contains("alpine") || goal.contains("alpine.js") {
        frameworks.push("Alpine.js".to_string());
    }
    if goal.contains("sqlite") || goal.contains("sqlite3") {
        frameworks.push("SQLite".to_string());
    }
    if has("Cargo.toml") && repo_root.join("src").exists() {
        frameworks.push("Rust".to_string());
    }
    if frameworks.is_empty() {
        frameworks.push("Undetermined".to_string());
    }
    frameworks
}

fn infer_implementation_focus(
    task: &TaskRecord,
    frameworks: &[String],
    notable_files: &[String],
) -> Vec<String> {
    let haystack = format!("{}\n{}", task.title, task.goal).to_ascii_lowercase();
    let mut focus = Vec::new();

    if frameworks.iter().any(|framework| framework == "Django") {
        if notable_files.is_empty() {
            focus.push("Scaffold the Django project and the first application module.".to_string());
        } else {
            focus.push(
                "Extend the existing Django project instead of creating a parallel scaffold."
                    .to_string(),
            );
        }
    }
    if haystack.contains("contact") {
        focus.push(
            "Model a contact record with the fields needed for a credible CRM v1.".to_string(),
        );
    }
    if haystack.contains("note") {
        focus.push("Add note records that can be attached to a specific contact.".to_string());
    }
    if haystack.contains("task") {
        focus.push("Add task records that can optionally relate back to a contact.".to_string());
    }
    if frameworks.iter().any(|framework| framework == "HTMX") {
        focus.push(
            "Use HTMX to keep create and update flows visibly interactive without a SPA shell."
                .to_string(),
        );
    }
    if frameworks.iter().any(|framework| framework == "Alpine.js") {
        focus.push(
            "Reserve Alpine.js for lightweight local state and UI affordances only.".to_string(),
        );
    }
    if focus.is_empty() {
        focus.push("Implement the smallest repo-aware delivery slice that satisfies the requested outcome.".to_string());
    }

    focus
}

fn infer_qa_focus(task: &TaskRecord, frameworks: &[String]) -> Vec<String> {
    let haystack = format!("{}\n{}", task.title, task.goal).to_ascii_lowercase();
    let mut focus = Vec::new();

    if haystack.contains("contact") {
        focus.push("Creating a contact through the primary UI flow.".to_string());
    }
    if haystack.contains("note") {
        focus.push(
            "Adding a note to a contact and seeing it render in the contact view.".to_string(),
        );
    }
    if haystack.contains("task") {
        focus.push("Creating a task that is visibly linked to a contact.".to_string());
    }
    if frameworks.iter().any(|framework| framework == "HTMX") {
        focus.push(
            "Verifying that updates appear through HTMX-driven partial refreshes.".to_string(),
        );
    }
    if focus.is_empty() {
        focus.push("Verifying the main requested behavior through the application UI.".to_string());
    }

    focus
}

fn render_repo_context_markdown(context: &RepoPlanningContext) -> String {
    let top_level = if context.top_level_entries.is_empty() {
        "- No top-level project files were detected yet.".to_string()
    } else {
        context
            .top_level_entries
            .iter()
            .map(|entry| format!("- `{entry}`"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let notable = if context.notable_files.is_empty() {
        "- The repository appears effectively empty, so planning should include initial scaffolding work.".to_string()
    } else {
        context
            .notable_files
            .iter()
            .map(|entry| format!("- `{entry}`"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let frameworks = context
        .frameworks
        .iter()
        .map(|framework| format!("- {framework}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "- Repository: `{}`\n- Repo state: `{}`\n- Detected stack signals:\n{}\n- Top-level entries:\n{}\n- Notable files:\n{}",
        context.repo_name, context.repo_state, frameworks, top_level, notable
    )
}

fn render_scope_markdown(task: &TaskRecord, context: &RepoPlanningContext) -> String {
    let mut bullets = vec![
        "- Build the smallest implementation slice that satisfies the requested goal.".to_string(),
        "- Keep the implementation local-first and repository-scoped.".to_string(),
        "- Preserve deterministic task progression for later stages.".to_string(),
    ];

    bullets.extend(
        context
            .implementation_focus
            .iter()
            .map(|item| format!("- {item}")),
    );

    if task.goal.to_ascii_lowercase().contains("crm") {
        bullets.push(
            "- Keep the initial CRM slice focused on contacts, notes, and related tasks rather than broader sales workflows."
                .to_string(),
        );
    }

    bullets.join("\n")
}

fn render_dependency_markdown(context: &RepoPlanningContext) -> String {
    let mut bullets = vec!["- `.patron/` task workspace".to_string()];
    if context.repo_state == "empty" {
        bullets.push("- Initial application scaffold work in the target repository.".to_string());
    }
    bullets.extend(
        context
            .frameworks
            .iter()
            .filter(|framework| framework.as_str() != "Undetermined")
            .map(|framework| format!("- {framework} project dependencies or configuration")),
    );
    if bullets.len() == 1 {
        bullets.push("- Existing repository files identified during planning.".to_string());
    }
    bullets.join("\n")
}

fn render_plan_steps(task: &TaskRecord, context: &RepoPlanningContext) -> Vec<String> {
    let haystack = format!("{}\n{}", task.title, task.goal).to_ascii_lowercase();
    let mut steps = Vec::new();

    if context.repo_state == "empty" {
        steps.push(
            "Confirm the repository is still empty enough to require initial scaffolding."
                .to_string(),
        );
    } else {
        steps.push("Inspect the existing repository structure and confirm the most relevant implementation targets.".to_string());
    }

    if context
        .frameworks
        .iter()
        .any(|framework| framework == "Django")
    {
        if context.repo_state == "empty" {
            steps.push(
                "Create the Django project and a first app that will hold the CRM functionality."
                    .to_string(),
            );
        } else {
            steps.push("Locate the existing Django project/app boundaries and extend them without duplicating structure.".to_string());
        }
    }

    if haystack.contains("contact") && haystack.contains("note") && haystack.contains("task") {
        steps.push("Define the contact, note, and related task data model along with the relationship boundaries for v1.".to_string());
        steps.push("Implement the first CRUD-oriented UI slice for creating and browsing contacts with linked notes and tasks.".to_string());
    } else {
        steps.push("Implement the smallest viable change set for the requested goal.".to_string());
    }

    if context
        .frameworks
        .iter()
        .any(|framework| framework == "HTMX")
    {
        steps.push("Use HTMX-driven interactions for the key create or update workflows so the behavior is easy to verify in QA.".to_string());
    }

    steps.push(
        "Validate the changed behavior locally and hand off reviewable artifacts before QA begins."
            .to_string(),
    );
    steps
}

fn render_sample_app_qa_steps(task: &TaskRecord) -> String {
    format!(
        "# QA Steps\n\n## Scenario 1: Task artifacts are available\n- Open the task workspace for `{}`.\n- Confirm that `task.md`, `plan.md`, and `qa-steps.md` exist.\n- Expected result: all three planning artifacts are present and readable.\n\n## Scenario 2: Planning output reflects the requested goal\n- Read `task.md` and `plan.md`.\n- Confirm the original goal is captured and the plan describes concrete next steps.\n- Expected result: the planning package is aligned with the original goal and is understandable by a human.\n\n## Scenario 3: Sample app route loads successfully\n- Open the built-in sample app at `http://127.0.0.1:3000/sample-app`.\n- Confirm the page renders the triage board and interactive controls.\n- Expected result: the sample app is reachable and client-side rendering completes.\n\n## Scenario 4: Browser evidence is captured against the sample app\n- Run browser-driven QA against the sample app route instead of the Patron board.\n- Capture a screenshot and HAR file after the sample app finishes loading.\n- Expected result: QA leaves behind inspectable browser evidence for the sample app flow.\n\n## Evidence Requirements\n- Capture the presence of the planning artifacts.\n- Preserve the QA browser screenshot, HAR file, and QA log.\n- Record the final QA outcome and any missing evidence in `qa-report.md`.\n",
        task.id
    )
}

fn render_repo_aware_qa_steps(task: &TaskRecord, context: &RepoPlanningContext) -> String {
    let behavior_lines = context
        .qa_focus
        .iter()
        .enumerate()
        .map(|(index, item)| {
            format!(
                "## Scenario {}: {}\n- Execute the primary UI flow for this behavior.\n- Confirm the visible state matches the planned outcome.\n- Expected result: {} is working end to end.\n",
                index + 3,
                item,
                item
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let target_hint = if context
        .frameworks
        .iter()
        .any(|framework| framework == "Django")
    {
        "- Start the local Django application before browser-driven QA begins.\n- Use the target application UI as the verification surface rather than the Patron board.\n"
    } else {
        "- Start the target application before browser-driven QA begins.\n"
    };

    format!(
        "# QA Steps\n\n## Scenario 1: Task artifacts are available\n- Open the task workspace for `{}`.\n- Confirm that `task.md`, `plan.md`, and `qa-steps.md` exist.\n- Expected result: all three planning artifacts are present and readable.\n\n## Scenario 2: Planning output reflects the requested goal and repository state\n- Read `task.md` and `plan.md`.\n- Confirm the original goal is captured, the plan names concrete repository/application targets, and the repo state is reflected accurately.\n- Expected result: the planning package is aligned with the repository and is understandable by a human.\n\n{}## Evidence Requirements\n{}- Preserve the QA browser screenshot, HAR file, and QA log.\n- Record the final QA outcome and any missing evidence in `qa-report.md`.\n",
        task.id, behavior_lines, target_hint
    )
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
        FixLoopSource, RepoPlanningContext, TaskRecord, build_development_summary,
        build_fix_log_entry, build_planning_package, derive_title, review_development_outputs,
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
        let package = build_planning_package(
            &TaskRecord {
                id: "TASK-0001".into(),
                title: "Build draft intake".into(),
                goal: "Build the draft task intake flow".into(),
                state: "draft".into(),
                blocked_reason_code: None,
                blocked_reason_text: None,
                current_stage: None,
                workspace_path: ".patron/tasks/TASK-0001".into(),
                handoff_path: ".patron/tasks/TASK-0001/orchestrator-handoff.md".into(),
            },
            &test_repo_context(
                "existing",
                &["src", "Cargo.toml"],
                &["Rust"],
                &["runner flow"],
                &["task UI"],
            ),
        );

        assert!(package.task_md.contains("# Task"));
        assert!(package.plan_md.contains("# Plan"));
        assert!(package.qa_steps_md.contains("## Scenario 1"));
        assert!(package.qa_steps_md.contains("Expected result"));
    }

    #[test]
    fn planning_package_mentions_repo_context_and_specific_targets() {
        let package = build_planning_package(
            &TaskRecord {
                id: "TASK-0100".into(),
                title: "Build CRM slice".into(),
                goal: "Create a small CRM using django htmx alpine.js and sqlite3 with contacts notes and related tasks".into(),
                state: "draft".into(),
                blocked_reason_code: None,
                blocked_reason_text: None,
                current_stage: None,
                workspace_path: ".patron/tasks/TASK-0100".into(),
                handoff_path: ".patron/tasks/TASK-0100/orchestrator-handoff.md".into(),
            },
            &test_repo_context(
                "empty",
                &[],
                &["Django", "HTMX", "Alpine.js", "SQLite"],
                &[
                    "Scaffold the Django project and the first application module.",
                    "Model a contact record with the fields needed for a credible CRM v1.",
                ],
                &[
                    "Creating a contact through the primary UI flow.",
                    "Adding a note to a contact and seeing it render in the contact view.",
                ],
            ),
        );

        assert!(package.task_md.contains("## Repository Context"));
        assert!(package.task_md.contains("Repo state: `empty`"));
        assert!(
            package
                .plan_md
                .contains("Create the Django project and a first app")
        );
        assert!(
            package
                .qa_steps_md
                .contains("Creating a contact through the primary UI flow.")
        );
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

    fn test_repo_context(
        repo_state: &'static str,
        notable_files: &[&str],
        frameworks: &[&str],
        implementation_focus: &[&str],
        qa_focus: &[&str],
    ) -> RepoPlanningContext {
        RepoPlanningContext {
            repo_name: "test-repo".into(),
            repo_state,
            top_level_entries: notable_files
                .iter()
                .map(|value| value.to_string())
                .collect(),
            notable_files: notable_files
                .iter()
                .map(|value| value.to_string())
                .collect(),
            frameworks: frameworks.iter().map(|value| value.to_string()).collect(),
            implementation_focus: implementation_focus
                .iter()
                .map(|value| value.to_string())
                .collect(),
            qa_focus: qa_focus.iter().map(|value| value.to_string()).collect(),
        }
    }
}
