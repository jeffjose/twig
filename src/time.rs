use crate::variable::{replace_variables, ConfigWithName, LazyVariables, VariableProvider};
use chrono::{format::ParseError, Local};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

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

// Add a new error type that can handle both parse and template errors
#[derive(Debug)]
pub enum TimeError {
    Parse(ParseError),
}

impl fmt::Display for TimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeError::Parse(e) => write!(f, "Time parse error: {}", e),
        }
    }
}

impl Error for TimeError {}

impl From<ParseError> for TimeError {
    fn from(err: ParseError) -> Self {
        TimeError::Parse(err)
    }
}

impl LazyVariables for TimeProvider {
    type Error = TimeError;

    fn get_variable(name: &str) -> Result<String, Self::Error> {
        match name {
            "time" => Ok(Local::now().format("%H:%M:%S").to_string()),
            _ => Ok(Local::now().format(name).to_string()),
        }
    }

    fn variable_names() -> &'static [&'static str] {
        &["time"]
    }
}

impl VariableProvider for TimeProvider {
    type Error = TimeError;
    type Config = Config;

    fn get_value(config: &Self::Config) -> Result<String, Self::Error> {
        // If the format string doesn't contain any variables, return raw time
        if !config.format.contains('{') {
            return format_current_time(&config.format).map_err(TimeError::Parse);
        }

        // Get all needed variables using LazyVariables trait
        let vars = Self::get_needed_variables(&config.format)?;
        Ok(replace_variables(&config.format, &vars))
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
