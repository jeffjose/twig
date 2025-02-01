use serde::Deserialize;
use std::error::Error;
use std::net::IpAddr;
use local_ip_address::local_ip;

#[derive(Debug)]
pub enum IpError {
    Lookup(String),
}

impl std::fmt::Display for IpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpError::Lookup(e) => write!(f, "Failed to get IP address: {}", e),
        }
    }
}

impl Error for IpError {}

#[derive(Deserialize, Default)]
pub struct Config {
    // IP-specific config options will go here
}

pub fn get_ip(_config: &Config) -> Result<IpAddr, IpError> {
    local_ip().map_err(|e| IpError::Lookup(e.to_string()))
} 
