use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use crate::variable::{ConfigWithName, VariableProvider};

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
    pub name: Option<String>,
    pub shorten: bool,
    #[serde(default = "default_error")]
    pub error: String,
}

fn default_error() -> String {
    String::new()
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

impl ConfigWithName for Config {
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn error(&self) -> &str {
        &self.error
    }
}

pub struct CwdProvider;

impl VariableProvider for CwdProvider {
    type Error = CwdError;
    type Config = Config;

    fn get_value(config: &Self::Config) -> Result<String, Self::Error> {
        get_cwd(config)
    }

    fn section_name() -> &'static str {
        "cwd"
    }
} 
