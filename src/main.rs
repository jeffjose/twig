use clap::Parser;
use directories::BaseDirs;
use serde::Deserialize;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

mod time;
use time::{format_current_time, TimeConfig};

mod template;
use template::format_template;

mod hostname;
mod ip;
use hostname::Config as HostnameConfig;
use ip::Config as IpConfig;

mod cwd;
use cwd::Config as CwdConfig;

#[derive(Parser)]
#[command(version, about = "A configurable time display utility")]
struct Cli {
    /// Show timing information for each step
    #[arg(short, long)]
    timing: bool,
}

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
    #[serde(default)]
    prompt: PromptConfig,
    #[serde(default)]
    hostname: HostnameConfig,
    #[serde(default)]
    ip: IpConfig,
    #[serde(default)]
    cwd: CwdConfig,
}

#[derive(Deserialize, Default)]
struct PromptConfig {
    #[serde(default = "default_format")]
    format: String,
}

fn default_format() -> String {
    "{time}".to_string()
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
    let start = Instant::now();
    let cli = Cli::parse();

    let result: Result<(), Box<dyn Error>> = (|| {
        // Time the config loading
        let config_start = Instant::now();
        let config = load_config()?;
        let config_duration = config_start.elapsed();

        // Time the time formatting
        let time_start = Instant::now();
        let formatted_time = format_current_time(&config.time.time_format)?;
        let time_duration = time_start.elapsed();

        // Time the template formatting
        let template_start = Instant::now();

        // Time the variable gathering
        let vars_start = Instant::now();

        // Collect all strings first
        let mut collected_strings = Vec::new();
        collected_strings.push(formatted_time);

        let mut hostname_idx = None;
        let mut ip_idx = None;
        let mut cwd_idx = None;

        if config.prompt.format.contains("{hostname}") {
            match hostname::get_hostname(&config.hostname) {
                Ok(hostname) => {
                    collected_strings.push(hostname);
                    hostname_idx = Some(collected_strings.len() - 1);
                }
                Err(e) => eprintln!("Warning: couldn't get hostname: {}", e),
            }
        }

        if config.prompt.format.contains("{ip}") {
            match ip::get_ip(&config.ip) {
                Ok(ip) => {
                    collected_strings.push(ip.to_string());
                    ip_idx = Some(collected_strings.len() - 1);
                }
                Err(e) => eprintln!("Warning: couldn't get IP: {}", e),
            }
        }

        if config.prompt.format.contains("{cwd}") {
            match cwd::get_cwd(&config.cwd) {
                Ok(dir) => {
                    collected_strings.push(dir);
                    cwd_idx = Some(collected_strings.len() - 1);
                }
                Err(e) => eprintln!("Warning: couldn't get current directory: {}", e),
            }
        }

        // Now build the variables vector
        let mut variables = vec![("time", collected_strings[0].as_str())];

        if let Some(idx) = hostname_idx {
            variables.push(("hostname", collected_strings[idx].as_str()));
        }

        if let Some(idx) = ip_idx {
            variables.push(("ip", collected_strings[idx].as_str()));
        }

        if let Some(idx) = cwd_idx {
            variables.push(("cwd", collected_strings[idx].as_str()));
        }

        let vars_duration = vars_start.elapsed();

        let output = format_template(&config.prompt.format, &variables)?;
        let template_duration = template_start.elapsed();

        println!("{}", output);

        if cli.timing {
            eprintln!("\nTiming information:");
            eprintln!("  Config loading: {:?}", config_duration);
            eprintln!("  Variable gathering: {:?}", vars_duration);
            eprintln!("  Time formatting: {:?}", time_duration);
            eprintln!("  Template formatting: {:?}", template_duration);
            eprintln!("  Total time: {:?}", start.elapsed());
        }

        Ok(())
    })();

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
