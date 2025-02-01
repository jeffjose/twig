use chrono::{format::ParseError, Local};
use serde::{Deserialize, Serialize};
use crate::variable::{ConfigWithName, VariableProvider};

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

impl VariableProvider for TimeProvider {
    type Error = chrono::format::ParseError;
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
    Ok(Local::now().format(format).to_string())
}
