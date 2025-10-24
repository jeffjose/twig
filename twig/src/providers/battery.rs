// twig/src/providers/battery.rs

use super::{Provider, ProviderResult};
use crate::config::Config;
use battery::{Manager, State};
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct BatteryProvider;

impl BatteryProvider {
    pub fn new() -> Self {
        Self
    }

    /// Get battery information
    /// Returns (percentage, status, power) where power is in watts (positive=charging, negative=discharging)
    fn get_battery_info(&self) -> Option<(u8, String, Option<String>)> {
        // Create battery manager
        let manager = Manager::new().ok()?;

        // Get first battery (most systems have only one)
        let mut batteries = manager.batteries().ok()?;
        let battery = batteries.next()?.ok()?;

        // Get state of charge (percentage)
        let percentage = (battery.state_of_charge().value * 100.0) as u8;

        // Get battery state
        let status = match battery.state() {
            State::Charging => "Charging",
            State::Discharging => "Discharging",
            State::Full => "Full",
            State::Empty => "Empty",
            _ => "Unknown",
        };

        // Get power draw (watts)
        let power = {
            let rate = battery.energy_rate();
            let watts = rate.get::<battery::units::power::watt>();
            if watts.abs() > 0.1 {
                // Format with sign: +45W (charging) or -15W (discharging)
                Some(format!("{:+.1}W", watts))
            } else {
                None
            }
        };

        Some((percentage, status.to_string(), power))
    }
}

impl Provider for BatteryProvider {
    fn name(&self) -> &str {
        "battery"
    }

    fn sections(&self) -> Vec<&str> {
        vec!["battery"]
    }

    fn collect(&self, _config: &Config, _validate: bool) -> ProviderResult<HashMap<String, String>> {
        let mut vars = HashMap::new();

        // Get battery info if available
        // Returns empty vars if no battery (common for desktops)
        if let Some((percentage, status, power)) = self.get_battery_info() {
            vars.insert("battery_percentage".to_string(), format!("{}%", percentage));
            vars.insert("battery_status".to_string(), status);

            // Add power draw if available
            if let Some(power_str) = power {
                vars.insert("battery_power".to_string(), power_str);
            }
        }

        Ok(vars)
    }

    fn default_config(&self) -> HashMap<String, Value> {
        let mut defaults = HashMap::new();
        defaults.insert("battery".to_string(), json!({}));
        defaults
    }

    fn cacheable(&self) -> bool {
        // Battery status changes slowly, can be cached
        true
    }

    fn cache_duration(&self) -> u64 {
        // Cache for 30 seconds (battery doesn't change that quickly)
        30
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_battery_provider_creation() {
        let provider = BatteryProvider::new();
        assert_eq!(provider.name(), "battery");
        assert_eq!(provider.sections(), vec!["battery"]);
        assert!(provider.cacheable());
        assert_eq!(provider.cache_duration(), 30);
    }

    #[test]
    fn test_default_config() {
        let provider = BatteryProvider::new();
        let defaults = provider.default_config();

        assert!(defaults.contains_key("battery"));
    }

    #[test]
    fn test_battery_info_format() {
        let provider = BatteryProvider::new();

        // This test will only pass on systems with a battery
        // On desktops, it will return None which is expected
        if let Some((percentage, status, power)) = provider.get_battery_info() {
            // Check percentage is in valid range
            assert!(percentage <= 100);

            // Check status is one of the known states
            let valid_states = vec!["Charging", "Discharging", "Full", "Empty", "Unknown"];
            assert!(valid_states.contains(&status.as_str()));

            // If power is present, check format
            if let Some(power_str) = power {
                // Should contain 'W' for watts
                assert!(power_str.contains('W'));
                // Should start with + or -
                assert!(power_str.starts_with('+') || power_str.starts_with('-'));
            }
        }
    }
}
