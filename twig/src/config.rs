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
}

fn default_time_format() -> String {
    "%H:%M:%S".to_string()
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
