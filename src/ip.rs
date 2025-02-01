use local_ip_address::list_afinet_netifas;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::net::IpAddr;
use crate::variable::{ConfigWithName, VariableProvider};

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
    pub name: Option<String>,
    pub interface: Option<String>,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_error")]
    pub error: String,
}

fn default_format() -> String {
    "{ip}".to_string()  // Default format using {ip} variable
}

fn default_error() -> String {
    String::new()
}

pub fn get_ip(config: &Config) -> Result<IpAddr, IpConfigError> {
    let raw_ip = match &config.interface {
        Some(interface) => {
            // Get all network interfaces
            let interfaces = list_afinet_netifas()
                .map_err(|e| IpConfigError::Lookup(e.to_string()))?;

            // Find the requested interface
            interfaces
                .iter()
                .find(|(name, _)| name == interface)
                .map(|(_, addr)| *addr)
                .ok_or_else(|| IpConfigError::InterfaceNotFound(interface.clone()))?
        }
        None => {
            // Default behavior: get the default local IP
            local_ip_address::local_ip()
                .map_err(|e| IpConfigError::Lookup(e.to_string()))?
        }
    };

    // Format the IP using the format string
    Ok(raw_ip)
}

impl ConfigWithName for Config {
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn error(&self) -> &str {
        &self.error
    }
}

pub struct IpProvider;

impl VariableProvider for IpProvider {
    type Error = IpConfigError;
    type Config = Config;

    fn get_value(config: &Self::Config) -> Result<String, Self::Error> {
        let ip = get_ip(config)?;
        Ok(config.format.replace("{ip}", &ip.to_string()))
    }

    fn section_name() -> &'static str {
        "ip"
    }
} 
