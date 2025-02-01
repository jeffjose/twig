#[cfg(test)]
mod tests {
    use std::env;
    use crate::env_var::{Config, EnvProvider};
    use crate::variable::{VariableProvider, ConfigWithName};

    #[test]
    fn test_env_var_found() {
        // Set up test environment variable
        env::set_var("TEST_VAR", "test_value");

        let config = Config {
            name: "$TEST_VAR".to_string(),
            error: String::new(),
        };

        let result = EnvProvider::get_value(&config).unwrap();
        assert_eq!(result, "test_value");

        // Clean up
        env::remove_var("TEST_VAR");
    }

    #[test]
    fn test_env_var_not_found() {
        let config = Config {
            name: "$NONEXISTENT_VAR".to_string(),
            error: "not_found".to_string(),
        };

        let result = EnvProvider::get_value(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_env_var_without_prefix() {
        // Set up test environment variable
        env::set_var("TEST_VAR2", "test_value2");

        let config = Config {
            name: "TEST_VAR2".to_string(), // No $ prefix
            error: String::new(),
        };

        let result = EnvProvider::get_value(&config).unwrap();
        assert_eq!(result, "test_value2");

        // Clean up
        env::remove_var("TEST_VAR2");
    }

    #[test]
    fn test_env_var_empty() {
        // Set up empty environment variable
        env::set_var("EMPTY_VAR", "");

        let config = Config {
            name: "$EMPTY_VAR".to_string(),
            error: "empty_error".to_string(),
        };

        let result = EnvProvider::get_value(&config).unwrap();
        assert_eq!(result, "");

        // Clean up
        env::remove_var("EMPTY_VAR");
    }

    #[test]
    fn test_section_name() {
        assert_eq!(EnvProvider::section_name(), "env");
    }

    #[test]
    fn test_config_name() {
        let config = Config {
            name: "$TEST".to_string(),
            error: String::new(),
        };
        assert_eq!(config.name(), Some("$TEST"));
    }

    #[test]
    fn test_config_error() {
        let config = Config {
            name: "$TEST".to_string(),
            error: "test_error".to_string(),
        };
        assert_eq!(config.error(), "test_error");
    }
} 
