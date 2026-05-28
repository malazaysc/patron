use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{
    app::RuntimePaths,
    db::{self, StageRunRecord, WorkingArtifactUpsert},
};

#[derive(Clone, Debug)]
pub struct RunnerJob {
    pub task_id: String,
    pub stage: String,
    pub summary: String,
    pub repo_root: String,
    pub repo_name: String,
    pub git_branch: Option<String>,
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

#[derive(Clone, Debug)]
pub struct RepoSnapshot {
    pub branch: String,
    pub status_porcelain: String,
    pub changed_files: Vec<String>,
    pub diff_stat: String,
    pub diff_patch: String,
    pub has_changes: bool,
}

#[derive(Clone, Debug)]
pub struct RepoExecutionResult {
    pub final_message: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i64,
}

pub fn status_label() -> &'static str {
    "single-stage runner wrapper with codex execution and repo-state capture available"
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
            "run_id: {}\nstage: {}\nsummary: {}\nrepo_root: {}\nrepo_name: {}\ngit_branch: {}\nstatus: running\n\n",
            run.id,
            run.stage,
            job.summary,
            job.repo_root,
            job.repo_name,
            job.git_branch.as_deref().unwrap_or("unknown")
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

pub fn append_runner_log(log_path: &Path, entry: &str) -> Result<(), String> {
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

pub fn capture_repo_snapshot(repo_root: &Path) -> Result<RepoSnapshot, String> {
    let branch = git_output(repo_root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|_| "unknown".to_string());
    let status_porcelain = git_output(repo_root, &["status", "--short"]).unwrap_or_default();
    let filtered_status = status_porcelain
        .lines()
        .filter(|line| !line.trim().ends_with(".patron/") && !line.contains(".patron/"))
        .collect::<Vec<_>>()
        .join("\n");
    let untracked_files = git_output(repo_root, &["ls-files", "--others", "--exclude-standard"])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty() && !path.starts_with(".patron/"))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    let mut changed_files = filtered_status
        .lines()
        .filter_map(parse_status_line)
        .flat_map(|path| expand_changed_path(repo_root, &path))
        .collect::<Vec<_>>();
    for path in untracked_files {
        if !changed_files.iter().any(|existing| existing == &path) {
            changed_files.push(path);
        }
    }
    changed_files.sort();
    changed_files.dedup();

    let diff_stat = compose_repo_diff(repo_root, &changed_files, true)?;
    let diff_patch = compose_repo_diff(repo_root, &changed_files, false)?;

    Ok(RepoSnapshot {
        branch,
        status_porcelain: filtered_status.clone(),
        changed_files: changed_files.clone(),
        diff_stat,
        diff_patch,
        has_changes: !filtered_status.trim().is_empty() || !changed_files.is_empty(),
    })
}

pub fn write_repo_artifact_set(
    runtime: &RuntimePaths,
    task_id: &str,
    run_id: &str,
    role_prefix: &str,
    before: &RepoSnapshot,
    after: &RepoSnapshot,
) -> Result<Vec<PathBuf>, String> {
    let workspace_path = runtime.tasks_dir.join(task_id);
    fs::create_dir_all(&workspace_path).map_err(|error| {
        format!(
            "failed to ensure task workspace {}: {error}",
            workspace_path.display()
        )
    })?;

    let before_path = workspace_path.join(format!("{role_prefix}-repo-status-before.txt"));
    let after_path = workspace_path.join(format!("{role_prefix}-repo-status-after.txt"));
    let changed_files_path = workspace_path.join(format!("{role_prefix}-changed-files.txt"));
    let diff_stat_path = workspace_path.join(format!("{role_prefix}-diff-stat.txt"));
    let diff_patch_path = workspace_path.join(format!("{role_prefix}-diff.patch"));

    fs::write(
        &before_path,
        format!(
            "branch: {}\nstatus:\n{}\n",
            before.branch,
            if before.status_porcelain.trim().is_empty() {
                "(clean)\n"
            } else {
                &before.status_porcelain
            }
        ),
    )
    .map_err(|error| format!("failed to write {}: {error}", before_path.display()))?;
    fs::write(
        &after_path,
        format!(
            "branch: {}\nstatus:\n{}\n",
            after.branch,
            if after.status_porcelain.trim().is_empty() {
                "(clean)\n"
            } else {
                &after.status_porcelain
            }
        ),
    )
    .map_err(|error| format!("failed to write {}: {error}", after_path.display()))?;
    fs::write(
        &changed_files_path,
        if after.changed_files.is_empty() {
            "(no changed files)\n".to_string()
        } else {
            after.changed_files.join("\n") + "\n"
        },
    )
    .map_err(|error| format!("failed to write {}: {error}", changed_files_path.display()))?;
    fs::write(
        &diff_stat_path,
        if after.diff_stat.trim().is_empty() {
            "(no diff stat available)\n".to_string()
        } else {
            after.diff_stat.clone()
        },
    )
    .map_err(|error| format!("failed to write {}: {error}", diff_stat_path.display()))?;
    fs::write(
        &diff_patch_path,
        if after.diff_patch.trim().is_empty() {
            "(no patch available)\n".to_string()
        } else {
            after.diff_patch.clone()
        },
    )
    .map_err(|error| format!("failed to write {}: {error}", diff_patch_path.display()))?;

    let files = [
        (
            &before_path,
            format!("{role_prefix}_repo_status_before"),
            "text/plain",
        ),
        (
            &after_path,
            format!("{role_prefix}_repo_status_after"),
            "text/plain",
        ),
        (
            &changed_files_path,
            format!("{role_prefix}_changed_files"),
            "text/plain",
        ),
        (
            &diff_stat_path,
            format!("{role_prefix}_diff_stat"),
            "text/plain",
        ),
        (
            &diff_patch_path,
            format!("{role_prefix}_diff_patch"),
            "text/x-diff",
        ),
    ];

    for (path, role, media_type) in files {
        db::upsert_working_artifact(
            runtime,
            WorkingArtifactUpsert {
                task_id,
                role: &role,
                artifact_kind: "repo_state",
                relative_path: &format!(
                    ".patron/tasks/{task_id}/{}",
                    path.file_name()
                        .and_then(|v| v.to_str())
                        .unwrap_or_default()
                ),
                media_type,
                required_for_stage: false,
                stage_run_id: Some(run_id),
            },
        )?;
    }

    Ok(vec![
        before_path,
        after_path,
        changed_files_path,
        diff_stat_path,
        diff_patch_path,
    ])
}

pub fn run_codex_exec(
    repo_root: &Path,
    prompt: &str,
    output_message_path: &Path,
) -> Result<RepoExecutionResult, String> {
    let mut child = Command::new("codex")
        .arg("exec")
        .arg("-C")
        .arg(repo_root)
        .arg("-s")
        .arg("workspace-write")
        .arg("-o")
        .arg(output_message_path)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to spawn codex exec: {error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|error| format!("failed to write prompt to codex exec: {error}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for codex exec: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let final_message = fs::read_to_string(output_message_path).unwrap_or_default();
    let exit_code = output.status.code().map_or(1, i64::from);

    Ok(RepoExecutionResult {
        final_message,
        stdout,
        stderr,
        exit_code,
    })
}

fn git_output(repo_root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .map_err(|error| format!("failed to run git {:?}: {error}", args))?;
    if !output.status.success() {
        return Err(format!(
            "git {:?} failed with status {}",
            args,
            output
                .status
                .code()
                .map_or_else(|| "signal".to_string(), |code| code.to_string())
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string())
}

fn parse_status_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() > 3 {
        Some(trimmed[3..].to_string())
    } else {
        Some(trimmed.to_string())
    }
}

fn expand_changed_path(repo_root: &Path, path: &str) -> Vec<String> {
    if path.starts_with(".patron/") || path == ".patron" {
        return Vec::new();
    }

    let absolute = repo_root.join(path);
    if absolute.is_dir() {
        let mut expanded = Vec::new();
        expand_directory(repo_root, &absolute, &mut expanded);
        expanded.sort();
        expanded.dedup();
        return expanded;
    }

    vec![path.to_string()]
}

fn expand_directory(repo_root: &Path, directory: &Path, output: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name == ".patron")
        {
            continue;
        }
        if path.is_dir() {
            expand_directory(repo_root, &path, output);
            continue;
        }
        if let Ok(relative) = path.strip_prefix(repo_root) {
            let relative = relative.to_string_lossy().replace('\\', "/");
            if !relative.starts_with(".patron/") {
                output.push(relative);
            }
        }
    }
}

fn compose_repo_diff(
    repo_root: &Path,
    changed_files: &[String],
    stat_only: bool,
) -> Result<String, String> {
    let tracked_diff = if stat_only {
        git_output(repo_root, &["diff", "--stat"]).unwrap_or_default()
    } else {
        git_output(repo_root, &["diff", "--"]).unwrap_or_default()
    };
    let tracked_cached_diff = if stat_only {
        git_output(repo_root, &["diff", "--cached", "--stat"]).unwrap_or_default()
    } else {
        git_output(repo_root, &["diff", "--cached", "--"]).unwrap_or_default()
    };

    let mut sections = Vec::new();
    push_diff_section(&mut sections, tracked_diff);
    push_diff_section(&mut sections, tracked_cached_diff);

    for relative_path in changed_files {
        let absolute_path = repo_root.join(relative_path);
        if !absolute_path.is_file() {
            continue;
        }
        let args = if stat_only {
            vec![
                "-C".to_string(),
                repo_root.display().to_string(),
                "diff".to_string(),
                "--no-index".to_string(),
                "--stat".to_string(),
                "--".to_string(),
                "/dev/null".to_string(),
                relative_path.clone(),
            ]
        } else {
            vec![
                "-C".to_string(),
                repo_root.display().to_string(),
                "diff".to_string(),
                "--no-index".to_string(),
                "--".to_string(),
                "/dev/null".to_string(),
                relative_path.clone(),
            ]
        };
        let output = Command::new("git")
            .args(&args)
            .output()
            .map_err(|error| format!("failed to capture diff for {relative_path}: {error}"))?;
        if output.status.code().is_none_or(|code| code > 1) {
            return Err(format!(
                "git diff for {relative_path} failed with status {}",
                output
                    .status
                    .code()
                    .map_or_else(|| "signal".to_string(), |code| code.to_string())
            ));
        }
        let diff = String::from_utf8_lossy(&output.stdout).trim().to_string();
        push_diff_section(&mut sections, diff);
    }

    Ok(sections.join("\n\n"))
}

fn push_diff_section(sections: &mut Vec<String>, diff: String) {
    let trimmed = diff.trim();
    if trimmed.is_empty() {
        return;
    }
    if sections.iter().any(|existing| existing.trim() == trimmed) {
        return;
    }
    sections.push(trimmed.to_string());
}

fn completion_status(completion: &RunnerCompletion) -> &'static str {
    match completion {
        RunnerCompletion::Completed => "completed",
        RunnerCompletion::Failed => "failed",
        RunnerCompletion::Interrupted => "interrupted",
    }
}
