#[cfg(test)]
mod tests {
    use crate::cwd::{get_cwd_home, get_cwd_parent, Config, CwdProvider};
    use crate::variable::{ConfigWithName, VariableProvider};
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    fn setup_home_dir() -> PathBuf {
        #[cfg(windows)]
        let home = PathBuf::from(r"C:\Users\testuser");
        #[cfg(not(windows))]
        let home = PathBuf::from("/home/testuser");

        env::set_var("HOME", &home);
        home
    }

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

    #[test]
    fn test_cwd_parent() {
        #[cfg(windows)]
        let current_dir = PathBuf::from(r"C:\Users\testuser\projects\rust");
        #[cfg(not(windows))]
        let current_dir = PathBuf::from("/home/testuser/projects/rust");

        let parent = get_cwd_parent(&current_dir);

        #[cfg(windows)]
        assert_eq!(parent, r"C:\Users\testuser\projects");
        #[cfg(not(windows))]
        assert_eq!(parent, "/home/testuser/projects");
    }

    #[test]
    fn test_cwd_home() {
        let _home = setup_home_dir(); // Prefix with underscore since we need the side effect
                                      // Test path inside home directory
        #[cfg(windows)]
        let current_dir = PathBuf::from(r"C:\Users\testuser\projects\rust");
        #[cfg(not(windows))]
        let current_dir = PathBuf::from("/home/testuser/projects/rust");

        let relative_to_home = get_cwd_home(&current_dir);
        assert_eq!(relative_to_home, "~/projects/rust");

        // Test path outside home directory
        #[cfg(windows)]
        let outside_dir = PathBuf::from(r"D:\other\path");
        #[cfg(not(windows))]
        let outside_dir = PathBuf::from("/opt/other/path");

        let outside_home = get_cwd_home(&outside_dir);

        #[cfg(windows)]
        assert_eq!(outside_home, r"D:\other\path");
        #[cfg(not(windows))]
        assert_eq!(outside_home, "/opt/other/path");
    }
}
