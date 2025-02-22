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

#[derive(Deserialize, Serialize, Default)]
pub struct Config {
    #[serde(default)]
    pub shorten: bool,  // If true, show only the last component
    pub name: Option<String>,  // Add name field
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
    fn test_shorten_path() {
        let config = Config {
            shorten: true,
            name: Some("dir".to_string()),
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
        };

        let result = get_cwd(&config).unwrap();
        assert!(result.starts_with("/"));  // Should be absolute path
    }

    #[test]
    fn test_root_path() {
        let config = Config {
            shorten: true,
            name: Some("dir".to_string()),
        };

        // Try with root directory
        std::env::set_current_dir("/").unwrap();
        let result = get_cwd(&config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "/");
    }
} 
