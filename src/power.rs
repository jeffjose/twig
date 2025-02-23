use battery::{Manager, State};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug)]
pub enum PowerError {
    BatteryError(battery::Error),
    BatteryNotFound,
    IoError(std::io::Error),
    JsonError(serde_json::Error),
}

impl fmt::Display for PowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PowerError::BatteryError(e) => write!(f, "Battery error: {}", e),
            PowerError::BatteryNotFound => write!(f, "No battery found"),
            PowerError::IoError(e) => write!(f, "IO error: {}", e),
            PowerError::JsonError(e) => write!(f, "JSON error: {}", e),
        }
    }
}

impl Error for PowerError {}

impl From<battery::Error> for PowerError {
    fn from(err: battery::Error) -> Self {
        PowerError::BatteryError(err)
    }
}

impl From<std::io::Error> for PowerError {
    fn from(err: std::io::Error) -> Self {
        PowerError::IoError(err)
    }
}

impl From<serde_json::Error> for PowerError {
    fn from(err: serde_json::Error) -> Self {
        PowerError::JsonError(err)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryInfo {
    pub percentage: i32,
    pub status: String,
    pub time_left: String,
    pub power_now: f64,
    pub energy_now: f64,
    pub energy_full: f64,
    pub voltage: f64,
    pub temperature: f64,
    pub technology: String,
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    pub cycle_count: i32,
    pub capacity: i32,
    #[serde(with = "system_time_serde")]
    pub cached_at: SystemTime,
}

// Serialization helper for SystemTime
mod system_time_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = time
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0));
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(UNIX_EPOCH + Duration::from_secs(secs))
    }
}

impl Default for BatteryInfo {
    fn default() -> Self {
        Self {
            percentage: 0,
            status: String::from("Unknown"),
            time_left: String::from("Unknown"),
            power_now: 0.0,
            energy_now: 0.0,
            energy_full: 0.0,
            voltage: 0.0,
            temperature: 0.0,
            technology: String::from("Unknown"),
            manufacturer: String::from("Unknown"),
            model: String::from("Unknown"),
            serial: String::from("Unknown"),
            cycle_count: 0,
            capacity: 0,
            cached_at: SystemTime::now(),
        }
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub name: Option<String>,
    pub format: String,
    #[serde(default)]
    pub cache_enabled: Option<bool>,
    #[serde(default)]
    pub cache_duration: Option<u64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: None,
            format: String::new(),
            cache_enabled: None,
            cache_duration: None,
        }
    }
}

fn get_cache_path() -> Result<PathBuf, PowerError> {
    BaseDirs::new()
        .map(|base_dirs| {
            base_dirs
                .cache_dir()
                .join("twig")
                .join("battery_cache.json")
        })
        .ok_or_else(|| PowerError::BatteryNotFound)
}

fn read_cache() -> Result<Option<BatteryInfo>, PowerError> {
    let cache_path = get_cache_path()?;
    if !cache_path.exists() {
        eprintln!("Cache file does not exist at {:?}", cache_path);
        return Ok(None);
    }

    let cache_content = fs::read_to_string(&cache_path)?;
    serde_json::from_str(&cache_content)
        .map(Some)
        .map_err(Into::into)
}

fn write_cache(info: &BatteryInfo) -> Result<(), PowerError> {
    let cache_path = get_cache_path()?;

    // Ensure cache directory exists
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let cache_content = serde_json::to_string(info)?;
    fs::write(&cache_path, cache_content)?;
    eprintln!("Successfully wrote cache to {:?}", cache_path);
    Ok(())
}

pub fn get_battery_info_internal() -> Result<BatteryInfo, PowerError> {
    let manager_start = Instant::now();
    let manager = Manager::new()?;
    let manager_time = manager_start.elapsed();

    let battery_start = Instant::now();
    let battery = manager
        .batteries()?
        .next()
        .transpose()?
        .ok_or(PowerError::BatteryNotFound)?;
    let battery_time = battery_start.elapsed();

    let mut info = BatteryInfo::default();

    let info_start = Instant::now();
    // Basic information
    info.percentage = (battery.state_of_charge().value * 100.0) as i32;
    let state = battery.state();
    info.status = match state {
        State::Charging => "Charging",
        State::Discharging => "Discharging",
        State::Empty => "Empty",
        State::Full => "Full",
        State::Unknown => "Unknown",
        _ => "Unknown",
    }
    .to_string();

    // Time information
    if let Some(time_to_full) = battery.time_to_full() {
        info.time_left = format_duration(Duration::from_secs(time_to_full.value as u64));
    } else if let Some(time_to_empty) = battery.time_to_empty() {
        info.time_left = format_duration(Duration::from_secs(time_to_empty.value as u64));
    }

    // Power information
    let power_rate = battery.energy_rate().value as f64;
    info.power_now = match state {
        State::Charging => power_rate,
        State::Discharging => -power_rate,
        _ => power_rate,
    };
    info.energy_now = battery.energy().value as f64;
    info.energy_full = battery.energy_full().value as f64;
    info.voltage = battery.voltage().value as f64;

    // Temperature (if available)
    if let Some(temp) = battery.temperature() {
        info.temperature = temp.value as f64;
    }

    // Cycle count (if available)
    if let Some(cycles) = battery.cycle_count() {
        info.cycle_count = cycles as i32;
    }

    // Technology
    info.technology = format!("{:?}", battery.technology());

    // Manufacturer and model information
    info.manufacturer = battery.vendor().unwrap_or("Unknown").to_string();
    info.model = battery.model().unwrap_or("Unknown").to_string();
    info.serial = "Unknown".to_string();

    // Health/capacity percentage
    info.capacity = (battery.state_of_health().value * 100.0) as i32;
    let info_time = info_start.elapsed();

    eprintln!(
        "Power timing: Manager init: {:?}, Battery access: {:?}, Info gathering: {:?}",
        manager_time, battery_time, info_time
    );

    Ok(info)
}

fn format_duration(duration: Duration) -> String {
    let total_minutes = duration.as_secs() / 60;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;

    if hours > 0 {
        if minutes > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}h", hours)
        }
    } else {
        format!("{}m", minutes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_format_duration() {
        // Test various duration formats
        assert_eq!(format_duration(Duration::from_secs(0)), "0m");
        assert_eq!(format_duration(Duration::from_secs(30)), "0m");
        assert_eq!(format_duration(Duration::from_secs(60)), "1m");
        assert_eq!(format_duration(Duration::from_secs(90)), "1m");
        assert_eq!(format_duration(Duration::from_secs(3600)), "1h");
        assert_eq!(format_duration(Duration::from_secs(3660)), "1h 1m");
        assert_eq!(format_duration(Duration::from_secs(7200)), "2h");
        assert_eq!(format_duration(Duration::from_secs(7230)), "2h");
        assert_eq!(format_duration(Duration::from_secs(7290)), "2h 1m");
    }

    #[test]
    fn test_battery_info_default() {
        let info = BatteryInfo::default();
        assert_eq!(info.percentage, 0);
        assert_eq!(info.status, "Unknown");
        assert_eq!(info.time_left, "Unknown");
        assert_eq!(info.power_now, 0.0);
        assert_eq!(info.energy_now, 0.0);
        assert_eq!(info.energy_full, 0.0);
        assert_eq!(info.voltage, 0.0);
        assert_eq!(info.temperature, 0.0);
        assert_eq!(info.technology, "Unknown");
        assert_eq!(info.manufacturer, "Unknown");
        assert_eq!(info.model, "Unknown");
        assert_eq!(info.serial, "Unknown");
        assert_eq!(info.cycle_count, 0);
        assert_eq!(info.capacity, 0);
    }

    #[test]
    fn test_power_error_display() {
        let err = PowerError::BatteryNotFound;
        assert_eq!(err.to_string(), "No battery found");
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert!(config.name.is_none());
        assert_eq!(config.format, "");
    }

    #[test]
    fn test_config_serialization() {
        let config = Config {
            name: Some("test".to_string()),
            format: "{percentage}% ({power_now}W)".to_string(),
            cache_enabled: Some(true),
            cache_duration: Some(10),
        };

        // Test serialization
        let serialized = serde_json::to_string(&config).unwrap();
        assert_eq!(
            serialized,
            r#"{"name":"test","format":"{percentage}% ({power_now}W)","cache_enabled":true,"cache_duration":10}"#
        );

        // Test deserialization
        let deserialized: Config = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.name, Some("test".to_string()));
        assert_eq!(deserialized.format, "{percentage}% ({power_now}W)");
        assert_eq!(deserialized.cache_enabled, Some(true));
        assert_eq!(deserialized.cache_duration, Some(10));
    }

    #[test]
    fn test_battery_info_debug() {
        let info = BatteryInfo::default();
        let debug_str = format!("{:?}", info);

        // Check all fields are present in debug output
        assert!(debug_str.contains("percentage: 0"));
        assert!(debug_str.contains("status: \"Unknown\""));
        assert!(debug_str.contains("time_left: \"Unknown\""));
        assert!(debug_str.contains("power_now: 0.0"));
        assert!(debug_str.contains("energy_now: 0.0"));
        assert!(debug_str.contains("energy_full: 0.0"));
        assert!(debug_str.contains("voltage: 0.0"));
        assert!(debug_str.contains("temperature: 0.0"));
        assert!(debug_str.contains("technology: \"Unknown\""));
        assert!(debug_str.contains("manufacturer: \"Unknown\""));
        assert!(debug_str.contains("model: \"Unknown\""));
        assert!(debug_str.contains("serial: \"Unknown\""));
        assert!(debug_str.contains("cycle_count: 0"));
        assert!(debug_str.contains("capacity: 0"));
    }

    #[test]
    fn test_battery_state_conversion() {
        // Helper function to convert state to string like in get_battery_info
        let convert_state = |state: State| -> String {
            match state {
                State::Charging => "Charging",
                State::Discharging => "Discharging",
                State::Empty => "Empty",
                State::Full => "Full",
                State::Unknown => "Unknown",
                _ => "Unknown",
            }
            .to_string()
        };

        // Test all known states
        assert_eq!(convert_state(State::Charging), "Charging");
        assert_eq!(convert_state(State::Discharging), "Discharging");
        assert_eq!(convert_state(State::Empty), "Empty");
        assert_eq!(convert_state(State::Full), "Full");
        assert_eq!(convert_state(State::Unknown), "Unknown");
    }

    #[test]
    fn test_percentage_calculation() {
        // Helper function to calculate percentage like in get_battery_info
        let calculate_percentage = |value: f32| -> i32 { (value * 100.0) as i32 };

        // Test various percentage calculations
        assert_eq!(calculate_percentage(0.0), 0);
        assert_eq!(calculate_percentage(0.5), 50);
        assert_eq!(calculate_percentage(1.0), 100);
        assert_eq!(calculate_percentage(0.01), 1);
        assert_eq!(calculate_percentage(0.99), 99);
    }

    #[test]
    fn test_power_formatting() {
        // Helper function to format power like in main.rs
        let format_power = |value: f64| -> String {
            if value.abs() < 0.01 {
                "0.0".to_string()
            } else {
                format!("{:+.1}", value)
            }
        };

        // Test power formatting with various values
        assert_eq!(format_power(0.0), "0.0");
        assert_eq!(format_power(0.001), "0.0");
        assert_eq!(format_power(-0.009), "0.0");
        assert_eq!(format_power(1.234), "+1.2");
        assert_eq!(format_power(-5.678), "-5.7");
        assert_eq!(format_power(10.0), "+10.0");
    }

    #[test]
    fn test_error_impl() {
        // Test that PowerError implements std::error::Error
        let err = PowerError::BatteryNotFound;
        let _err_ref: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_error_debug() {
        // Test Debug implementation
        let err = PowerError::BatteryNotFound;
        assert_eq!(format!("{:?}", err), "BatteryNotFound");
    }
}
