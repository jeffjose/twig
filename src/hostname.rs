use crate::variable::{replace_variables, ConfigWithName, LazyVariables, VariableProvider};
use hostname;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::process::Command;

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
    if !config.format.contains('{') {
        return Ok(config.format.clone());
    }

    let vars = HostnameProvider::get_needed_variables(&config.format)?;
    Ok(replace_variables(&config.format, &vars))
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

impl LazyVariables for HostnameProvider {
    type Error = HostnameError;

    fn get_variable(name: &str) -> Result<String, Self::Error> {
        match name {
            "hostname" => hostname::get()
                .map_err(HostnameError::Lookup)
                .map(|h| h.to_string_lossy().into_owned()),
            "fqdn" => {
                let hostname = Self::get_variable("hostname")?;
                Ok(Command::new("hostname")
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
                    .unwrap_or(hostname))
            }
            _ => Err(HostnameError::Lookup(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Unknown variable",
            ))),
        }
    }

    fn variable_names() -> &'static [&'static str] {
        &["hostname", "fqdn"]
    }
}
