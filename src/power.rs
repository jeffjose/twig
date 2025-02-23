use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::time::SystemTime;

#[derive(Debug)]
pub enum PowerError {
    IoError(std::io::Error),
    JsonError(serde_json::Error),
    NoBattery,
}

impl fmt::Display for PowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PowerError::IoError(e) => write!(f, "IO error: {}", e),
            PowerError::JsonError(e) => write!(f, "JSON error: {}", e),
            PowerError::NoBattery => write!(f, "No battery found"),
        }
    }
}

impl Error for PowerError {}

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

impl From<battery::Error> for PowerError {
    fn from(err: battery::Error) -> Self {
        PowerError::IoError(std::io::Error::new(
            std::io::ErrorKind::Other,
            err.to_string(),
        ))
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
    pub capacity: i32,
    pub cycle_count: i32,
    pub technology: String,
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    pub updated_at: SystemTime,
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
            capacity: 0,
            cycle_count: 0,
            technology: String::from("Unknown"),
            manufacturer: String::from("Unknown"),
            model: String::from("Unknown"),
            serial: String::from("Unknown"),
            updated_at: SystemTime::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub name: Option<String>,
    pub format: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: None,
            format: "{percentage}% ({status})".to_string(),
        }
    }
}

pub fn get_battery_info_internal() -> Result<BatteryInfo, PowerError> {
    let manager = battery::Manager::new()?;
    let mut batteries = manager.batteries()?;
    let battery = batteries.next().ok_or(PowerError::NoBattery)??;

    let state = battery.state();
    let percentage = (battery.state_of_charge().value * 100.0) as i32;
    let status = format!("{:?}", state);
    let time_left = match state {
        battery::State::Charging => battery.time_to_full().map(|t| format!("{:.1}h", t.value)),
        battery::State::Discharging => battery.time_to_empty().map(|t| format!("{:.1}h", t.value)),
        _ => None,
    }
    .unwrap_or_else(|| "N/A".to_string());

    let power_rate = battery.energy_rate().value as f64;
    let power_now = match state {
        battery::State::Charging => power_rate,
        battery::State::Discharging => -power_rate,
        _ => power_rate,
    };

    Ok(BatteryInfo {
        percentage,
        status,
        time_left,
        power_now,
        energy_now: battery.energy().value as f64,
        energy_full: battery.energy_full().value as f64,
        voltage: battery.voltage().value as f64,
        temperature: battery.temperature().map(|t| t.value as f64).unwrap_or(0.0),
        capacity: 100, // Default to 100% since we can't reliably get state of health
        cycle_count: battery.cycle_count().map(|c| c as i32).unwrap_or(0),
        technology: format!("{:?}", battery.technology()),
        manufacturer: battery.vendor().unwrap_or("Unknown").to_string(),
        model: battery.model().unwrap_or("Unknown").to_string(),
        serial: battery.serial_number().unwrap_or("Unknown").to_string(),
        updated_at: SystemTime::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let config = Config {
            name: Some("test".to_string()),
            format: "{percentage}% ({power_now}W)".to_string(),
        };

        let serialized = serde_json::to_string(&config).unwrap();
        let expected = r#"{"name":"test","format":"{percentage}% ({power_now}W)"}"#;
        assert_eq!(serialized, expected);

        let deserialized: Config = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.name, Some("test".to_string()));
        assert_eq!(deserialized.format, "{percentage}% ({power_now}W)");
    }
}
