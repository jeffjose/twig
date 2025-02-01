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
        Ok(path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| String::from(".")))
    } else {
        path.to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| CwdError::ToString(path.into_os_string()))
    }
} 
