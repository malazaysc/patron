use std::fs;

use crate::{
    app::RuntimePaths,
    db::{self, TaskRecord},
    domain::task_lifecycle::TaskState,
};

pub fn status_label() -> &'static str {
    "goal intake and planning scaffolding pending"
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
        workspace_path: workspace_relative.clone(),
        handoff_path: format!("{workspace_relative}/orchestrator-handoff.md"),
    };

    db::insert_task(runtime, &task)?;
    db::register_workspace_artifact(runtime, &task_id, &workspace_relative)?;

    Ok(task)
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

#[cfg(test)]
mod tests {
    use super::derive_title;

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
}
