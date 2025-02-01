use directories::BaseDirs;
use serde::Deserialize;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;

mod time;
use time::{format_current_time, TimeConfig};

#[derive(Debug)]
enum ConfigError {
    IoError(std::io::Error),
    TomlError(toml::de::Error),
    InvalidTimeFormat(String),
    NoConfigDir,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::IoError(e) => write!(f, "Failed to read config file: {}", e),
            ConfigError::TomlError(e) => write!(f, "Failed to parse config file: {}", e),
            ConfigError::InvalidTimeFormat(fmt) => write!(f, "Invalid time format string: {}", fmt),
            ConfigError::NoConfigDir => write!(f, "Could not determine config directory"),
        }
    }
}

impl Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(err: std::io::Error) -> Self {
        ConfigError::IoError(err)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(err: toml::de::Error) -> Self {
        ConfigError::TomlError(err)
    }
}

#[derive(Deserialize, Default)]
struct Config {
    #[serde(default)]
    time: TimeConfig,
}

fn get_config_path() -> Result<PathBuf, ConfigError> {
    BaseDirs::new()
        .map(|base_dirs| base_dirs.config_dir().join("twig").join("config.toml"))
        .ok_or(ConfigError::NoConfigDir)
}

fn validate_time_format(format: &str) -> Result<(), ConfigError> {
    format_current_time(format)
        .map(|_| ())
        .map_err(|_| ConfigError::InvalidTimeFormat(format.to_string()))
}

fn load_config() -> Result<Config, ConfigError> {
    let config_path = get_config_path()?;

    if !config_path.exists() {
        let config = Config::default();
        validate_time_format(&config.time.time_format)?;
        return Ok(config);
    }

    let content = fs::read_to_string(config_path)?;
    let config: Config = toml::from_str(&content)?;
    validate_time_format(&config.time.time_format)?;
    Ok(config)
}

fn main() {
    match load_config() {
        Ok(config) => match format_current_time(&config.time.time_format) {
            Ok(formatted_time) => println!("Current time: {}", formatted_time),
            Err(e) => {
                eprintln!("Error formatting time: {}", e);
                std::process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
