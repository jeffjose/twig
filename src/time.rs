use chrono::{format::ParseError, Local};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Default)]
pub struct TimeConfig {
    #[serde(default = "default_time_format")]
    pub format: String,
    pub name: Option<String>,
}

fn default_time_format() -> String {
    "%H:%M:%S".to_string()
}

pub fn format_current_time(format: &str) -> Result<String, ParseError> {
    Ok(Local::now().format(format).to_string())
}
