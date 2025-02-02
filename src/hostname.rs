use hostname;
use serde::{Deserialize, Serialize};
use std::error::Error;
use crate::variable::{ConfigWithName, VariableProvider};
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug)]
pub enum HostnameError {
    Lookup(std::io::Error),
    DnsLookup(std::io::Error),
    CommandFailed(std::io::Error),
}

impl std::fmt::Display for HostnameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostnameError::Lookup(e) => write!(f, "Failed to get hostname: {}", e),
            HostnameError::DnsLookup(e) => write!(f, "Failed to get FQDN: {}", e),
            HostnameError::CommandFailed(e) => write!(f, "Hostname command failed: {}", e),
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

pub fn get_hostname_variables() -> Result<HashMap<String, String>, HostnameError> {
    let mut vars = HashMap::new();

    // Get basic hostname
    let hostname = hostname::get()
        .map_err(HostnameError::Lookup)?
        .to_string_lossy()
        .into_owned();
    vars.insert("hostname".to_string(), hostname.clone());

    // Get FQDN using hostname -f command
    let fqdn = Command::new("hostname")
        .arg("-f")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| hostname.clone());
    vars.insert("fqdn".to_string(), fqdn);

    Ok(vars)
}

pub fn get_hostname(config: &Config) -> Result<String, HostnameError> {
    let vars = get_hostname_variables()?;
    
    // Replace all variables in the format string
    let mut result = config.format.clone();
    for (var_name, value) in vars {
        result = result.replace(&format!("{{{}}}", var_name), &value);
    }
    
    Ok(result)
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
