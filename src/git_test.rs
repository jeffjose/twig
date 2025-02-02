#[cfg(test)]
mod tests {
    use super::super::git::{Config, GitProvider};
    use crate::variable::VariableProvider;
    use std::env;
    use std::fs;
    use std::process::Command;

    fn setup_test_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let original_dir = env::current_dir().unwrap();

        env::set_current_dir(&dir).unwrap();

        // Initialize git repo
        Command::new("git").args(&["init"]).output().unwrap();
        Command::new("git")
            .args(&["config", "user.name", "test"])
            .output()
            .unwrap();
        Command::new("git")
            .args(&["config", "user.email", "test@example.com"])
            .output()
            .unwrap();

        // Create and commit a test file
        fs::write("test.txt", "test").unwrap();
        Command::new("git")
            .args(&["add", "test.txt"])
            .output()
            .unwrap();
        Command::new("git")
            .args(&["commit", "-m", "Initial commit"])
            .output()
            .unwrap();

        dir
    }

    #[test]
    fn test_git_basic() {
        let temp_dir = setup_test_repo();

        let config = Config {
            name: Some("status".to_string()),
            format: "{git_branch}".to_string(),
            error: String::new(),
        };

        let result = GitProvider::get_value(&config).unwrap();
        assert_eq!(result, "");

        env::set_current_dir(temp_dir.path().parent().unwrap()).unwrap();
    }

    #[test]
    fn test_git_changes() {
        let temp_dir = setup_test_repo();

        // Create an unstaged change
        fs::write("test.txt", "modified").unwrap();

        let config = Config {
            name: Some("status".to_string()),
            format: "{git_changes}".to_string(),
            error: String::new(),
        };

        let result = GitProvider::get_value(&config).unwrap();
        assert_eq!(result, "");

        env::set_current_dir(temp_dir.path().parent().unwrap()).unwrap();
    }

    #[test]
    fn test_not_git_repo() {
        let temp_dir = tempfile::tempdir().unwrap();
        env::set_current_dir(&temp_dir).unwrap();

        let config = Config {
            name: Some("status".to_string()),
            format: "{git_branch}".to_string(),
            error: String::new(),
        };

        let result = GitProvider::get_value(&config).unwrap();
        assert_eq!(result, "");

        env::set_current_dir(temp_dir.path().parent().unwrap()).unwrap();
    }

    #[test]
    fn test_git_staged_changes() {
        let temp_dir = setup_test_repo();

        // Create a staged change
        fs::write("test.txt", "modified").unwrap();
        Command::new("git")
            .args(&["add", "test.txt"])
            .output()
            .unwrap();

        let config = Config {
            name: Some("status".to_string()),
            format: "{git_changes}".to_string(),
            error: String::new(),
        };

        let result = GitProvider::get_value(&config).unwrap();
        assert_eq!(result, "");

        env::set_current_dir(temp_dir.path().parent().unwrap()).unwrap();
    }

    #[test]
    fn test_git_untracked() {
        let temp_dir = setup_test_repo();

        // Create an untracked file
        fs::write("untracked.txt", "new file").unwrap();

        let config = Config {
            name: Some("status".to_string()),
            format: "{git_untracked}".to_string(),
            error: String::new(),
        };

        let result = GitProvider::get_value(&config).unwrap();
        assert_eq!(result, "");

        env::set_current_dir(temp_dir.path().parent().unwrap()).unwrap();
    }

    #[test]
    fn test_git_stash() {
        let temp_dir = setup_test_repo();

        // Create and stash a change
        fs::write("test.txt", "modified").unwrap();
        Command::new("git").args(&["stash"]).output().unwrap();

        let config = Config {
            name: Some("status".to_string()),
            format: "{git_stash}".to_string(),
            error: String::new(),
        };

        let result = GitProvider::get_value(&config).unwrap();
        assert_eq!(result, "");

        env::set_current_dir(temp_dir.path().parent().unwrap()).unwrap();
    }

    #[test]
    fn test_git_combined_status() {
        let temp_dir = setup_test_repo();

        // Create various states
        fs::write("test.txt", "modified").unwrap(); // unstaged change
        fs::write("new.txt", "new file").unwrap(); // untracked file
        Command::new("git")
            .args(&["add", "test.txt"])
            .output()
            .unwrap();

        let config = Config {
            name: Some("status".to_string()),
            format: "{git_branch}{git_changes}{git_untracked}".to_string(),
            error: String::new(),
        };

        let result = GitProvider::get_value(&config).unwrap();
        assert_eq!(result, "");

        env::set_current_dir(temp_dir.path().parent().unwrap()).unwrap();
    }

    // Add more tests for other git states...
}
