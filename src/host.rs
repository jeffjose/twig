use serde::Deserialize;
use std::error::Error;
use std::net::IpAddr;
use hostname::get as get_hostname;
use local_ip_address::local_ip;

#[derive(Debug)]
pub enum HostError {
    HostnameLookup(std::io::Error),
    HostnameInvalid(std::string::FromUtf8Error),
    IpLookup(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::HostnameLookup(e) => write!(f, "Failed to get hostname: {}", e),
            HostError::HostnameInvalid(e) => write!(f, "Invalid hostname: {}", e),
            HostError::IpLookup(e) => write!(f, "Failed to get IP address: {}", e),
        }
    }
}

impl Error for HostError {}

#[derive(Deserialize, Default)]
pub struct HostnameConfig {
    // Hostname-specific config options will go here
}

#[derive(Deserialize, Default)]
pub struct IpConfig {
    // IP-specific config options will go here
}

#[derive(Deserialize, Default)]
pub struct HostConfig {
    #[serde(default)]
    pub hostname: HostnameConfig,
    #[serde(default)]
    pub ip: IpConfig,
}

pub struct HostInfo {
    pub hostname: Option<String>,
    pub ip: Option<IpAddr>,
}

pub fn get_host_info(template: &str) -> Result<HostInfo, HostError> {
    let hostname = if template.contains("{hostname}") {
        Some(get_hostname()
            .map_err(HostError::HostnameLookup)?
            .to_string_lossy()
            .into_owned())
    } else {
        None
    };

    let ip = if template.contains("{ip}") {
        Some(local_ip().map_err(|e| HostError::IpLookup(e.to_string()))?)
    } else {
        None
    };

    Ok(HostInfo { hostname, ip })
} 
