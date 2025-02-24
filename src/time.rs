use chrono::Local;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum TimeError {
    Format(()),
}

impl fmt::Display for TimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeError::Format(_) => write!(f, "Invalid time format"),
        }
    }
}

impl Error for TimeError {}

#[derive(Deserialize, Serialize)]
pub struct TimeConfig {
    #[serde(default = "default_time_format")]
    pub format: String,
    pub name: Option<String>,
    #[serde(default)]
    pub deferred: bool,
}

impl Default for TimeConfig {
    fn default() -> Self {
        Self {
            format: default_time_format(),
            name: None,
            deferred: false,
        }
    }
}

fn default_time_format() -> String {
    "%H:%M:%S".to_string()
}

pub fn format_current_time(format: &str) -> Result<String, TimeError> {
    if format.is_empty() {
        return Ok(String::new());
    }

    // Check for invalid format specifiers
    let mut i = 0;
    let bytes = format.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'%' {
            i += 1;
            if i >= bytes.len() {
                return Err(TimeError::Format(()));
            }
            // Handle numeric modifiers (e.g. %3f)
            if bytes[i].is_ascii_digit() {
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err(TimeError::Format(()));
                }
            }
            match bytes[i] {
                b'A' | b'a' | b'B' | b'b' | b'C' | b'c' | b'd' | b'D' | b'e' | b'f' | b'F'
                | b'H' | b'h' | b'I' | b'j' | b'k' | b'l' | b'M' | b'm' | b'n' | b'P' | b'p'
                | b'R' | b'r' | b'S' | b'T' | b't' | b'U' | b'u' | b'V' | b'v' | b'W' | b'w'
                | b'X' | b'x' | b'Y' | b'y' | b'Z' | b'z' | b'%' => (),
                _ => return Err(TimeError::Format(())),
            }
        }
        i += 1;
    }

    let now = Local::now();
    Ok(now.format(format).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use std::time::Duration;

    #[test]
    fn test_default_time_format() {
        let config = TimeConfig::default();
        assert_eq!(config.format, "%H:%M:%S");
        assert_eq!(config.name, None);
        assert!(!config.deferred);
    }

    #[test]
    fn test_custom_time_format() {
        let config = TimeConfig {
            format: "%Y-%m-%d %H:%M:%S".to_string(),
            name: Some("datetime".to_string()),
            deferred: false,
        };
        assert_eq!(config.format, "%Y-%m-%d %H:%M:%S");
        assert_eq!(config.name, Some("datetime".to_string()));
        assert!(!config.deferred);
    }

    #[test]
    fn test_format_current_time_default() {
        let result = format_current_time("%H:%M:%S");
        assert!(result.is_ok());
        let time = result.unwrap();
        assert_eq!(time.len(), 8); // HH:MM:SS = 8 chars
        assert!(time.contains(":"));
    }

    #[test]
    fn test_format_current_time_custom() {
        let result = format_current_time("%Y-%m-%d");
        assert!(result.is_ok());
        let date = result.unwrap();
        assert_eq!(date.len(), 10); // YYYY-MM-DD = 10 chars
        assert!(date.contains("-"));
    }

    #[test]
    fn test_format_current_time_invalid() {
        let result = format_current_time("%invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_format_current_time_empty() {
        let result = format_current_time("");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_format_current_time_unicode() {
        let result = format_current_time("时间：%H时%M分%S秒");
        assert!(result.is_ok());
        let time = result.unwrap();
        assert!(time.starts_with("时间："));
        assert!(time.contains("时"));
        assert!(time.contains("分"));
        assert!(time.contains("秒"));
    }

    #[test]
    fn test_format_current_time_all_specifiers() {
        // Only use format specifiers that are supported by chrono
        let format = "%Y-%m-%d %H:%M:%S %Z %z %A %B %a %b %d %w %j %U %W %c %x %X %p %P";
        let result = format_current_time(format);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(
            !output.contains('%'),
            "Some format specifiers were not replaced"
        );
    }

    #[test]
    fn test_format_current_time_mixed_valid_invalid() {
        let format = "%H:%M:%S %invalid %Y-%m-%d";
        let result = format_current_time(format);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_current_time_parse_error() {
        let format = "%"; // Incomplete format specifier
        let result = format_current_time(format);
        assert!(result.is_err());
    }

    #[test]
    fn test_time_matches_system() {
        let now = Local::now();
        let formatted = format_current_time("%H:%M").unwrap();
        let system_time = now.format("%H:%M").to_string();
        assert_eq!(formatted, system_time);
    }

    #[test]
    fn test_format_current_time_performance() {
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            let _ = format_current_time("%Y-%m-%d %H:%M:%S.%3f");
        }
        let duration = start.elapsed();
        assert!(duration < Duration::from_secs(1));
    }

    #[test]
    fn test_format_current_time_complex() {
        let format = "Year: %Y\nMonth: %m\nDay: %d\nTime: %H:%M:%S\nTimezone: %Z";
        let result = format_current_time(format);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Year: "));
        assert!(output.contains("Month: "));
        assert!(output.contains("Day: "));
        assert!(output.contains("Time: "));
        assert!(output.contains("Timezone: "));
    }
}
