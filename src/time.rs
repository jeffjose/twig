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
    use regex::Regex;
    use std::time::Instant;

    #[test]
    fn test_format_current_time_empty() {
        let result = format_current_time("");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_format_current_time_invalid() {
        let result = format_current_time("%invalid");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Invalid time format");
    }

    #[test]
    fn test_format_current_time_parse_error() {
        let result = format_current_time("%");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Invalid time format");
    }

    #[test]
    fn test_format_current_time_mixed_valid_invalid() {
        let result = format_current_time("%H:%M:%invalid");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Invalid time format");
    }

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
    fn test_time_matches_system() {
        let now = Local::now();
        let formatted = format_current_time("%H").unwrap();
        let system_hour = now.format("%H").to_string();
        assert_eq!(formatted, system_hour);
    }

    #[test]
    fn test_format_current_time_complex() {
        let format = "%Y-%m-%d %H:%M:%S.%3f %z %Z";
        let result = format_current_time(format).unwrap();
        // Try different possible formats:
        // 1. "2024-03-21 15:30:45.123 +0000 UTC"
        // 2. "2024-03-21 15:30:45.123 -0800 PST"
        // 3. "2024-03-21 15:30:45.123 -0800" (no timezone name)
        let patterns = [
            r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3} [+-]\d{4} \w+$",
            r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3} [+-]\d{4}$",
            r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d+ [+-]\d{4}.*$",
        ];

        let matches = patterns
            .iter()
            .any(|pattern| Regex::new(pattern).unwrap().is_match(&result));

        assert!(
            matches,
            "Time format '{}' did not match any expected pattern",
            result
        );
    }

    #[test]
    fn test_format_current_time_unicode() {
        let format = "年:%Y 月:%m 日:%d 時:%H 分:%M 秒:%S";
        let result = format_current_time(format).unwrap();
        let re = Regex::new(r"^年:\d{4} 月:\d{2} 日:\d{2} 時:\d{2} 分:\d{2} 秒:\d{2}$").unwrap();
        assert!(re.is_match(&result));
    }

    #[test]
    fn test_format_current_time_performance() {
        let formats = [
            "%Y-%m-%d %H:%M:%S",
            "%Y-%m-%d %H:%M:%S.%3f %z",
            "%A, %B %d, %Y at %H:%M:%S",
            "%Y年%m月%d日 %H時%M分%S秒",
            "%d/%m/%y %I:%M %p",
        ];

        let iterations = 1000;
        let start = Instant::now();

        for &format in &formats {
            for _ in 0..iterations {
                let _ = format_current_time(format);
            }
        }

        let duration = start.elapsed();
        let avg_duration = duration.as_micros() as f64 / (formats.len() * iterations) as f64;

        // Average time should be less than 50 microseconds per format
        assert!(
            avg_duration < 50.0,
            "Time formatting is too slow: {} µs",
            avg_duration
        );
    }

    #[test]
    fn test_format_current_time_all_specifiers() {
        let format = "%Y-%m-%d %H:%M:%S.%f %A %B %Z %z %p %j %U %W %c %x %X";
        let result = format_current_time(format).unwrap();
        assert!(
            !result.contains('%'),
            "Some format specifiers were not replaced"
        );
    }
}
