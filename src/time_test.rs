#[cfg(test)]
mod tests {
    use crate::time::{Config, TimeProvider};
    use crate::variable::{ConfigWithName, VariableProvider};
    use chrono::Local;

    #[test]
    fn test_time_basic() {
        let config = Config {
            name: Some("time".to_string()),
            format: "%H:%M:%S".to_string(),
            error: String::new(),
        };

        let result = TimeProvider::get_value(&config);
        assert!(result.is_ok());

        let time_str = result.unwrap();
        // Should match HH:MM:SS format (e.g., "14:30:45")
        assert!(time_str.len() == 8);
        assert!(time_str.chars().nth(2) == Some(':'));
        assert!(time_str.chars().nth(5) == Some(':'));
    }

    #[test]
    fn test_time_utc() {
        let config = Config {
            name: Some("utc".to_string()),
            format: "%H:%M:%S UTC".to_string(),
            error: String::new(),
        };

        let result = TimeProvider::get_value(&config).unwrap();
        assert!(result.ends_with("UTC"));
    }

    #[test]
    fn test_time_custom_format() {
        let config = Config {
            name: Some("date".to_string()),
            format: "%Y-%m-%d".to_string(),
            error: String::new(),
        };

        let result = TimeProvider::get_value(&config).unwrap();

        // Should match YYYY-MM-DD format
        assert_eq!(result.len(), 10);
        assert!(result.chars().nth(4) == Some('-'));
        assert!(result.chars().nth(7) == Some('-'));

        // Year should be current year
        let current_year = Local::now().format("%Y").to_string();
        assert!(result.starts_with(&current_year));
    }

    #[test]
    fn test_section_name() {
        assert_eq!(TimeProvider::section_name(), "time");
    }

    #[test]
    fn test_config_name() {
        let config = Config {
            name: Some("test_time".to_string()),
            format: "%H:%M:%S".to_string(),
            error: String::new(),
        };
        assert_eq!(config.name(), Some("test_time"));
    }

    #[test]
    fn test_config_error() {
        let config = Config {
            name: Some("test_time".to_string()),
            format: "%H:%M:%S".to_string(),
            error: "test_error".to_string(),
        };
        assert_eq!(config.error(), "test_error");
    }

    #[test]
    fn test_time_12hour() {
        let config = Config {
            name: Some("time12".to_string()),
            format: "%I:%M:%S %p".to_string(),
            error: String::new(),
        };

        let result = TimeProvider::get_value(&config).unwrap();

        // Should match HH:MM:SS AM/PM format
        assert!(result.ends_with("AM") || result.ends_with("PM"));
        assert_eq!(result.len(), 11); // "HH:MM:SS AM" or "HH:MM:SS PM"
    }

    #[test]
    fn test_time_with_timezone() {
        let config = Config {
            name: Some("timezone".to_string()),
            format: "%H:%M:%S %Z".to_string(),
            error: String::new(),
        };

        let result = TimeProvider::get_value(&config).unwrap();
        // Should have timezone at the end
        assert!(result.len() > 8); // More than just HH:MM:SS
    }

    #[test]
    fn test_time_no_name() {
        let config = Config {
            name: None,
            format: "%H:%M:%S".to_string(),
            error: String::new(),
        };

        let result = TimeProvider::get_value(&config);
        assert!(result.is_ok());
    }
}
