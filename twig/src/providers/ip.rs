// twig/src/providers/ip.rs

use super::{Provider, ProviderError, ProviderResult};
use crate::config::Config;
use get_if_addrs::{get_if_addrs, IfAddr};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::IpAddr;

pub struct IpProvider;

impl IpProvider {
    pub fn new() -> Self {
        Self
    }

    /// Get all network interfaces
    fn get_interfaces(&self) -> Result<Vec<get_if_addrs::Interface>, String> {
        get_if_addrs().map_err(|e| format!("Failed to get interfaces: {}", e))
    }

    /// Filter out loopback and link-local addresses
    fn filter_interfaces(&self, interfaces: Vec<get_if_addrs::Interface>) -> Vec<get_if_addrs::Interface> {
        interfaces
            .into_iter()
            .filter(|iface| {
                // Skip loopback interfaces
                if iface.is_loopback() {
                    return false;
                }

                // Skip interfaces without IP addresses
                let addr = match &iface.addr {
                    IfAddr::V4(v4) => IpAddr::V4(v4.ip),
                    IfAddr::V6(v6) => IpAddr::V6(v6.ip),
                };

                // Skip link-local IPv6 addresses (fe80::)
                if let IpAddr::V6(v6) = addr {
                    if v6.segments()[0] == 0xfe80 {
                        return false;
                    }
                }

                true
            })
            .collect()
    }

    /// Select interface based on config
    /// If interface name specified in config, find that interface
    /// Otherwise, return first non-loopback interface
    fn select_interface(
        &self,
        interfaces: Vec<get_if_addrs::Interface>,
        config_interface: Option<&str>,
    ) -> Option<get_if_addrs::Interface> {
        if let Some(name) = config_interface {
            // Find specific interface by name
            interfaces.into_iter().find(|iface| iface.name == name)
        } else {
            // Return first interface (already filtered)
            interfaces.into_iter().next()
        }
    }

    /// Get IP address from interface
    /// Returns (address, version) where version is 4 or 6
    fn get_ip_address(
        &self,
        interface: &get_if_addrs::Interface,
        _prefer_ipv6: bool,
    ) -> Option<(IpAddr, u8)> {
        let addr = match &interface.addr {
            IfAddr::V4(v4) => IpAddr::V4(v4.ip),
            IfAddr::V6(v6) => IpAddr::V6(v6.ip),
        };

        let version = match addr {
            IpAddr::V4(_) => 4,
            IpAddr::V6(_) => 6,
        };

        // For now, just return what we have
        // In the future, we could scan all addresses on the interface
        // and prefer IPv6 or IPv4 based on config
        Some((addr, version))
    }
}

impl Provider for IpProvider {
    fn name(&self) -> &str {
        "ip"
    }

    fn sections(&self) -> Vec<&str> {
        vec!["ip"]
    }

    fn collect(&self, config: &Config, validate: bool) -> ProviderResult<HashMap<String, String>> {
        let mut vars = HashMap::new();

        // Read config
        let ip_config = config.ip.as_ref();
        let interface_name = ip_config
            .and_then(|c| c.interface.as_deref());
        let prefer_ipv6 = ip_config
            .map(|c| c.prefer_ipv6)
            .unwrap_or(false);

        // Get interfaces
        let interfaces = match self.get_interfaces() {
            Ok(ifaces) => ifaces,
            Err(_) => {
                return if validate {
                    Err(ProviderError::ResourceNotAvailable(
                        "Failed to get network interfaces".to_string(),
                    ))
                } else {
                    Ok(vars) // Silent failure - return empty vars
                };
            }
        };

        // Filter interfaces
        let filtered = self.filter_interfaces(interfaces);

        // Select interface
        if let Some(iface) = self.select_interface(filtered, interface_name) {
            vars.insert("ip_interface".to_string(), iface.name.clone());

            if let Some((addr, version)) = self.get_ip_address(&iface, prefer_ipv6) {
                vars.insert("ip_address".to_string(), addr.to_string());
                vars.insert("ip_version".to_string(), version.to_string());
            }
        }

        Ok(vars)
    }

    fn default_config(&self) -> HashMap<String, Value> {
        let mut defaults = HashMap::new();
        defaults.insert(
            "ip".to_string(),
            json!({
                "prefer_ipv6": false
            }),
        );
        defaults
    }

    fn cacheable(&self) -> bool {
        // IP addresses can change, but slowly
        // Cache for 30 seconds
        true
    }

    fn cache_duration(&self) -> u64 {
        // Cache for 30 seconds (IPs don't change that often)
        30
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_provider_creation() {
        let provider = IpProvider::new();
        assert_eq!(provider.name(), "ip");
        assert_eq!(provider.sections(), vec!["ip"]);
        assert!(provider.cacheable());
        assert_eq!(provider.cache_duration(), 30);
    }

    #[test]
    fn test_filter_loopback() {
        let provider = IpProvider::new();

        // Get real interfaces
        let interfaces = provider.get_interfaces();

        if let Ok(ifaces) = interfaces {
            let filtered = provider.filter_interfaces(ifaces);

            // Verify no loopback interfaces remain
            for iface in &filtered {
                assert!(!iface.is_loopback(), "Found loopback interface: {}", iface.name);
            }

            // Verify no link-local IPv6 addresses remain
            for iface in &filtered {
                let addr = match &iface.addr {
                    IfAddr::V4(v4) => IpAddr::V4(v4.ip),
                    IfAddr::V6(v6) => IpAddr::V6(v6.ip),
                };

                if let IpAddr::V6(v6) = addr {
                    assert_ne!(v6.segments()[0], 0xfe80, "Found link-local IPv6: {}", v6);
                }
            }
        }
    }

    #[test]
    fn test_default_config() {
        let provider = IpProvider::new();
        let defaults = provider.default_config();

        assert!(defaults.contains_key("ip"));
        assert_eq!(defaults["ip"]["prefer_ipv6"], false);
    }
}
