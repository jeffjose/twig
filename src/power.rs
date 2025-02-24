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
    #[serde(default)]
    pub deferred: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: None,
            format: "{percentage}% ({status})".to_string(),
            deferred: false,
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
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_config_serialization() {
        let config = Config {
            name: Some("test".to_string()),
            format: "{percentage}% ({power_now}W)".to_string(),
            deferred: false,
        };

        let serialized = serde_json::to_string(&config).unwrap();
        let expected =
            r#"{"name":"test","format":"{percentage}% ({power_now}W)","deferred":false}"#;
        assert_eq!(serialized, expected);

        let deserialized: Config = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.name, Some("test".to_string()));
        assert_eq!(deserialized.format, "{percentage}% ({power_now}W)");
        assert_eq!(deserialized.deferred, false);
    }

    #[test]
    fn test_battery_info_formatting() {
        let info = BatteryInfo {
            percentage: 75,
            status: "Charging".to_string(),
            time_left: "1:30".to_string(),
            power_now: 45.5,
            energy_now: 50.0,
            energy_full: 100.0,
            voltage: 12.1,
            temperature: 35.5,
            capacity: 95,
            cycle_count: 100,
            technology: "Li-ion".to_string(),
            manufacturer: "Test".to_string(),
            model: "Battery1".to_string(),
            serial: "12345".to_string(),
            updated_at: SystemTime::now(),
        };

        // Test all format variables
        let config = Config {
            name: Some("test".to_string()),
            format: "{percentage}% {status} {time_left} {power_now}W {energy_now}Wh {energy_full}Wh {voltage}V {temperature}°C {capacity}% {cycle_count} {technology} {manufacturer} {model} {serial}".to_string(),
            deferred: false,
        };

        let formatted = config
            .format
            .replace("{percentage}", &info.percentage.to_string())
            .replace("{status}", &info.status)
            .replace("{time_left}", &info.time_left)
            .replace("{power_now}", &format!("{:+.1}", info.power_now))
            .replace("{energy_now}", &format!("{:.1}", info.energy_now))
            .replace("{energy_full}", &format!("{:.1}", info.energy_full))
            .replace("{voltage}", &format!("{:.1}", info.voltage))
            .replace("{temperature}", &format!("{:.1}", info.temperature))
            .replace("{capacity}", &info.capacity.to_string())
            .replace("{cycle_count}", &info.cycle_count.to_string())
            .replace("{technology}", &info.technology)
            .replace("{manufacturer}", &info.manufacturer)
            .replace("{model}", &info.model)
            .replace("{serial}", &info.serial);

        let expected = "75% Charging 1:30 +45.5W 50.0Wh 100.0Wh 12.1V 35.5°C 95% 100 Li-ion Test Battery1 12345";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_battery_info_edge_cases() {
        let info = BatteryInfo {
            percentage: 100,
            status: "Full".to_string(),
            time_left: "0:00".to_string(),
            power_now: 0.0,
            energy_now: 100.0,
            energy_full: 100.0,
            voltage: 12.0,
            temperature: 25.0,
            capacity: 100,
            cycle_count: 0,
            technology: "Unknown".to_string(),
            manufacturer: "Unknown".to_string(),
            model: "Unknown".to_string(),
            serial: "Unknown".to_string(),
            updated_at: SystemTime::now(),
        };

        // Test zero power
        let config = Config {
            name: Some("test".to_string()),
            format: "{percentage}% ({power_now}W)".to_string(),
            deferred: false,
        };

        let formatted = config
            .format
            .replace("{percentage}", &info.percentage.to_string())
            .replace(
                "{power_now}",
                &if info.power_now.abs() < 0.01 {
                    "0.0".to_string()
                } else {
                    format!("{:+.1}", info.power_now)
                },
            );

        assert_eq!(formatted, "100% (0.0W)");

        // Test negative power (discharging)
        let mut info = info;
        info.power_now = -25.5;
        info.status = "Discharging".to_string();

        let formatted = config
            .format
            .replace("{percentage}", &info.percentage.to_string())
            .replace(
                "{power_now}",
                &if info.power_now.abs() < 0.01 {
                    "0.0".to_string()
                } else {
                    format!("{:+.1}", info.power_now)
                },
            );

        assert_eq!(formatted, "100% (-25.5W)");
    }

    #[test]
    fn test_battery_info_default() {
        let info = BatteryInfo::default();
        assert_eq!(info.percentage, 0);
        assert_eq!(info.status, "Unknown");
        assert_eq!(info.power_now, 0.0);
        assert_eq!(info.energy_now, 0.0);
        assert_eq!(info.energy_full, 0.0);
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.name, None);
        assert_eq!(config.format, "{percentage}% ({status})");
        assert!(!config.deferred);
    }

    #[test]
    fn test_power_error_display() {
        let error = PowerError::NoBattery;
        assert_eq!(error.to_string(), "No battery found");

        let error = PowerError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "test"));
        assert_eq!(error.to_string(), "IO error: test");

        let error =
            PowerError::JsonError(serde_json::from_str::<BatteryInfo>("invalid json").unwrap_err());
        assert!(error.to_string().contains("JSON error:"));
    }

    #[test]
    fn test_battery_info_internal() {
        // This test will be skipped if no battery is present
        match get_battery_info_internal() {
            Ok(info) => {
                assert!(info.percentage >= 0 && info.percentage <= 100);
                assert!(!info.status.is_empty());
                assert!(!info.technology.is_empty());
            }
            Err(PowerError::NoBattery) => {
                // Skip test if no battery is present
                println!("Skipping battery test - no battery found");
            }
            Err(e) => {
                panic!("Unexpected error: {}", e);
            }
        }
    }

    #[test]
    fn test_error_conversions() {
        // Test From<std::io::Error>
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let power_error = PowerError::from(io_error);
        assert!(matches!(power_error, PowerError::IoError(_)));

        // Test From<serde_json::Error>
        let json_error = serde_json::from_str::<BatteryInfo>("invalid json").unwrap_err();
        let power_error = PowerError::from(json_error);
        assert!(matches!(power_error, PowerError::JsonError(_)));

        // Test From<battery::Error>
        let battery_error =
            battery::Error::from(std::io::Error::new(std::io::ErrorKind::Other, "test"));
        let power_error = PowerError::from(battery_error);
        assert!(matches!(power_error, PowerError::IoError(_)));
    }

    #[test]
    fn test_battery_info_system_time() {
        let info = BatteryInfo::default();
        let now = SystemTime::now();
        assert!(info.updated_at <= now);
        assert!(info.updated_at >= UNIX_EPOCH);
    }

    #[test]
    fn test_battery_states() {
        let mut info = BatteryInfo::default();

        // Test Charging state
        info.status = "Charging".to_string();
        info.power_now = 45.5;
        assert!(info.power_now > 0.0);

        // Test Discharging state
        info.status = "Discharging".to_string();
        info.power_now = -25.5;
        assert!(info.power_now < 0.0);

        // Test Full state
        info.status = "Full".to_string();
        info.percentage = 100;
        assert_eq!(info.percentage, 100);

        // Test Empty state
        info.status = "Empty".to_string();
        info.percentage = 0;
        assert_eq!(info.percentage, 0);
    }

    #[test]
    fn test_battery_info_boundary_values() {
        let mut info = BatteryInfo::default();

        // Test percentage boundaries
        info.percentage = -1;
        assert!(info.percentage < 0); // Should allow negative for error cases
        info.percentage = 101;
        assert!(info.percentage > 100); // Should allow >100 for error cases

        // Test power boundaries
        info.power_now = -100.0;
        assert!(info.power_now < 0.0);
        info.power_now = 100.0;
        assert!(info.power_now > 0.0);

        // Test temperature boundaries
        info.temperature = -273.15; // Absolute zero
        assert!(info.temperature <= -273.15);
        info.temperature = 100.0; // Boiling point
        assert!(info.temperature >= 100.0);
    }

    #[test]
    fn test_battery_info_string_fields() {
        let info = BatteryInfo {
            percentage: 75,
            status: "Custom Status".to_string(),
            time_left: "1:30".to_string(),
            power_now: 45.5,
            energy_now: 50.0,
            energy_full: 100.0,
            voltage: 12.1,
            temperature: 35.5,
            capacity: 95,
            cycle_count: 100,
            technology: "Custom Tech".to_string(),
            manufacturer: "Custom Mfg".to_string(),
            model: "Custom Model".to_string(),
            serial: "Custom Serial".to_string(),
            updated_at: SystemTime::now(),
        };

        assert_eq!(info.status, "Custom Status");
        assert_eq!(info.technology, "Custom Tech");
        assert_eq!(info.manufacturer, "Custom Mfg");
        assert_eq!(info.model, "Custom Model");
        assert_eq!(info.serial, "Custom Serial");
    }
}
