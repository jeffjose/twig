#[cfg(test)]
mod tests {
    use crate::hostname::{Config, HostnameProvider, HostnameError};
    use crate::variable::{VariableProvider, ConfigWithName};

    #[test]
    fn test_hostname_basic() {
        let config = Config {
            name: Some("host".to_string()),
            error: String::new(),
        };

        let result = HostnameProvider::get_value(&config);
        assert!(result.is_ok());
        
        // Hostname should not be empty
        let hostname = result.unwrap();
        assert!(!hostname.is_empty());
    }

    #[test]
    fn test_hostname_error_handling() {
        let config = Config {
            name: Some("host".to_string()),
            error: "hostname_error".to_string(),
        };

        // We can't easily force a hostname error, but we can verify error string
        if let Err(err) = HostnameProvider::get_value(&config) {
            assert!(matches!(err, HostnameError::Lookup(_)));
        }
    }

    #[test]
    fn test_section_name() {
        assert_eq!(HostnameProvider::section_name(), "hostname");
    }

    #[test]
    fn test_config_name() {
        let config = Config {
            name: Some("test_host".to_string()),
            error: String::new(),
        };
        assert_eq!(config.name(), Some("test_host"));
    }

    #[test]
    fn test_config_error() {
        let config = Config {
            name: Some("test_host".to_string()),
            error: "test_error".to_string(),
        };
        assert_eq!(config.error(), "test_error");
    }

    #[test]
    fn test_hostname_format() {
        let config = Config {
            name: Some("host".to_string()),
            error: String::new(),
        };

        let result = HostnameProvider::get_value(&config).unwrap();
        
        // Hostname should follow some basic rules:
        // - Should not contain spaces
        assert!(!result.contains(' '));
        // - Should not be empty
        assert!(!result.is_empty());
        // - Should not be too long (max 255 chars by standard)
        assert!(result.len() <= 255);
        // - Should only contain valid hostname characters
        assert!(result.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.'));
    }

    #[test]
    fn test_hostname_no_name() {
        let config = Config {
            name: None,
            error: String::new(),
        };

        let result = HostnameProvider::get_value(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_display() {
        use std::io;
        let error = HostnameError::Lookup(io::Error::new(io::ErrorKind::Other, "test error"));
        assert!(error.to_string().contains("test error"));
    }

    #[test]
    fn test_hostname_default_format() {
        let config = Config {
            name: Some("host".to_string()),
            format: "{hostname}".to_string(),
            error: String::new(),
        };

        let result = HostnameProvider::get_value(&config).unwrap();
        // Should be just the hostname without any formatting
        assert!(!result.is_empty());
        assert!(!result.contains("{hostname}"));
    }

    #[test]
    fn test_hostname_custom_format() {
        let config = Config {
            name: Some("host".to_string()),
            format: "HOST={hostname}".to_string(),
            error: String::new(),
        };

        let result = HostnameProvider::get_value(&config).unwrap();
        assert!(result.starts_with("HOST="));
        assert!(!result.contains("{hostname}"));
    }

    #[test]
    fn test_hostname_bracketed_format() {
        let config = Config {
            name: Some("host".to_string()),
            format: "[{hostname}]".to_string(),
            error: String::new(),
        };

        let result = HostnameProvider::get_value(&config).unwrap();
        assert!(result.starts_with("["));
        assert!(result.ends_with("]"));
        assert!(!result.contains("{hostname}"));
    }

    #[test]
    fn test_hostname_format_multiple_replacements() {
        let config = Config {
            name: Some("host".to_string()),
            format: "{hostname} - {hostname}".to_string(),
            error: String::new(),
        };

        let result = HostnameProvider::get_value(&config).unwrap();
        let parts: Vec<&str> = result.split(" - ").collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], parts[1]); // Both replacements should be identical
    }

    #[test]
    fn test_hostname_empty_format() {
        let config = Config {
            name: Some("host".to_string()),
            format: String::new(),
            error: String::new(),
        };

        let result = HostnameProvider::get_value(&config).unwrap();
        assert!(result.is_empty());
    }
} 
