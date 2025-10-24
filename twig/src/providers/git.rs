// twig/src/providers/git.rs

use super::{Provider, ProviderError, ProviderResult};
use crate::config::Config;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Command;
use std::time::SystemTime;

pub struct GitProvider;

impl GitProvider {
    pub fn new() -> Self {
        Self
    }

    /// Check if git command is available
    fn git_available(&self) -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Check if current directory is in a git repo
    fn is_git_repo(&self) -> bool {
        Command::new("git")
            .args(&["rev-parse", "--git-dir"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Get current branch name
    fn get_branch(&self) -> Option<String> {
        let output = Command::new("git")
            .args(&["branch", "--show-current"])
            .output()
            .ok()?;

        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if branch.is_empty() {
                None // Detached HEAD state
            } else {
                Some(branch)
            }
        } else {
            None
        }
    }

    /// Get commits ahead/behind remote
    fn get_ahead_behind(&self) -> Option<(u32, u32)> {
        let output = Command::new("git")
            .args(&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
            .output()
            .ok()?;

        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            let parts: Vec<&str> = text.trim().split_whitespace().collect();
            if parts.len() == 2 {
                let ahead = parts[0].parse().ok()?;
                let behind = parts[1].parse().ok()?;
                return Some((ahead, behind));
            }
        }
        None
    }

    /// Get working tree status (staged, modified, and untracked file counts)
    /// Returns (staged_count, unstaged_count)
    /// unstaged_count includes both modified and untracked files
    fn get_status(&self) -> (usize, usize) {
        let output = Command::new("git")
            .args(&["status", "--porcelain"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                let mut staged = 0;
                let mut unstaged = 0;

                for line in text.lines() {
                    if line.is_empty() {
                        continue;
                    }

                    // Git status --porcelain format:
                    // XY filename
                    // X = index status, Y = working tree status
                    let chars: Vec<char> = line.chars().collect();
                    if chars.len() < 2 {
                        continue;
                    }

                    let x = chars[0];
                    let y = chars[1];

                    // Staged files: anything in the index (X is not space, ?, or !)
                    if x != ' ' && x != '?' && x != '!' {
                        staged += 1;
                    }

                    // Unstaged files: modified or untracked (Y is not space)
                    // This includes: modified (M), deleted (D), untracked (?), etc.
                    if y != ' ' {
                        unstaged += 1;
                    }
                }

                return (staged, unstaged);
            }
        }

        (0, 0)
    }

    /// Get elapsed time since last git state change
    /// This checks the timestamp of the last commit
    fn get_elapsed_time(&self) -> Option<String> {
        // Get timestamp of last commit
        let output = Command::new("git")
            .args(&["log", "-1", "--format=%ct"])
            .output()
            .ok()?;

        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            let timestamp: u64 = text.trim().parse().ok()?;

            // Get current time
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()?
                .as_secs();

            let elapsed = now.saturating_sub(timestamp);

            // Format as human-readable
            return Some(Self::format_duration(elapsed));
        }

        None
    }

    /// Format duration in human-readable format (e.g., "2s", "5m", "17h")
    fn format_duration(seconds: u64) -> String {
        if seconds < 60 {
            format!("{}s", seconds)
        } else if seconds < 3600 {
            format!("{}m", seconds / 60)
        } else {
            format!("{}h", seconds / 3600)
        }
    }
}

impl Provider for GitProvider {
    fn name(&self) -> &str {
        "git"
    }

    fn sections(&self) -> Vec<&str> {
        vec!["git"]
    }

    fn collect(&self, _config: &Config, validate: bool) -> ProviderResult<HashMap<String, String>> {
        let mut vars = HashMap::new();

        // Check if git is available
        if !self.git_available() {
            return if validate {
                Err(ProviderError::CommandNotFound(
                    "git command not found".to_string()
                ))
            } else {
                Ok(vars) // Silent failure - return empty vars
            };
        }

        // If not in a git repo, return empty (not an error)
        if !self.is_git_repo() {
            return Ok(vars);
        }

        // Get branch name
        let branch = if let Some(branch) = self.get_branch() {
            branch
        } else {
            "HEAD".to_string() // Detached HEAD
        };

        // Variable: {git_branch} = branch name
        vars.insert("git_branch".to_string(), branch);

        // Get ahead/behind status
        if let Some((ahead, behind)) = self.get_ahead_behind() {
            let tracking = if behind > 0 {
                format!("(behind.{})", behind)
            } else if ahead > 0 {
                format!("(ahead.{})", ahead)
            } else {
                String::new() // Up to date
            };

            if !tracking.is_empty() {
                vars.insert("git_tracking".to_string(), tracking);
            }
        }

        // Get working tree status
        let (staged, unstaged) = self.get_status();

        // Separate clean vs dirty status into different variables
        if staged == 0 && unstaged == 0 {
            // Clean status
            vars.insert("git_status_clean".to_string(), ":✔".to_string());
        } else {
            // Dirty status
            let status = if staged > 0 && unstaged > 0 {
                format!(":+{}+{}", staged, unstaged)
            } else if staged > 0 {
                format!(":+{}", staged)
            } else {
                format!(":+{}", unstaged)
            };
            vars.insert("git_status_dirty".to_string(), status);
        }

        // Get elapsed time
        if let Some(elapsed) = self.get_elapsed_time() {
            vars.insert("git_elapsed".to_string(), format!(":{}", elapsed));
        }

        Ok(vars)
    }

    fn default_config(&self) -> HashMap<String, Value> {
        let mut defaults = HashMap::new();
        // Git section enabled with no special config
        defaults.insert("git".to_string(), json!({}));
        defaults
    }

    fn cacheable(&self) -> bool {
        // Git status changes frequently, don't cache
        false
    }
}
