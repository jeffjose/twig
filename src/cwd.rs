use crate::variable::{ConfigWithName, VariableProvider};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

// Helper function to get all available variables for the current path
fn get_cwd_variables(path: &std::path::Path) -> Result<HashMap<String, String>, CwdError> {
    let mut vars = HashMap::new();

    // Full path
    let full_path = path
        .to_str()
        .map(String::from)
        .ok_or_else(|| CwdError::ToString(path.to_path_buf().into_os_string()))?;
    vars.insert("cwd".to_string(), full_path);

    // Short version (current directory name)
    let short_path = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(String::from)
        .unwrap_or_else(|| ".".to_string());
    vars.insert("cwd_short".to_string(), short_path);

    // Add parent directory variable
    vars.insert("cwd_parent".to_string(), get_cwd_parent(path));

    // Add home-relative path variable
    vars.insert("cwd_home".to_string(), get_cwd_home(path));

    Ok(vars)
}

pub fn get_cwd(config: &Config) -> Result<String, CwdError> {
    let path = env::current_dir().map_err(CwdError::GetCwd)?;
    let vars = get_cwd_variables(&path)?;

    // Replace all variables in the format string
    let mut result = config.format.clone();
    for (var_name, value) in vars {
        result = result.replace(&format!("{{{}}}", var_name), &value);
    }

    Ok(result)
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

// Update your existing format_cwd function to handle the new variables
pub fn format_cwd(format: &str, path: &Path) -> String {
    let mut result = format.to_string();

    // Replace existing variables
    if format.contains("{cwd}") {
        result = result.replace("{cwd}", &path.to_string_lossy());
    }
    if format.contains("{cwd_short}") {
        result = result.replace(
            "{cwd_short}",
            &path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        );
    }

    // Add new variables
    if format.contains("{cwd_parent}") {
        result = result.replace("{cwd_parent}", &get_cwd_parent(path));
    }
    if format.contains("{cwd_home}") {
        result = result.replace("{cwd_home}", &get_cwd_home(path));
    }

    result
}
