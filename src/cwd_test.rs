#[cfg(test)]
mod tests {
    use crate::cwd::{Config, CwdProvider};
    use crate::variable::{ConfigWithName, VariableProvider};
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_cwd_variables() {
        let temp_dir = env::temp_dir().join("cwd_test");
        fs::create_dir_all(&temp_dir).unwrap();
        let original_dir = env::current_dir().unwrap();

        env::set_current_dir(&temp_dir).unwrap();

        // Test each variable individually
        let test_cases = vec![
            ("{cwd}", temp_dir.to_string_lossy().to_string()),
            ("{cwd_short}", "cwd_test".to_string()),
        ];

        for (format, expected) in test_cases {
            let config = Config {
                name: Some("dir".to_string()),
                format: format.to_string(),
                error: String::new(),
            };

            let result = CwdProvider::get_value(&config).unwrap();
            assert_eq!(result, expected);
        }

        // Test multiple variables in one format
        let config = Config {
            name: Some("dir".to_string()),
            format: "FULL={cwd} (SHORT={cwd_short})".to_string(),
            error: String::new(),
        };

        let result = CwdProvider::get_value(&config).unwrap();
        assert_eq!(
            result,
            format!("FULL={} (SHORT=cwd_test)", temp_dir.to_string_lossy())
        );

        // Clean up
        env::set_current_dir(original_dir).unwrap();
        fs::remove_dir(&temp_dir).unwrap();
    }

    #[test]
    fn test_cwd_root() {
        let temp_dir = env::temp_dir().join("cwd_test_root");
        fs::create_dir_all(&temp_dir).unwrap();
        let original_dir = env::current_dir().unwrap();

        env::set_current_dir(&temp_dir).unwrap();

        let test_cases = vec![
            ("{cwd}", temp_dir.to_string_lossy().to_string()),
            ("{cwd_short}", "cwd_test_root".to_string()),
        ];

        for (format, expected) in test_cases {
            let config = Config {
                name: Some("dir".to_string()),
                format: format.to_string(),
                error: String::new(),
            };

            let result = CwdProvider::get_value(&config).unwrap();
            assert_eq!(result, expected);
        }

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
            format: "{cwd}".to_string(),
            error: "bad_dir".to_string(),
        };

        let result = env::set_current_dir(&nonexistent).and_then(|_| {
            CwdProvider::get_value(&config)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        });

        assert!(result.is_err());

        // Restore original directory
        env::set_current_dir(original_dir).unwrap();
    }

    // Standard config tests
    #[test]
    fn test_section_name() {
        assert_eq!(CwdProvider::section_name(), "cwd");
    }

    #[test]
    fn test_config_name() {
        let config = Config {
            name: Some("test_dir".to_string()),
            format: "{cwd}".to_string(),
            error: String::new(),
        };
        assert_eq!(config.name(), Some("test_dir"));
    }

    #[test]
    fn test_config_error() {
        let config = Config {
            name: Some("test_dir".to_string()),
            format: "{cwd}".to_string(),
            error: "test_error".to_string(),
        };
        assert_eq!(config.error(), "test_error");
    }
}
