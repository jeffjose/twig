use local_ip_address::list_afinet_netifas;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::net::IpAddr;

#[derive(Debug)]
pub enum IpConfigError {
    Lookup(String),
    InterfaceNotFound(String),
}

impl std::fmt::Display for IpConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpConfigError::Lookup(e) => write!(f, "Failed to get IP address: {}", e),
            IpConfigError::InterfaceNotFound(iface) => write!(f, "Interface not found: {}", iface),
        }
    }
}

impl Error for IpConfigError {}

#[derive(Deserialize, Serialize, Default)]
pub struct Config {
    // IP-specific config options will go here
    pub name: Option<String>,
    pub interface: Option<String>,
}

pub fn get_ip(config: &Config) -> Result<IpAddr, IpConfigError> {
    match &config.interface {
        Some(interface) => {
            // Get all network interfaces
            let interfaces =
                list_afinet_netifas().map_err(|e| IpConfigError::Lookup(e.to_string()))?;

            // Find the requested interface
            interfaces
                .iter()
                .find(|(name, _)| name == interface)
                .map(|(_, addr)| *addr)
                .ok_or_else(|| IpConfigError::InterfaceNotFound(interface.clone()))
        }
        None => {
            // Default behavior: get the default local IP
            local_ip_address::local_ip().map_err(|e| IpConfigError::Lookup(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_default_ip_retrieval() {
        let config = Config::default();
        let result = get_ip(&config);
        assert!(result.is_ok());

        let ip = result.unwrap();
        // IP should be either v4 or v6
        match ip {
            IpAddr::V4(_) => (),
            IpAddr::V6(_) => (),
        }

        // Should not be 0.0.0.0 or ::
        assert_ne!(ip, IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
        assert_ne!(ip, IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0)));
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.name, None);
        assert_eq!(config.interface, None);
    }

    #[test]
    fn test_config_with_interface() {
        let config = Config {
            name: Some("local".to_string()),
            interface: Some("eth0".to_string()),
        };
        assert_eq!(config.name, Some("local".to_string()));
        assert_eq!(config.interface, Some("eth0".to_string()));
    }

    #[test]
    fn test_invalid_interface() {
        let config = Config {
            name: None,
            interface: Some("nonexistent0".to_string()),
        };
        let result = get_ip(&config);
        assert!(result.is_err());
        match result.unwrap_err() {
            IpConfigError::InterfaceNotFound(iface) => {
                assert_eq!(iface, "nonexistent0");
            }
            _ => panic!("Expected InterfaceNotFound error"),
        }
    }

    #[test]
    fn test_error_display() {
        let lookup_error = IpConfigError::Lookup("network error".to_string());
        assert_eq!(
            lookup_error.to_string(),
            "Failed to get IP address: network error"
        );

        let interface_error = IpConfigError::InterfaceNotFound("eth99".to_string());
        assert_eq!(interface_error.to_string(), "Interface not found: eth99");
    }

    #[test]
    fn test_list_interfaces() {
        // This test verifies we can list network interfaces
        let interfaces = list_afinet_netifas();
        assert!(interfaces.is_ok());

        let interfaces = interfaces.unwrap();
        assert!(
            !interfaces.is_empty(),
            "System should have at least one network interface"
        );

        // Test with the first available interface
        if let Some((interface_name, _)) = interfaces.iter().next() {
            let config = Config {
                name: None,
                interface: Some(interface_name.clone()),
            };
            let result = get_ip(&config);
            assert!(result.is_ok());
        }
    }
}
