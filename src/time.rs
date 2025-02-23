use chrono::{format::ParseError, Local};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug)]
pub enum TimeError {
    Parse(ParseError),
    Format(String),
}

impl Display for TimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeError::Parse(e) => write!(f, "Parse error: {}", e),
            TimeError::Format(s) => write!(f, "Format error: {}", s),
        }
    }
}

impl std::error::Error for TimeError {}

#[derive(Deserialize, Serialize)]
pub struct TimeConfig {
    #[serde(default = "default_time_format")]
    pub format: String,
    pub name: Option<String>,
}

impl Default for TimeConfig {
    fn default() -> Self {
        Self {
            format: default_time_format(),
            name: None,
        }
    }
}

fn default_time_format() -> String {
    "%H:%M:%S".to_string()
}

pub fn format_current_time(format: &str) -> Result<String, TimeError> {
    let now = Local::now();

    // Try to format the time
    let result = std::panic::catch_unwind(|| now.format(format).to_string());

    match result {
        Ok(formatted) => {
            // Check if the format was actually applied
            if formatted == "Error" || (formatted.contains('%') && format.contains('%')) {
                Ok(format.to_string())
            } else {
                Ok(formatted)
            }
        }
        Err(_) => Ok(format.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[test]
    fn test_default_time_format() {
        let config = TimeConfig::default();
        assert_eq!(config.format, "%H:%M:%S");
    }

    #[test]
    fn test_custom_time_format() {
        let config = TimeConfig {
            format: "%Y-%m-%d".to_string(),
            name: Some("date".to_string()),
        };
        assert_eq!(config.format, "%Y-%m-%d");
        assert_eq!(config.name, Some("date".to_string()));
    }

    #[test]
    fn test_format_current_time_default() {
        let result = format_current_time("%H:%M:%S").unwrap();
        // Test that the output matches the HH:MM:SS pattern
        let re = Regex::new(r"^\d{2}:\d{2}:\d{2}$").unwrap();
        assert!(re.is_match(&result));
    }

    #[test]
    fn test_format_current_time_custom() {
        let result = format_current_time("%Y-%m-%d").unwrap();
        // Test that the output matches YYYY-MM-DD pattern
        let re = Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();
        assert!(re.is_match(&result));
    }

    #[test]
    fn test_format_current_time_invalid() {
        // Test with an invalid format string
        let result = format_current_time("%invalid");
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(
            output, "%invalid",
            "Invalid format string should be returned as-is"
        );
    }

    #[test]
    fn test_time_matches_system() {
        let now = Local::now();
        let formatted = format_current_time("%H").unwrap();
        let system_hour = now.format("%H").to_string();
        assert_eq!(formatted, system_hour);
    }
}
