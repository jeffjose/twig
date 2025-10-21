// twig/src/providers/git.rs

use super::{Provider, ProviderError, ProviderResult};
use crate::config::Config;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Command;

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

    /// Check if working directory is dirty (has uncommitted changes)
    fn is_dirty(&self) -> bool {
        Command::new("git")
            .args(&["status", "--porcelain"])
            .output()
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false)
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
}

impl Provider for GitProvider {
    fn name(&self) -> &str {
        "git"
    }

    fn sections(&self) -> Vec<&str> {
        vec!["git"]
    }

    fn collect(&self, config: &Config, validate: bool) -> ProviderResult<HashMap<String, String>> {
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
        if let Some(branch) = self.get_branch() {
            // Determine the variable name to use
            let var_name = config.git
                .as_ref()
                .and_then(|c| c.name.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("git");

            // Primary variable: {git} = branch name
            vars.insert(var_name.to_string(), branch);
        } else {
            // Detached HEAD or error
            let var_name = config.git
                .as_ref()
                .and_then(|c| c.name.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("git");
            vars.insert(var_name.to_string(), "HEAD".to_string());
        }

        // Check config for advanced features (future depth)
        if let Some(_git_config) = config.git.as_ref() {
            // Future: Support for showing dirty status
            // if git_config.show_dirty == Some(true) {
            //     let dirty = self.is_dirty();
            //     vars.insert("git_dirty".to_string(), dirty.to_string());
            // }

            // Future: Support for ahead/behind counts
            // if git_config.show_ahead_behind == Some(true) {
            //     if let Some((ahead, behind)) = self.get_ahead_behind() {
            //         vars.insert("git_ahead".to_string(), ahead.to_string());
            //         vars.insert("git_behind".to_string(), behind.to_string());
            //     }
            // }
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
