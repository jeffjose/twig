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

    /// Check if in a git repository
    fn is_git_repo(&self) -> bool {
        Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Batch query git status - gets branch, upstream, ahead/behind, and file status in ONE command
    /// Returns (branch, upstream, ahead, behind, staged_count, unstaged_count)
    fn get_git_status_batch(&self) -> Option<(String, Option<String>, u32, u32, usize, usize)> {
        let output = Command::new("git")
            .args(["status", "--porcelain=v2", "--branch"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let text = String::from_utf8_lossy(&output.stdout);

        let mut branch = String::from("HEAD"); // Default for detached HEAD
        let mut upstream: Option<String> = None;
        let mut ahead: u32 = 0;
        let mut behind: u32 = 0;
        let mut staged: usize = 0;
        let mut unstaged: usize = 0;

        for line in text.lines() {
            if line.starts_with("# branch.head ") {
                // Branch name
                branch = line.strip_prefix("# branch.head ")?.to_string();
            } else if line.starts_with("# branch.upstream ") {
                // Upstream branch
                upstream = Some(line.strip_prefix("# branch.upstream ")?.to_string());
            } else if line.starts_with("# branch.ab ") {
                // Ahead/behind: "# branch.ab +2 -1" means ahead 2, behind 1
                let ab = line.strip_prefix("# branch.ab ")?;
                let parts: Vec<&str> = ab.split_whitespace().collect();
                if parts.len() == 2 {
                    ahead = parts[0].trim_start_matches('+').parse().ok()?;
                    behind = parts[1].trim_start_matches('-').parse().ok()?;
                }
            } else if line.starts_with("1 ") || line.starts_with("2 ") {
                // Staged files (ordinary changed entries or rename/copy entries)
                staged += 1;
            } else if line.starts_with("? ") {
                // Untracked files
                unstaged += 1;
            } else if line.starts_with("u ") {
                // Unmerged files (conflicts) - count as unstaged
                unstaged += 1;
            }
        }

        Some((branch, upstream, ahead, behind, staged, unstaged))
    }

    /// Get elapsed time since last git state change
    /// This checks the timestamp of the last commit
    fn get_elapsed_time(&self) -> Option<String> {
        // Get timestamp of last commit
        let output = Command::new("git")
            .args(["log", "-1", "--format=%ct"])
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

        // Check if in a git repo
        if !self.is_git_repo() {
            return Ok(vars);
        }

        // Query git using batched command (gets everything in one call)
        let (branch, _upstream, ahead, behind, staged, unstaged) =
            match self.get_git_status_batch() {
                Some(result) => result,
                None => return Ok(vars), // Failed to get status
            };

        // Build variables from batched result
        vars.insert("git_branch".to_string(), branch);

        // Tracking status
        let tracking = if behind > 0 {
            format!("(behind.{})", behind)
        } else if ahead > 0 {
            format!("(ahead.{})", ahead)
        } else {
            String::new()
        };

        if !tracking.is_empty() {
            vars.insert("git_tracking".to_string(), tracking);
        }

        // File status
        if staged == 0 && unstaged == 0 {
            vars.insert("git_status_clean".to_string(), ":✔".to_string());
        } else {
            if staged > 0 {
                vars.insert("git_status_staged".to_string(), format!(":+{}", staged));
            }
            if unstaged > 0 {
                vars.insert("git_status_unstaged".to_string(), format!(":+{}", unstaged));
            }
        }

        // Elapsed time
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(GitProvider::format_duration(5), "5s");
        assert_eq!(GitProvider::format_duration(59), "59s");
        assert_eq!(GitProvider::format_duration(60), "1m");
        assert_eq!(GitProvider::format_duration(90), "1m");
        assert_eq!(GitProvider::format_duration(3599), "59m");
        assert_eq!(GitProvider::format_duration(3600), "1h");
        assert_eq!(GitProvider::format_duration(7200), "2h");
    }

    #[test]
    fn test_git_provider_creation() {
        let provider = GitProvider::new();
        assert_eq!(provider.name(), "git");
        assert_eq!(provider.sections(), vec!["git"]);
        assert!(!provider.cacheable());
    }
}
