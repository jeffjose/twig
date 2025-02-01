use local_ip_address::{list_afinet_netifas, Error as IpError};
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
            let interfaces = list_afinet_netifas()
                .map_err(|e| IpConfigError::Lookup(e.to_string()))?;

            // Find the requested interface
            interfaces
                .iter()
                .find(|(name, _)| name == interface)
                .map(|(_, addr)| *addr)
                .ok_or_else(|| IpConfigError::InterfaceNotFound(interface.clone()))
        }
        None => {
            // Default behavior: get the default local IP
            local_ip_address::local_ip()
                .map_err(|e| IpConfigError::Lookup(e.to_string()))
        }
    }
} 
