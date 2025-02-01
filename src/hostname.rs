use hostname;
use serde::{Deserialize, Serialize};
use std::error::Error;
use crate::variable::{ConfigWithName, VariableProvider};

#[derive(Debug)]
pub enum HostnameError {
    Lookup(std::io::Error),
}

impl std::fmt::Display for HostnameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostnameError::Lookup(e) => write!(f, "Failed to get hostname: {}", e),
        }
    }
}

impl Error for HostnameError {}

#[derive(Deserialize, Serialize, Default)]
pub struct Config {
    // Hostname-specific config options will go here
    pub name: Option<String>,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_error")]
    pub error: String,
}

fn default_format() -> String {
    "{hostname}".to_string()
}

fn default_error() -> String {
    String::new()
}

pub fn get_hostname(config: &Config) -> Result<String, HostnameError> {
    // First get the raw hostname
    let raw_hostname = hostname::get()
        .map_err(HostnameError::Lookup)
        .map(|os_string| os_string.to_string_lossy().into_owned())?;
    
    // Replace {hostname} in the format string with the actual hostname
    Ok(config.format.replace("{hostname}", &raw_hostname))
}

impl ConfigWithName for Config {
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn error(&self) -> &str {
        &self.error
    }
}

pub struct HostnameProvider;

impl VariableProvider for HostnameProvider {
    type Error = HostnameError;
    type Config = Config;

    fn get_value(config: &Self::Config) -> Result<String, Self::Error> {
        get_hostname(config)
    }

    fn section_name() -> &'static str {
        "hostname"
    }
}
