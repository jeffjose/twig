use serde::Deserialize;
use std::error::Error;
use hostname;

#[derive(Debug)]
pub enum HostnameError {
    Lookup(std::io::Error),
    Invalid(std::string::FromUtf8Error),
}

impl std::fmt::Display for HostnameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostnameError::Lookup(e) => write!(f, "Failed to get hostname: {}", e),
            HostnameError::Invalid(e) => write!(f, "Invalid hostname: {}", e),
        }
    }
}

impl Error for HostnameError {}

#[derive(Deserialize, Default)]
pub struct Config {
    // Hostname-specific config options will go here
}

pub fn get_hostname(_config: &Config) -> Result<String, HostnameError> {
    hostname::get()
        .map_err(HostnameError::Lookup)
        .map(|os_string| os_string.to_string_lossy().into_owned())
} 
