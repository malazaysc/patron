use std::fs;
use std::path::PathBuf;

use crate::{
    app::RuntimePaths,
    db::{self, StageRunRecord, WorkingArtifactUpsert},
};

#[derive(Clone, Debug)]
pub struct RunnerJob {
    pub task_id: String,
    pub stage: String,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum RunnerCompletion {
    Completed,
    Failed,
    Interrupted,
}

#[derive(Clone, Debug)]
pub struct RunnerOutcome {
    pub completion: RunnerCompletion,
    pub exit_code: i64,
    pub error_summary: Option<String>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct RunnerExecution {
    pub run: StageRunRecord,
    pub log_path: PathBuf,
}

pub fn status_label() -> &'static str {
    "single-stage runner wrapper available"
}

pub fn execute_job<F>(
    runtime: &RuntimePaths,
    job: RunnerJob,
    body: F,
) -> Result<RunnerExecution, String>
where
    F: FnOnce(&StageRunRecord, &PathBuf) -> Result<RunnerOutcome, String>,
{
    let run = db::create_stage_run(runtime, &job.task_id, &job.stage)?;
    let run_dir = runtime.runs_dir.join(&job.task_id);
    fs::create_dir_all(&run_dir).map_err(|error| {
        format!(
            "failed to create runner directory {}: {error}",
            run_dir.display()
        )
    })?;

    let log_path = run_dir.join(format!("{}.log", run.id));
    fs::write(
        &log_path,
        format!(
            "run_id: {}\nstage: {}\nsummary: {}\nstatus: running\n\n",
            run.id, run.stage, job.summary
        ),
    )
    .map_err(|error| {
        format!(
            "failed to write initial runner log {}: {error}",
            log_path.display()
        )
    })?;

    let body_result = body(&run, &log_path);
    match body_result {
        Ok(outcome) => {
            append_log(
                &log_path,
                &format!(
                    "completion: {:?}\nexit_code: {}\n{}\n",
                    outcome.completion,
                    outcome.exit_code,
                    outcome
                        .error_summary
                        .as_deref()
                        .map(|summary| format!("error_summary: {summary}"))
                        .unwrap_or_else(|| "error_summary: none".to_string())
                ),
            )?;
            db::complete_stage_run(
                runtime,
                &run.id,
                completion_status(&outcome.completion),
                Some(outcome.exit_code),
                outcome.error_summary.as_deref(),
            )?;
            db::upsert_working_artifact(
                runtime,
                WorkingArtifactUpsert {
                    task_id: &job.task_id,
                    role: &format!("run_log_{}", run.id),
                    artifact_kind: "runner_log",
                    relative_path: &format!(".patron/runs/{}/{}.log", job.task_id, run.id),
                    media_type: "text/plain",
                    required_for_stage: false,
                    stage_run_id: Some(run.id.as_str()),
                },
            )?;

            Ok(RunnerExecution { run, log_path })
        }
        Err(error) => {
            append_log(
                &log_path,
                &format!("completion: Failed\nexit_code: 1\nerror_summary: {error}\n"),
            )?;
            db::complete_stage_run(runtime, &run.id, "failed", Some(1), Some(&error))?;
            db::upsert_working_artifact(
                runtime,
                WorkingArtifactUpsert {
                    task_id: &job.task_id,
                    role: &format!("run_log_{}", run.id),
                    artifact_kind: "runner_log",
                    relative_path: &format!(".patron/runs/{}/{}.log", job.task_id, run.id),
                    media_type: "text/plain",
                    required_for_stage: false,
                    stage_run_id: Some(run.id.as_str()),
                },
            )?;
            Err(error)
        }
    }
}

fn append_log(log_path: &PathBuf, entry: &str) -> Result<(), String> {
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
    file.write_all(entry.as_bytes()).map_err(|error| {
        format!(
            "failed to append runner log {}: {error}",
            log_path.display()
        )
    })
}

fn completion_status(completion: &RunnerCompletion) -> &'static str {
    match completion {
        RunnerCompletion::Completed => "completed",
        RunnerCompletion::Failed => "failed",
        RunnerCompletion::Interrupted => "interrupted",
    }
}
