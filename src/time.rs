use chrono::{format::ParseError, Local};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Default)]
pub struct Config {
    pub name: Option<String>,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_error")]
    pub error: String,
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
