use std::path::Path;

use crate::{
    app::RuntimePaths,
    db::{self, StageRunRecord, WorkingArtifactUpsert},
    domain::task_lifecycle::{ActorKind, TaskState, TaskStateMachine, TransitionMetadata},
};

pub fn reconcile_interrupted_runs(runtime: &RuntimePaths) -> Result<(), String> {
    let open_runs = db::list_open_stage_runs(runtime)?;
    for run in open_runs {
        db::complete_stage_run(
            runtime,
            &run.id,
            "interrupted",
            None,
            Some("run was still marked running during startup reconciliation"),
        )?;

        let Some(task) = db::get_task(runtime, &run.task_id)? else {
            continue;
        };
        let current_state = parse_task_state(&task.state)?;
        if !current_state.is_active_stage() {
            continue;
        }

        register_recovered_artifacts(runtime, &run)?;

        let recommendation = recommend_recovery(runtime, &run.task_id, &run, current_state)?;
        let metadata = TransitionMetadata {
            actor: ActorKind::System,
            actor_id: None,
            occurred_at: timestamp_now(),
            reason_code: Some(recommendation.reason_code.to_string()),
            reason_text: recommendation.reason_text,
            run_id: Some(run.id.clone()),
            required_human_action: None,
        };

        TaskStateMachine::validate_transition(
            current_state,
            recommendation.target_state,
            &metadata,
        )
        .map_err(|error| {
            format!(
                "invalid recovery transition for {} from {} to {}: {error:?}",
                run.task_id,
                current_state.as_str(),
                recommendation.target_state.as_str()
            )
        })?;
        db::transition_task_state(
            runtime,
            &run.task_id,
            current_state,
            recommendation.target_state,
            &metadata,
        )?;
    }

    Ok(())
}

struct RecoveryRecommendation {
    target_state: TaskState,
    reason_code: &'static str,
    reason_text: String,
}

fn recommend_recovery(
    runtime: &RuntimePaths,
    task_id: &str,
    run: &StageRunRecord,
    current_state: TaskState,
) -> Result<RecoveryRecommendation, String> {
    let workspace_path = runtime.tasks_dir.join(task_id);
    let recommendation = match run.stage.as_str() {
        "planning" => {
            let completed = all_exist(&[
                &workspace_path.join("task.md"),
                &workspace_path.join("plan.md"),
                &workspace_path.join("qa-steps.md"),
            ]);
            if completed {
                RecoveryRecommendation {
                    target_state: TaskState::ReadyForDevelopment,
                    reason_code: "recovered_completed_artifacts",
                    reason_text: format!(
                        "startup recovery found a complete planning package for interrupted run {}",
                        run.id
                    ),
                }
            } else {
                RecoveryRecommendation {
                    target_state: TaskState::ReadyForPlanning,
                    reason_code: "recovered_retry_ready",
                    reason_text: format!(
                        "startup recovery reset interrupted planning run {} to ready_for_planning",
                        run.id
                    ),
                }
            }
        }
        "development" => {
            if workspace_path.join("development-summary.md").exists() {
                RecoveryRecommendation {
                    target_state: TaskState::ReadyForReview,
                    reason_code: "recovered_completed_artifacts",
                    reason_text: format!(
                        "startup recovery found development-summary.md for interrupted run {}",
                        run.id
                    ),
                }
            } else {
                RecoveryRecommendation {
                    target_state: TaskState::ReadyForDevelopment,
                    reason_code: "recovered_retry_ready",
                    reason_text: format!(
                        "startup recovery reset interrupted development run {} to ready_for_development",
                        run.id
                    ),
                }
            }
        }
        "review" => {
            let review_path = workspace_path.join("review.md");
            if review_path.exists() {
                let review_contents = std::fs::read_to_string(&review_path).map_err(|error| {
                    format!("failed to read {}: {error}", review_path.display())
                })?;
                if review_contents.contains("- Status: fix_required") {
                    RecoveryRecommendation {
                        target_state: TaskState::FixRequired,
                        reason_code: "recovered_review_findings",
                        reason_text: format!(
                            "startup recovery found review findings for interrupted run {}",
                            run.id
                        ),
                    }
                } else {
                    RecoveryRecommendation {
                        target_state: TaskState::ReadyForQa,
                        reason_code: "recovered_completed_artifacts",
                        reason_text: format!(
                            "startup recovery found a passing review document for interrupted run {}",
                            run.id
                        ),
                    }
                }
            } else {
                RecoveryRecommendation {
                    target_state: TaskState::ReadyForReview,
                    reason_code: "recovered_retry_ready",
                    reason_text: format!(
                        "startup recovery reset interrupted review run {} to ready_for_review",
                        run.id
                    ),
                }
            }
        }
        "qa" => recommend_qa_recovery(&workspace_path, run)?,
        "pr_preparation" => {
            if workspace_path.join("pr-summary.md").exists() {
                RecoveryRecommendation {
                    target_state: TaskState::AwaitingHuman,
                    reason_code: "recovered_completed_artifacts",
                    reason_text: format!(
                        "startup recovery found pr-summary.md for interrupted run {}",
                        run.id
                    ),
                }
            } else {
                RecoveryRecommendation {
                    target_state: TaskState::ReadyForPr,
                    reason_code: "recovered_retry_ready",
                    reason_text: format!(
                        "startup recovery reset interrupted PR preparation run {} to ready_for_pr",
                        run.id
                    ),
                }
            }
        }
        _ => RecoveryRecommendation {
            target_state: TaskState::Blocked,
            reason_code: "recovered_manual_investigation",
            reason_text: format!(
                "startup recovery could not safely classify interrupted run {} and blocked the task for inspection",
                run.id
            ),
        },
    };

    if recommendation.target_state == TaskState::Blocked && current_state == TaskState::Blocked {
        return Ok(RecoveryRecommendation {
            target_state: current_state,
            reason_code: recommendation.reason_code,
            reason_text: recommendation.reason_text,
        });
    }

    Ok(recommendation)
}

fn register_recovered_artifacts(
    runtime: &RuntimePaths,
    run: &StageRunRecord,
) -> Result<(), String> {
    let workspace_path = runtime.tasks_dir.join(&run.task_id);
    let qa_report_path = workspace_path.join("qa-report.md");
    if qa_report_path.exists() {
        db::upsert_working_artifact(
            runtime,
            WorkingArtifactUpsert {
                task_id: &run.task_id,
                role: "qa_report_md",
                artifact_kind: "qa_report",
                relative_path: &format!(".patron/tasks/{}/qa-report.md", run.task_id),
                media_type: "text/markdown",
                required_for_stage: true,
                stage_run_id: Some(run.id.as_str()),
            },
        )?;
    }

    let pr_summary_path = workspace_path.join("pr-summary.md");
    if pr_summary_path.exists() {
        db::upsert_working_artifact(
            runtime,
            WorkingArtifactUpsert {
                task_id: &run.task_id,
                role: "pr_summary_md",
                artifact_kind: "pr_summary",
                relative_path: &format!(".patron/tasks/{}/pr-summary.md", run.task_id),
                media_type: "text/markdown",
                required_for_stage: true,
                stage_run_id: Some(run.id.as_str()),
            },
        )?;
    }

    Ok(())
}

fn recommend_qa_recovery(
    workspace_path: &Path,
    run: &StageRunRecord,
) -> Result<RecoveryRecommendation, String> {
    let qa_report_path = workspace_path.join("qa-report.md");
    if !qa_report_path.exists() {
        return Ok(RecoveryRecommendation {
            target_state: TaskState::ReadyForQa,
            reason_code: "recovered_retry_ready",
            reason_text: format!(
                "startup recovery reset interrupted QA run {} to ready_for_qa",
                run.id
            ),
        });
    }

    let qa_report = std::fs::read_to_string(&qa_report_path)
        .map_err(|error| format!("failed to read {}: {error}", qa_report_path.display()))?;
    if qa_report.contains("Next state: `fix_required`") {
        return Ok(RecoveryRecommendation {
            target_state: TaskState::FixRequired,
            reason_code: "recovered_qa_findings",
            reason_text: format!(
                "startup recovery found QA findings for interrupted run {}",
                run.id
            ),
        });
    }

    let has_evidence = qa_report.contains("Screenshot:")
        && qa_report.contains("HAR:")
        && qa_report.contains("QA log:");
    if has_evidence && qa_report.contains("Next state: `ready_for_pr`") {
        Ok(RecoveryRecommendation {
            target_state: TaskState::ReadyForPr,
            reason_code: "recovered_completed_artifacts",
            reason_text: format!(
                "startup recovery found a complete QA report and evidence references for interrupted run {}",
                run.id
            ),
        })
    } else {
        Ok(RecoveryRecommendation {
            target_state: TaskState::Blocked,
            reason_code: "recovered_missing_evidence",
            reason_text: format!(
                "startup recovery found QA output for interrupted run {} but evidence was incomplete",
                run.id
            ),
        })
    }
}

fn all_exist(paths: &[&Path]) -> bool {
    paths.iter().all(|path| path.exists())
}

fn timestamp_now() -> String {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => format!("unix:{}", duration.as_secs()),
        Err(_) => "unix:0".to_string(),
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
