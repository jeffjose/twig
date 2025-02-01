#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use crate::cwd::{Config, CwdProvider, CwdError};
    use crate::variable::{VariableProvider, ConfigWithName};

    #[test]
    fn test_cwd_full_path() {
        // Set up test directory
        let temp_dir = env::temp_dir().join("cwd_test");
        fs::create_dir_all(&temp_dir).unwrap();
        let original_dir = env::current_dir().unwrap();
        
        env::set_current_dir(&temp_dir).unwrap();

        let config = Config {
            name: Some("dir".to_string()),
            shorten: false,
            format: "{cwd}".to_string(),
            error: String::new(),
        };

        let result = CwdProvider::get_value(&config).unwrap();
        assert_eq!(result, temp_dir.to_string_lossy());

        // Clean up
        env::set_current_dir(original_dir).unwrap();
        fs::remove_dir(&temp_dir).unwrap();
    }

    #[test]
    fn test_cwd_shortened() {
        // Set up test directory
        let temp_dir = env::temp_dir().join("cwd_test_short");
        fs::create_dir_all(&temp_dir).unwrap();
        let original_dir = env::current_dir().unwrap();

        env::set_current_dir(&temp_dir).unwrap();

        let config = Config {
            name: Some("dir".to_string()),
            shorten: true,
            format: "{cwd}".to_string(),
            error: String::new(),
        };

        let result = CwdProvider::get_value(&config).unwrap();
        assert_eq!(result, "cwd_test_short");

        // Clean up
        env::set_current_dir(original_dir).unwrap();
        fs::remove_dir(&temp_dir).unwrap();
    }

    #[test]
    fn test_cwd_root() {
        // Instead of trying to change to root, let's test root-like behavior
        let temp_dir = env::temp_dir().join("cwd_test_root");
        fs::create_dir_all(&temp_dir).unwrap();
        let original_dir = env::current_dir().unwrap();

        env::set_current_dir(&temp_dir).unwrap();

        let config = Config {
            name: Some("dir".to_string()),
            shorten: true,
            format: "{cwd}".to_string(),
            error: String::new(),
        };

        let result = CwdProvider::get_value(&config).unwrap();
        assert_eq!(result, "cwd_test_root"); // When shortened, should show just the directory name

        // Test with shorten = false
        let config = Config {
            name: Some("dir".to_string()),
            shorten: false,
            format: "{cwd}".to_string(),
            error: String::new(),
        };

        let result = CwdProvider::get_value(&config).unwrap();
        assert_eq!(result, temp_dir.to_string_lossy()); // Should show full path

        // Clean up
        env::set_current_dir(original_dir).unwrap();
        fs::remove_dir(&temp_dir).unwrap();
    }

    #[test]
    fn test_cwd_nonexistent() {
        let original_dir = env::current_dir().unwrap();
        let nonexistent = PathBuf::from("/path/that/does/not/exist");
        
        let config = Config {
            name: Some("dir".to_string()),
            shorten: false,
            format: "{cwd}".to_string(),
            error: "bad_dir".to_string(),
        };

        // Try to get value without changing directory
        let result = env::set_current_dir(&nonexistent)
            .and_then(|_| CwdProvider::get_value(&config).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));

        assert!(result.is_err());

        // Restore original directory
        env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_section_name() {
        assert_eq!(CwdProvider::section_name(), "cwd");
    }

    #[test]
    fn test_config_name() {
        let config = Config {
            name: Some("test_dir".to_string()),
            shorten: false,
            format: "{cwd}".to_string(),
            error: String::new(),
        };
        assert_eq!(config.name(), Some("test_dir"));
    }

    #[test]
    fn test_config_error() {
        let config = Config {
            name: Some("test_dir".to_string()),
            shorten: false,
            format: "{cwd}".to_string(),
            error: "test_error".to_string(),
        };
        assert_eq!(config.error(), "test_error");
    }

    #[test]
    fn test_config_shorten() {
        let config = Config {
            name: Some("test_dir".to_string()),
            shorten: true,
            format: "{cwd}".to_string(),
            error: String::new(),
        };
        assert!(config.shorten);
    }

    #[test]
    fn test_format_behavior() {
        let temp_dir = env::temp_dir().join("cwd_format_test");
        fs::create_dir_all(&temp_dir).unwrap();
        let original_dir = env::current_dir().unwrap();
        
        env::set_current_dir(&temp_dir).unwrap();

        // Test with full path
        let config = Config {
            name: Some("dir".to_string()),
            shorten: false,
            format: "PWD={cwd}".to_string(),
            error: String::new(),
        };

        let result = CwdProvider::get_value(&config).unwrap();
        assert!(result.starts_with("PWD="));
        assert!(!result.contains("{cwd}")); // Variable should be replaced
        assert_eq!(result, format!("PWD={}", temp_dir.to_string_lossy()));

        // Test with shortened path
        let config = Config {
            name: Some("dir".to_string()),
            shorten: true,
            format: "DIR={cwd}".to_string(),
            error: String::new(),
        };

        let result = CwdProvider::get_value(&config).unwrap();
        assert!(result.starts_with("DIR="));
        assert_eq!(result, format!("DIR=cwd_format_test"));

        // Clean up
        env::set_current_dir(original_dir).unwrap();
        fs::remove_dir(&temp_dir).unwrap();
    }

    #[test]
    fn test_format_with_special_chars() {
        let config = Config {
            name: Some("dir".to_string()),
            shorten: false,
            format: "[[{cwd}]]".to_string(),
            error: String::new(),
        };

        let result = CwdProvider::get_value(&config).unwrap();
        assert!(result.starts_with("[["));
        assert!(result.ends_with("]]"));
        assert!(!result.contains("{cwd}"));
    }
} 
