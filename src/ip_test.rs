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
            error: String::new(),
        };

        let result = IpProvider::get_value(&config);
        assert!(result.is_ok());
        
        // IP should be parseable and valid
        let ip_str = result.unwrap();
        let ip: IpAddr = ip_str.parse().expect("Should be valid IP address");
        assert!(!ip.is_unspecified());  // Shouldn't be 0.0.0.0
        assert!(!ip.is_multicast());    // Shouldn't be multicast
    }

    #[test]
    fn test_ip_with_interface() {
        // Get the first available interface name
        let interfaces = local_ip_address::list_afinet_netifas().unwrap();
        if let Some((interface_name, _)) = interfaces.first() {
            let config = Config {
                name: Some("eth".to_string()),
                interface: Some(interface_name.clone()),
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
            error: String::new(),
        };
        assert_eq!(config.name(), Some("test_ip"));
    }

    #[test]
    fn test_config_error() {
        let config = Config {
            name: Some("test_ip".to_string()),
            interface: None,
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
            error: String::new(),
        };

        let result = IpProvider::get_value(&config);
        assert!(result.is_ok());
    }
} 
