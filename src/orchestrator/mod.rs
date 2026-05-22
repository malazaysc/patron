use std::fs;
use std::path::Path;

use crate::{
    app::RuntimePaths,
    db::{self, TaskRecord},
    domain::task_lifecycle::{ActorKind, TaskState, TaskStateMachine, TransitionMetadata},
};

pub fn status_label() -> &'static str {
    "draft intake and planning scaffolding available"
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

    let stage_run = db::create_stage_run(runtime, &task.id, "planning")?;
    let planning_metadata = transition_metadata(
        ActorKind::Orchestrator,
        "planning started",
        Some("planning_started"),
        Some(stage_run.id.as_str()),
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
        &task.id,
        "task_md",
        "task_document",
        &format!("{task_root}/task.md"),
        "text/markdown",
        true,
        Some(stage_run.id.as_str()),
    )?;
    db::upsert_working_artifact(
        runtime,
        &task.id,
        "plan_md",
        "plan_document",
        &format!("{task_root}/plan.md"),
        "text/markdown",
        true,
        Some(stage_run.id.as_str()),
    )?;
    db::upsert_working_artifact(
        runtime,
        &task.id,
        "qa_steps_md",
        "qa_steps_document",
        &format!("{task_root}/qa-steps.md"),
        "text/markdown",
        true,
        Some(stage_run.id.as_str()),
    )?;

    let finished_metadata = transition_metadata(
        ActorKind::Orchestrator,
        "planning completed and artifacts generated",
        Some("planning_completed"),
        Some(stage_run.id.as_str()),
    );
    TaskStateMachine::validate_transition(
        TaskState::Planning,
        TaskState::ReadyForDevelopment,
        &finished_metadata,
    )
    .map_err(|error| format!("invalid planning->ready_for_development transition: {error:?}"))?;
    db::transition_task_state(
        runtime,
        &task.id,
        TaskState::Planning,
        TaskState::ReadyForDevelopment,
        &finished_metadata,
    )?;
    db::complete_stage_run(runtime, &stage_run.id, "completed", None)?;

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
        "# QA Steps\n\n## Scenario 1: Task artifacts are available\n- Open the task workspace for `{}`.\n- Confirm that `task.md`, `plan.md`, and `qa-steps.md` exist.\n- Expected result: all three planning artifacts are present and readable.\n\n## Scenario 2: Planning output reflects the requested goal\n- Read `task.md` and `plan.md`.\n- Confirm the original goal is captured and the plan describes concrete next steps.\n- Expected result: the planning package is aligned with the original goal and is understandable by a human.\n\n## Scenario 3: The task is ready for development\n- Inspect the task state in the Patron UI or SQLite state.\n- Confirm the task moved to `ready_for_development` after planning completed.\n- Expected result: development can start without requiring additional planning work.\n\n## Evidence Requirements\n- Capture the presence of the three planning artifacts.\n- Record the post-planning task state.\n- Preserve any planning errors if the task fails to advance.\n",
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
    use super::{TaskRecord, build_planning_package, derive_title};

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
            current_stage: None,
            workspace_path: ".patron/tasks/TASK-0001".into(),
            handoff_path: ".patron/tasks/TASK-0001/orchestrator-handoff.md".into(),
        });

        assert!(package.task_md.contains("# Task"));
        assert!(package.plan_md.contains("# Plan"));
        assert!(package.qa_steps_md.contains("## Scenario 1"));
        assert!(package.qa_steps_md.contains("Expected result"));
    }
}
