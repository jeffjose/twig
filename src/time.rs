use chrono::{Local, format::ParseError};
use serde::Deserialize;
use std::error::Error;

#[derive(Deserialize, Default)]
pub struct TimeConfig {
    #[serde(default = "default_time_format")]
    pub format: String,
}

fn default_time_format() -> String {
    "%H:%M:%S".to_string()
}

pub fn format_current_time(format: &str) -> Result<String, ParseError> {
    Ok(Local::now().format(format).to_string())
} 
