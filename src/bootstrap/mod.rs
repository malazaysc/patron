use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{app::RuntimePaths, db};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepoContext {
    pub cwd: PathBuf,
    pub repo_root: PathBuf,
    pub repo_name: String,
    pub git_branch: Option<String>,
    pub is_git_repo: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequirementCheck {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootstrapStatus {
    pub repo: RepoContext,
    pub runtime_root: PathBuf,
    pub runtime_exists: bool,
    pub state_db_exists: bool,
    pub initialized: bool,
    pub requirements: Vec<RequirementCheck>,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
}

impl BootstrapStatus {
    pub fn setup_ready(&self) -> bool {
        self.initialized && self.blockers.is_empty()
    }

    pub fn summary(&self) -> String {
        let mut summary = String::new();
        let _ = writeln!(&mut summary, "repo: {}", self.repo.repo_root.display());
        let _ = writeln!(&mut summary, "runtime: {}", self.runtime_root.display());
        let _ = writeln!(
            &mut summary,
            "initialized: {}",
            if self.initialized { "yes" } else { "no" }
        );
        if !self.blockers.is_empty() {
            let _ = writeln!(&mut summary, "blockers:");
            for blocker in &self.blockers {
                let _ = writeln!(&mut summary, "- {blocker}");
            }
        }
        if !self.warnings.is_empty() {
            let _ = writeln!(&mut summary, "warnings:");
            for warning in &self.warnings {
                let _ = writeln!(&mut summary, "- {warning}");
            }
        }
        summary
    }
}

pub fn detect_repo_context(cwd: &Path) -> RepoContext {
    let repo_root = find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let repo_name = repo_root
        .file_name()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "unknown-repo".to_string());
    let is_git_repo =
        repo_root.join(".git").exists() || repo_root != cwd || cwd.join(".git").exists();
    let git_branch = if is_git_repo {
        command_output_trimmed(
            Command::new("git")
                .arg("-C")
                .arg(&repo_root)
                .arg("rev-parse")
                .arg("--abbrev-ref")
                .arg("HEAD"),
        )
    } else {
        None
    };

    RepoContext {
        cwd: cwd.to_path_buf(),
        repo_root,
        repo_name,
        git_branch,
        is_git_repo,
    }
}

pub fn inspect(runtime: &RuntimePaths, repo: RepoContext) -> BootstrapStatus {
    let runtime_exists = runtime.root.exists();
    let state_db_exists = runtime.state_db.exists();
    let initialized = runtime_exists && state_db_exists;

    let requirements = vec![
        RequirementCheck {
            name: "git repository",
            ok: repo.is_git_repo,
            detail: if repo.is_git_repo {
                format!("detected {}", repo.repo_root.display())
            } else {
                "current directory is not inside a git repository".into()
            },
        },
        RequirementCheck {
            name: "git",
            ok: command_succeeds(Command::new("git").arg("--version")),
            detail: "required for repository context and future branch-aware execution".into(),
        },
        RequirementCheck {
            name: "npx",
            ok: command_succeeds(Command::new("npx").arg("--version")),
            detail: "required for Playwright-backed QA capture".into(),
        },
    ];

    let mut blockers = Vec::new();
    let mut warnings = Vec::new();

    if !repo.is_git_repo {
        blockers.push(
            "Patron must run from a git repository root or a child directory inside a git repository."
                .into(),
        );
    }
    if !initialized {
        blockers.push("Patron runtime is not initialized. Run `cargo run -- init`.".into());
    }
    for requirement in &requirements {
        if !requirement.ok {
            if requirement.name == "npx" {
                warnings.push(
                    "Playwright QA prerequisites are missing. QA evidence capture will fail until `npx` is available."
                        .into(),
                );
            } else {
                blockers.push(format!(
                    "Missing required dependency `{}`. {}",
                    requirement.name, requirement.detail
                ));
            }
        }
    }

    BootstrapStatus {
        repo,
        runtime_root: runtime.root.clone(),
        runtime_exists,
        state_db_exists,
        initialized,
        requirements,
        blockers,
        warnings,
    }
}

pub fn initialize_runtime(
    runtime: &RuntimePaths,
    repo: &RepoContext,
) -> Result<BootstrapStatus, String> {
    runtime
        .ensure_layout()
        .map_err(|error| format!("failed to initialize Patron runtime layout: {error}"))?;
    db::initialize(runtime)
        .map_err(|error| format!("failed to initialize Patron sqlite state: {error}"))?;
    db::persist_repo_metadata(runtime, repo)
        .map_err(|error| format!("failed to persist repository metadata: {error}"))?;
    Ok(inspect(runtime, repo.clone()))
}

fn find_git_root(start: &Path) -> Option<PathBuf> {
    for candidate in start.ancestors() {
        if candidate.join(".git").exists() {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

fn command_succeeds(command: &mut Command) -> bool {
    command
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn command_output_trimmed(command: &mut Command) -> Option<String> {
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

pub fn current_dir() -> io::Result<PathBuf> {
    std::env::current_dir()
}
