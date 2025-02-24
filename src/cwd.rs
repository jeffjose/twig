use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::ffi::OsString;

#[derive(Debug)]
pub enum CwdError {
    GetCwd(std::io::Error),
    ToString(OsString),
}

impl std::fmt::Display for CwdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CwdError::GetCwd(e) => write!(f, "Failed to get current directory: {}", e),
            CwdError::ToString(path) => write!(f, "Invalid characters in path: {:?}", path),
        }
    }
}

impl Error for CwdError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub name: Option<String>,
    #[serde(default)]
    pub shorten: bool,
    #[serde(default)]
    pub deferred: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: None,
            shorten: false,
            deferred: false,
        }
    }
}

pub fn get_cwd(config: &Config) -> Result<String, CwdError> {
    let path = env::current_dir().map_err(CwdError::GetCwd)?;

    if config.shorten {
        if path == std::path::Path::new("/") {
            Ok("/".to_string())
        } else {
            Ok(path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| String::from(".")))
        }
    } else {
        path.to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| CwdError::ToString(path.into_os_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_cwd() {
        let config = Config {
            name: None,
            shorten: false,
            deferred: false,
        };
        let result = get_cwd(&config).unwrap();
        assert!(result.starts_with("/")); // Should be absolute path
    }

    #[test]
    fn test_get_cwd_shortened() {
        let config = Config {
            name: None,
            shorten: true,
            deferred: false,
        };
        let result = get_cwd(&config).unwrap();
        assert!(!result.contains("/")); // Should be just the directory name
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.name, None);
        assert_eq!(config.shorten, false);
        assert_eq!(config.deferred, false);
    }

    #[test]
    fn test_shorten_path() {
        let config = Config {
            shorten: true,
            name: Some("dir".to_string()),
            deferred: false,
        };

        // Create a test directory and change into it
        let temp_dir = std::env::temp_dir().join("test_dir");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let result = get_cwd(&config).unwrap();
        assert_eq!(result, "test_dir");

        // Clean up
        std::env::set_current_dir(original_dir).unwrap();
        std::fs::remove_dir(&temp_dir).unwrap();
    }

    #[test]
    fn test_full_path() {
        let config = Config {
            shorten: false,
            name: Some("dir".to_string()),
            deferred: false,
        };

        let result = get_cwd(&config).unwrap();
        assert!(result.starts_with("/")); // Should be absolute path
    }

    #[test]
    fn test_root_path() {
        let config = Config {
            shorten: true,
            name: Some("dir".to_string()),
            deferred: false,
        };

        // Try with root directory
        std::env::set_current_dir("/").unwrap();
        let result = get_cwd(&config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "/");
    }

    #[test]
    fn test_deferred_config() {
        let config = Config {
            name: Some("dir".to_string()),
            shorten: true,
            deferred: true,
        };
        assert!(config.deferred);
        assert_eq!(config.name, Some("dir".to_string()));
        assert!(config.shorten);
    }

    #[test]
    fn test_deferred_default() {
        let config = Config::default();
        assert!(!config.deferred, "deferred should be false by default");
    }
}
