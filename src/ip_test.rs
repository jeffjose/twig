#[cfg(test)]
mod tests {
    use crate::ip::{Config, IpProvider, IpConfigError};
    use crate::variable::{VariableProvider, ConfigWithName};
    use std::net::IpAddr;

    #[test]
    fn test_ip_basic() {
        let config = Config {
            name: Some("local".to_string()),
            interface: None,
            format: "{ip}".to_string(),
            error: String::new(),
        };

        let result = IpProvider::get_value(&config);
        assert!(result.is_ok());
        
        // IP should be parseable and valid
        let ip_str = result.unwrap();
        let ip: IpAddr = ip_str.parse().expect("Should be valid IP address");
        assert!(!ip_str.is_empty());  // Shouldn't be empty
        assert!(!ip_str.contains(' '));  // Shouldn't contain spaces
        assert!(!ip.is_multicast());  // Shouldn't be multicast
        assert!(!ip.is_unspecified());  // Shouldn't be 0.0.0.0
    }

    #[test]
    fn test_ip_with_interface() {
        // Get the first available interface name
        let interfaces = local_ip_address::list_afinet_netifas().unwrap();
        if let Some((interface_name, _)) = interfaces.first() {
            let config = Config {
                name: Some("eth".to_string()),
                interface: Some(interface_name.clone()),
                format: "{ip}".to_string(),
                error: String::new(),
            };

            let result = IpProvider::get_value(&config);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_ip_invalid_interface() {
        let config = Config {
            name: Some("invalid".to_string()),
            interface: Some("nonexistent0".to_string()),
            format: "{ip}".to_string(),
            error: "not_found".to_string(),
        };

        let result = IpProvider::get_value(&config);
        assert!(result.is_err());
        assert!(matches!(result, Err(IpConfigError::InterfaceNotFound(_))));
    }

    #[test]
    fn test_section_name() {
        assert_eq!(IpProvider::section_name(), "ip");
    }

    #[test]
    fn test_config_name() {
        let config = Config {
            name: Some("test_ip".to_string()),
            interface: None,
            format: "{ip}".to_string(),
            error: String::new(),
        };
        assert_eq!(config.name(), Some("test_ip"));
    }

    #[test]
    fn test_config_error() {
        let config = Config {
            name: Some("test_ip".to_string()),
            interface: None,
            format: "{ip}".to_string(),
            error: "test_error".to_string(),
        };
        assert_eq!(config.error(), "test_error");
    }

    #[test]
    fn test_error_display() {
        let error = IpConfigError::Lookup("test error".to_string());
        assert!(error.to_string().contains("test error"));

        let error = IpConfigError::InterfaceNotFound("eth99".to_string());
        assert!(error.to_string().contains("eth99"));
    }

    #[test]
    fn test_ip_format() {
        let config = Config {
            name: Some("ip".to_string()),
            interface: None,
            format: "{ip}".to_string(),
            error: String::new(),
        };

        let result = IpProvider::get_value(&config).unwrap();
        
        // IP address should follow basic rules:
        // - Should be parseable as IP
        let ip: IpAddr = result.parse().expect("Should be valid IP address");
        // - Should be either IPv4 or IPv6
        assert!(result.contains('.') || result.contains(':'));
        // - Should not be empty
        assert!(!result.is_empty());
        // - Should not contain spaces
        assert!(!result.contains(' '));
    }

    #[test]
    fn test_ip_no_name() {
        let config = Config {
            name: None,
            interface: None,
            format: "{ip}".to_string(),
            error: String::new(),
        };

        let result = IpProvider::get_value(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_behavior() {
        let config = Config {
            name: Some("local".to_string()),
            interface: None,
            format: "IP={ip}".to_string(),
            error: String::new(),
        };

        let result = IpProvider::get_value(&config).unwrap();
        assert!(result.starts_with("IP="));
        assert!(!result.contains("{ip}")); // Variable should be replaced
        
        // Get raw IP for comparison
        let raw_ip = local_ip_address::local_ip().unwrap();
        assert_eq!(result, format!("IP={}", raw_ip));
    }

    #[test]
    fn test_format_with_interface() {
        // First get a valid interface name
        let interfaces = local_ip_address::list_afinet_netifas().unwrap();
        if let Some((interface_name, expected_ip)) = interfaces.first() {
            let config = Config {
                name: Some("eth".to_string()),
                interface: Some(interface_name.clone()),
                format: "NET={ip}".to_string(),
                error: String::new(),
            };

            let result = IpProvider::get_value(&config).unwrap();
            assert!(result.starts_with("NET="));
            assert_eq!(result, format!("NET={}", expected_ip));
        }
    }
} 
