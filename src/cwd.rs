use crate::variable::{replace_variables, ConfigWithName, LazyVariables, VariableProvider};
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::path::Path;

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
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_error")]
    pub error: String,
}

fn default_format() -> String {
    "{cwd}".to_string()
}

fn default_error() -> String {
    String::new()
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

impl LazyVariables for CwdProvider {
    type Error = CwdError;

    fn get_variable(name: &str) -> Result<String, Self::Error> {
        let path = env::current_dir().map_err(CwdError::GetCwd)?;

        match name {
            "cwd" => path
                .to_str()
                .map(String::from)
                .ok_or_else(|| CwdError::ToString(path.to_path_buf().into_os_string())),
            "cwd_short" => Ok(path
                .file_name()
                .and_then(|name| name.to_str())
                .map(String::from)
                .unwrap_or_else(|| ".".to_string())),
            "cwd_parent" => Ok(get_cwd_parent(&path)),
            "cwd_home" => Ok(get_cwd_home(&path)),
            _ => Err(CwdError::GetCwd(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Unknown variable",
            ))),
        }
    }

    fn variable_names() -> &'static [&'static str] {
        &["cwd", "cwd_short", "cwd_parent", "cwd_home"]
    }
}

pub fn get_cwd(config: &Config) -> Result<String, CwdError> {
    if !config.format.contains('{') {
        return Ok(config.format.clone());
    }

    let vars = CwdProvider::get_needed_variables(&config.format)?;
    Ok(replace_variables(&config.format, &vars))
}

impl ConfigWithName for Config {
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn error(&self) -> &str {
        &self.error
    }
}

pub fn get_cwd_parent(path: &Path) -> String {
    path.parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_string()
}

pub fn get_cwd_home(path: &Path) -> String {
    if let Ok(home) = env::var("HOME") {
        let home_path = Path::new(&home);
        if let Ok(stripped) = path.strip_prefix(home_path) {
            return format!("~/{}", stripped.display());
        }
    }
    path.to_string_lossy().into_owned()
}
