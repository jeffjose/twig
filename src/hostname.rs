use hostname;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug)]
pub enum HostnameError {
    Lookup(std::io::Error),
}

impl std::fmt::Display for HostnameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostnameError::Lookup(e) => write!(f, "Failed to get hostname: {}", e),
        }
    }
}

impl Error for HostnameError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // Hostname-specific config options will go here
    pub name: Option<String>,
    #[serde(default)]
    pub deferred: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: None,
            deferred: false,
        }
    }
}

pub fn get_hostname(_config: &Config) -> Result<String, HostnameError> {
    hostname::get()
        .map_err(HostnameError::Lookup)
        .map(|os_string| os_string.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_hostname_retrieval() {
        let config = Config::default();
        let result = get_hostname(&config);
        assert!(result.is_ok());

        // The hostname should not be empty
        let hostname = result.unwrap();
        assert!(!hostname.is_empty());

        // Should match system hostname
        let system_hostname = hostname::get().unwrap().to_string_lossy().into_owned();
        assert_eq!(hostname, system_hostname);
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.name, None);
    }

    #[test]
    fn test_config_with_name() {
        let config = Config {
            name: Some("host".to_string()),
            deferred: false,
        };
        assert_eq!(config.name, Some("host".to_string()));
    }

    #[test]
    fn test_hostname_error_display() {
        let error =
            HostnameError::Lookup(std::io::Error::new(std::io::ErrorKind::Other, "test error"));
        assert_eq!(error.to_string(), "Failed to get hostname: test error");
    }

    #[test]
    fn test_hostname_matches_env() {
        if let Ok(env_hostname) = env::var("HOSTNAME") {
            let config = Config::default();
            let result = get_hostname(&config).unwrap();
            assert_eq!(result, env_hostname);
        }
    }

    #[test]
    fn test_get_hostname() {
        let config = Config {
            name: None,
            deferred: false,
        };
        let result = get_hostname(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_deferred_config() {
        let config = Config {
            name: Some("host".to_string()),
            deferred: true,
        };
        assert!(config.deferred);
        assert_eq!(config.name, Some("host".to_string()));
    }

    #[test]
    fn test_deferred_default() {
        let config = Config::default();
        assert!(!config.deferred, "deferred should be false by default");
    }
}
