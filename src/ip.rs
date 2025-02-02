use crate::variable::{replace_variables, ConfigWithName, LazyVariables, VariableProvider};
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug)]
pub enum IpConfigError {
    Lookup(String),
}

impl std::fmt::Display for IpConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpConfigError::Lookup(e) => write!(f, "Failed to get IP address: {}", e),
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
    "{ip}".to_string() // Default format using {ip} variable
}

fn default_error() -> String {
    String::new()
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

impl LazyVariables for IpProvider {
    type Error = IpConfigError;

    fn get_variable(name: &str) -> Result<String, Self::Error> {
        match name {
            "ip" => {
                let ip = local_ip_address::local_ip()
                    .map_err(|e| IpConfigError::Lookup(e.to_string()))?;
                Ok(ip.to_string())
            }
            _ => Err(IpConfigError::Lookup("Unknown variable".to_string())),
        }
    }

    fn variable_names() -> &'static [&'static str] {
        &["ip"]
    }
}

impl VariableProvider for IpProvider {
    type Error = IpConfigError;
    type Config = Config;

    fn get_value(config: &Self::Config) -> Result<String, Self::Error> {
        // If the format string doesn't contain any variables, return as-is
        if !config.format.contains('{') {
            return Ok(config.format.clone());
        }

        // Get all needed variables using LazyVariables trait
        let vars = Self::get_needed_variables(&config.format)?;
        Ok(replace_variables(&config.format, &vars))
    }

    fn section_name() -> &'static str {
        "ip"
    }
}
