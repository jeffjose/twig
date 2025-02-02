use chrono::{format::ParseError, Local};
use serde::{Deserialize, Serialize};
use crate::variable::{ConfigWithName, VariableProvider, LazyVariables};

#[derive(Deserialize, Serialize, Default)]
pub struct Config {
    pub name: Option<String>,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_error")]
    pub error: String,
}

impl ConfigWithName for Config {
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn error(&self) -> &str {
        &self.error
    }
}

pub struct TimeProvider;

impl LazyVariables for TimeProvider {
    type Error = ParseError;
    
    fn get_variable(name: &str) -> Result<String, Self::Error> {
        match name {
            "time" => Ok(Local::now().format("%H:%M:%S").to_string()),
            _ => Ok(Local::now().format(name).to_string()), // Treat unknown as format string
        }
    }
    
    fn variable_names() -> &'static [&'static str] {
        &["time"] // Basic variable, but format strings are handled directly
    }
}

impl VariableProvider for TimeProvider {
    type Error = ParseError;
    type Config = Config;

    fn get_value(config: &Self::Config) -> Result<String, Self::Error> {
        format_current_time(&config.format)
    }

    fn section_name() -> &'static str {
        "time"
    }
}

fn default_format() -> String {
    "%H:%M:%S".to_string()
}

fn default_error() -> String {
    String::new()
}

pub fn format_current_time(format: &str) -> Result<String, ParseError> {
    // Skip if no format specifiers
    if !format.contains('%') {
        return Ok(format.to_string());
    }
    Ok(Local::now().format(format).to_string())
}
