use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub time: Option<TimeConfig>,
    #[serde(default)]
    pub hostname: Option<HostnameConfig>,
    #[serde(default)]
    pub cwd: Option<CwdConfig>,
    #[serde(default)]
    pub git: Option<GitConfig>,
    #[serde(default)]
    pub ip: Option<IpConfig>,
    #[serde(default)]
    pub battery: Option<BatteryConfig>,
    pub prompt: PromptConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TimeConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_time_format")]
    pub format: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HostnameConfig {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CwdConfig {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitConfig {
    #[serde(default)]
    pub name: Option<String>,
    // Future: show_dirty, show_ahead_behind
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IpConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub interface: Option<String>,
    #[serde(default)]
    pub prefer_ipv6: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BatteryConfig {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PromptConfig {
    pub format: String,
    #[serde(default)]
    pub format_wide: Option<String>,
    #[serde(default)]
    pub format_narrow: Option<String>,
    #[serde(default = "default_width_threshold")]
    pub width_threshold: u16,
}

fn default_time_format() -> String {
    "%H:%M:%S".to_string()
}

fn default_width_threshold() -> u16 {
    100
}

impl PromptConfig {
    /// Get the appropriate format string based on terminal width
    ///
    /// # Arguments
    /// * `terminal_width` - Current terminal width in columns, or None if unknown
    ///
    /// # Returns
    /// The format string to use (wide, narrow, or default)
    pub fn get_format(&self, terminal_width: Option<u16>) -> &str {
        // If terminal width is available, check for responsive formats
        if let Some(width) = terminal_width {
            // If width is below threshold and narrow format is configured, use it
            if width < self.width_threshold {
                if let Some(ref narrow) = self.format_narrow {
                    return narrow;
                }
            }

            // If width is at/above threshold and wide format is configured, use it
            if width >= self.width_threshold {
                if let Some(ref wide) = self.format_wide {
                    return wide;
                }
            }
        }

        // Fallback to default format (always available)
        &self.format
    }
}

impl Config {
    pub fn has_section(&self, section: &str) -> bool {
        match section {
            "time" => self.time.is_some(),
            "hostname" => self.hostname.is_some(),
            "cwd" => self.cwd.is_some(),
            "git" => self.git.is_some(),
            "ip" => self.ip.is_some(),
            "battery" => self.battery.is_some(),
            _ => false,
        }
    }

    pub fn add_implicit_section(&mut self, section: String, _value: serde_json::Value) {
        match section.as_str() {
            "time" => self.time = Some(TimeConfig {
                name: None,
                format: "%H:%M:%S".to_string()
            }),
            "hostname" => self.hostname = Some(HostnameConfig { name: None }),
            "cwd" => self.cwd = Some(CwdConfig { name: None }),
            "git" => self.git = Some(GitConfig { name: None }),
            "ip" => self.ip = Some(IpConfig {
                name: None,
                interface: None,
                prefer_ipv6: false,
            }),
            "battery" => self.battery = Some(BatteryConfig {
                name: None,
            }),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_format_no_responsive() {
        // No responsive formats configured - should always use default
        let prompt = PromptConfig {
            format: "default".to_string(),
            format_wide: None,
            format_narrow: None,
            width_threshold: 100,
        };

        assert_eq!(prompt.get_format(Some(50)), "default");
        assert_eq!(prompt.get_format(Some(150)), "default");
        assert_eq!(prompt.get_format(None), "default");
    }

    #[test]
    fn test_get_format_narrow_only() {
        // Only narrow format configured
        let prompt = PromptConfig {
            format: "default".to_string(),
            format_wide: None,
            format_narrow: Some("narrow".to_string()),
            width_threshold: 100,
        };

        // Below threshold - use narrow
        assert_eq!(prompt.get_format(Some(50)), "narrow");
        assert_eq!(prompt.get_format(Some(99)), "narrow");

        // At/above threshold - use default (no wide configured)
        assert_eq!(prompt.get_format(Some(100)), "default");
        assert_eq!(prompt.get_format(Some(150)), "default");

        // No width - use default
        assert_eq!(prompt.get_format(None), "default");
    }

    #[test]
    fn test_get_format_wide_only() {
        // Only wide format configured
        let prompt = PromptConfig {
            format: "default".to_string(),
            format_wide: Some("wide".to_string()),
            format_narrow: None,
            width_threshold: 100,
        };

        // Below threshold - use default (no narrow configured)
        assert_eq!(prompt.get_format(Some(50)), "default");
        assert_eq!(prompt.get_format(Some(99)), "default");

        // At/above threshold - use wide
        assert_eq!(prompt.get_format(Some(100)), "wide");
        assert_eq!(prompt.get_format(Some(150)), "wide");

        // No width - use default
        assert_eq!(prompt.get_format(None), "default");
    }

    #[test]
    fn test_get_format_both() {
        // Both wide and narrow configured
        let prompt = PromptConfig {
            format: "default".to_string(),
            format_wide: Some("wide".to_string()),
            format_narrow: Some("narrow".to_string()),
            width_threshold: 100,
        };

        // Below threshold - use narrow
        assert_eq!(prompt.get_format(Some(50)), "narrow");
        assert_eq!(prompt.get_format(Some(99)), "narrow");

        // At/above threshold - use wide
        assert_eq!(prompt.get_format(Some(100)), "wide");
        assert_eq!(prompt.get_format(Some(150)), "wide");

        // No width - use default
        assert_eq!(prompt.get_format(None), "default");
    }

    #[test]
    fn test_get_format_custom_threshold() {
        // Custom threshold of 80
        let prompt = PromptConfig {
            format: "default".to_string(),
            format_wide: Some("wide".to_string()),
            format_narrow: Some("narrow".to_string()),
            width_threshold: 80,
        };

        assert_eq!(prompt.get_format(Some(50)), "narrow");
        assert_eq!(prompt.get_format(Some(79)), "narrow");
        assert_eq!(prompt.get_format(Some(80)), "wide");
        assert_eq!(prompt.get_format(Some(100)), "wide");
    }
}
