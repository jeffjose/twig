use battery::{Manager, State};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::time::Duration;

#[derive(Debug)]
pub enum PowerError {
    BatteryError(battery::Error),
    BatteryNotFound,
}

impl fmt::Display for PowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PowerError::BatteryError(e) => write!(f, "Battery error: {}", e),
            PowerError::BatteryNotFound => write!(f, "No battery found"),
        }
    }
}

impl Error for PowerError {}

impl From<battery::Error> for PowerError {
    fn from(err: battery::Error) -> Self {
        PowerError::BatteryError(err)
    }
}

#[derive(Deserialize, Serialize, Default)]
pub struct Config {
    pub name: Option<String>,
    pub format: String,
}

#[derive(Debug)]
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
        }
    }
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

pub fn get_battery_info() -> Result<BatteryInfo, PowerError> {
    let manager = Manager::new()?;
    let battery = manager
        .batteries()?
        .next()
        .transpose()?
        .ok_or(PowerError::BatteryNotFound)?;

    let mut info = BatteryInfo::default();

    // Basic information
    info.percentage = (battery.state_of_charge().value * 100.0) as i32;
    info.status = match battery.state() {
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
    info.power_now = battery.energy_rate().value as f64;
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
    info.serial = "Unknown".to_string(); // Serial number not available in the battery crate

    // Health/capacity percentage
    info.capacity = (battery.state_of_health().value * 100.0) as i32;

    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        use std::time::Duration;

        // Test hours and minutes
        let duration = Duration::from_secs(7200 + 1800); // 2h 30m
        assert_eq!(format_duration(duration), "2h 30m");

        // Test minutes only
        let duration = Duration::from_secs(1800); // 30m
        assert_eq!(format_duration(duration), "30m");

        // Test hours only
        let duration = Duration::from_secs(7200); // 2h
        assert_eq!(format_duration(duration), "2h");

        // Test zero duration
        let duration = Duration::from_secs(0);
        assert_eq!(format_duration(duration), "0m");
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
        assert_eq!(info.manufacturer, "Unknown");
        assert_eq!(info.model, "Unknown");
        assert_eq!(info.serial, "Unknown");
    }

    #[test]
    fn test_power_error_display() {
        // Test BatteryNotFound display
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
        use serde_json;

        // Test serialization
        let config = Config {
            name: Some("test".to_string()),
            format: "{percentage}%".to_string(),
        };
        let serialized = serde_json::to_string(&config).unwrap();
        assert_eq!(serialized, r#"{"name":"test","format":"{percentage}%"}"#);

        // Test deserialization
        let deserialized: Config = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.name, Some("test".to_string()));
        assert_eq!(deserialized.format, "{percentage}%");
    }

    #[test]
    fn test_battery_info_debug() {
        let info = BatteryInfo::default();
        let debug_str = format!("{:?}", info);

        // Verify debug output contains all fields
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
        use battery::State;

        // Test state conversion using the same logic as in get_battery_info
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

        assert_eq!(convert_state(State::Charging), "Charging");
        assert_eq!(convert_state(State::Discharging), "Discharging");
        assert_eq!(convert_state(State::Empty), "Empty");
        assert_eq!(convert_state(State::Full), "Full");
        assert_eq!(convert_state(State::Unknown), "Unknown");
    }

    #[test]
    fn test_percentage_calculation() {
        // Test percentage calculation helper
        let calculate_percentage = |value: f32| (value * 100.0) as i32;

        // Test various values
        assert_eq!(calculate_percentage(0.0), 0);
        assert_eq!(calculate_percentage(0.5), 50);
        assert_eq!(calculate_percentage(1.0), 100);
        assert_eq!(calculate_percentage(0.333), 33);
        assert_eq!(calculate_percentage(0.666), 66);
    }
}
