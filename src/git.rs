use crate::variable::{ConfigWithName, LazyVariables, VariableProvider};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::process::Command;

#[derive(Debug)]
pub enum GitError {
    NotGitRepo,
    CommandFailed(String),
    ParseError(String),
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitError::NotGitRepo => write!(f, "Not a git repository"),
            GitError::CommandFailed(cmd) => write!(f, "Git command failed: {}", cmd),
            GitError::ParseError(msg) => write!(f, "Failed to parse git output: {}", msg),
        }
    }
}

impl Error for GitError {}

#[derive(Deserialize, Serialize, Default)]
pub struct Config {
    pub name: Option<String>,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_error")]
    pub error: String,
}

fn default_format() -> String {
    "{git_branch}{git_state}{git_changes}{git_remote}{git_stash}".to_string()
}

fn default_error() -> String {
    String::new()
}

impl ConfigWithName for Config {
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn error(&self) -> &str {
        &self.error
    }
}

pub struct GitProvider;

impl LazyVariables for GitProvider {
    type Error = GitError;

    fn get_variable(name: &str) -> Result<String, Self::Error> {
        match name {
            "git_branch" => get_branch(),
            "git_state" => get_repo_state(),
            "git_changes" => get_changes_indicator(),
            "git_staged" => get_staged_count().map(|n| n.to_string()),
            "git_unstaged" => get_unstaged_count().map(|n| n.to_string()),
            "git_untracked" => has_untracked().map(|b| if b { " ?" } else { "" }.to_string()),
            "git_remote" => get_remote_status(),
            "git_ahead" => get_ahead_count().map(|n| n.to_string()),
            "git_behind" => get_behind_count().map(|n| n.to_string()),
            "git_stash" => get_stash_indicator(),
            "git_stash_count" => get_stash_count().map(|n| n.to_string()),
            _ => Err(GitError::ParseError(format!("Unknown variable: {}", name))),
        }
    }

    fn variable_names() -> &'static [&'static str] {
        &[
            "git_branch",
            "git_state",
            "git_changes",
            "git_staged",
            "git_unstaged",
            "git_untracked",
            "git_remote",
            "git_ahead",
            "git_behind",
            "git_stash",
            "git_stash_count",
        ]
    }
}

impl VariableProvider for GitProvider {
    type Error = GitError;
    type Config = Config;

    fn get_value(config: &Self::Config) -> Result<String, Self::Error> {
        if !is_git_repo()? {
            return Ok(String::new());
        }

        let vars = Self::get_needed_variables(&config.format)?;
        Ok(crate::variable::replace_variables(&config.format, &vars))
    }

    fn section_name() -> &'static str {
        "git"
    }
}

// Helper function to run git commands
fn run_git(args: &[&str]) -> Result<String, GitError> {
    Command::new("git")
        .args(args)
        .current_dir(std::env::current_dir().map_err(|e| GitError::CommandFailed(e.to_string()))?)
        .output()
        .map_err(|e| GitError::CommandFailed(e.to_string()))
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .map(|s| s.trim().to_string())
                    .map_err(|e| GitError::ParseError(e.to_string()))
            } else {
                Err(GitError::CommandFailed(
                    String::from_utf8_lossy(&output.stderr).to_string(),
                ))
            }
        })
}

fn is_git_repo() -> Result<bool, GitError> {
    match run_git(&["rev-parse", "--is-inside-work-tree"]) {
        Ok(_) => Ok(true),
        Err(GitError::CommandFailed(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

fn get_branch() -> Result<String, GitError> {
    run_git(&["symbolic-ref", "--short", "HEAD"])
        .or_else(|_| run_git(&["rev-parse", "--short", "HEAD"]))
}

fn get_repo_state() -> Result<String, GitError> {
    let git_dir = run_git(&["rev-parse", "--git-dir"])?;
    
    // Check various state files
    let states = [
        ("MERGE_HEAD", "MERGING"),
        ("REBASE_HEAD", "REBASING"),
        ("CHERRY_PICK_HEAD", "CHERRY-PICKING"),
        ("REVERT_HEAD", "REVERTING"),
        ("BISECT_LOG", "BISECTING"),
    ];

    for (file, state) in states {
        if std::path::Path::new(&git_dir).join(file).exists() {
            return Ok(state.to_string());
        }
    }

    Ok(String::new())
}

fn get_changes_indicator() -> Result<String, GitError> {
    let staged = get_staged_count()?;
    let unstaged = get_unstaged_count()?;
    
    let mut indicator = String::new();
    if unstaged > 0 || staged > 0 {
        indicator.push(' ');
        if unstaged > 0 {
            indicator.push('*');
        }
        if staged > 0 {
            indicator.push('+');
            indicator.push_str(&staged.to_string());
        }
    }
    
    Ok(indicator)
}

fn get_staged_count() -> Result<usize, GitError> {
    let status = run_git(&["diff", "--staged", "--numstat"])?;
    Ok(status.lines().count())
}

fn get_unstaged_count() -> Result<usize, GitError> {
    let status = run_git(&["diff", "--numstat"])?;
    Ok(status.lines().count())
}

fn has_untracked() -> Result<bool, GitError> {
    let status = run_git(&["ls-files", "--others", "--exclude-standard"])?;
    if !status.is_empty() {
        Ok(true)
    } else {
        Ok(false)
    }
}

fn get_remote_status() -> Result<String, GitError> {
    // First check if we have an upstream branch
    match run_git(&["rev-parse", "--abbrev-ref", "@{u}"]) {
        Ok(_) => {
            let ahead = get_ahead_count()?;
            let behind = get_behind_count()?;
            
            match (ahead, behind) {
                (0, 0) => Ok(String::new()),
                (a, 0) => Ok(format!(" ↑{}", a)),
                (0, b) => Ok(format!(" ↓{}", b)),
                (a, b) => Ok(format!(" ↕{},{}", a, b)),
            }
        },
        Err(_) => Ok(String::new()) // No upstream branch
    }
}

fn get_ahead_count() -> Result<usize, GitError> {
    match run_git(&["rev-list", "@{u}..HEAD", "--count"]) {
        Ok(count) => count.parse::<usize>()
            .map_err(|e: std::num::ParseIntError| GitError::ParseError(e.to_string())),
        Err(_) => Ok(0)  // Handle case when no upstream exists
    }
}

fn get_behind_count() -> Result<usize, GitError> {
    match run_git(&["rev-list", "HEAD..@{u}", "--count"]) {
        Ok(count) => count.parse::<usize>()
            .map_err(|e: std::num::ParseIntError| GitError::ParseError(e.to_string())),
        Err(_) => Ok(0)  // Handle case when no upstream exists
    }
}

fn get_stash_indicator() -> Result<String, GitError> {
    let count = get_stash_count()?;
    if count > 0 {
        Ok(format!(" ${}", count))
    } else {
        Ok(String::new())
    }
}

fn get_stash_count() -> Result<usize, GitError> {
    let stash = run_git(&["stash", "list"])?;
    Ok(stash.lines().count())
} 
