use clap::Parser;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use serde_json;
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

mod template_test;

#[derive(Parser)]
#[command(version, about = "A configurable time display utility")]
struct Cli {
    /// Show timing information for each step
    #[arg(short, long)]
    timing: bool,

    /// Use an alternate config file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Output mode (e.g. 'tcsh')
    #[arg(long)]
    mode: Option<String>,

    /// Show validation errors and warnings
    #[arg(long)]
    validate: bool,
}

#[derive(Debug)]
enum ConfigError {
    IoError(std::io::Error),
    TomlError(toml::de::Error),
    InvalidTimeFormat(String),
    NoConfigDir,
    EmptyConfigPath,
    MissingName(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::IoError(e) => write!(f, "Failed to read config file: {}", e),
            ConfigError::TomlError(e) => write!(f, "Failed to parse config file: {}", e),
            ConfigError::InvalidTimeFormat(fmt) => write!(f, "Invalid time format string: {}", fmt),
            ConfigError::NoConfigDir => write!(f, "Could not determine config directory"),
            ConfigError::EmptyConfigPath => write!(f, "Config path cannot be empty"),
            ConfigError::MissingName(section) => write!(
                f,
                "Multiple {} sections found but not all have 'name' parameter",
                section
            ),
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
    time: Vec<TimeConfig>,
    #[serde(default)]
    prompt: PromptConfig,
    #[serde(default)]
    hostname: Vec<HostnameConfig>,
    #[serde(default)]
    ip: Vec<IpConfig>,
    #[serde(default)]
    cwd: Vec<CwdConfig>,
}

#[derive(Deserialize, Default)]
struct PromptConfig {
    #[serde(default = "default_format")]
    format: String,
}

fn default_format() -> String {
    "{time}".to_string()
}

fn get_config_path(cli_config: &Option<PathBuf>) -> Result<PathBuf, ConfigError> {
    if let Some(path) = cli_config {
        if path.as_os_str().is_empty() {
            return Err(ConfigError::EmptyConfigPath);
        }
        Ok(path.clone())
    } else {
        BaseDirs::new()
            .map(|base_dirs| base_dirs.config_dir().join("twig").join("config.toml"))
            .ok_or(ConfigError::NoConfigDir)
    }
}

fn validate_time_format(format: &str) -> Result<(), ConfigError> {
    format_current_time(format)
        .map(|_| ())
        .map_err(|_| ConfigError::InvalidTimeFormat(format.to_string()))
}

fn ensure_config_exists(config_path: &PathBuf) -> Result<(), ConfigError> {
    // Create parent directories if they don't exist
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Create default config if it doesn't exist
    if !config_path.exists() {
        let default_config = r#"[[time]]
format = "%H:%M:%S"

[[time]]
name = "utc"
format = "%H:%M:%S UTC"

[[hostname]]
name = "short"
# Hostname-specific options could go here

[[ip]]
name = "local"
# IP-specific options could go here

[[cwd]]
name = "path"
shorten = false

[prompt]
format = "[{short:cyan}:{path:blue}] {time:green} ({utc:yellow})"
"#;
        fs::write(config_path, default_config)?;
    }
    Ok(())
}

fn validate_section_names<T>(configs: &[T], section_name: &str) -> Result<(), ConfigError>
where
    T: serde::de::DeserializeOwned + Default + Serialize,
{
    if configs.len() > 1 {
        // Check if any config in the section is missing a name
        for config in configs {
            #[derive(Deserialize)]
            struct NamedConfig {
                name: Option<String>,
            }

            let named: NamedConfig =
                match serde_json::to_value(config).and_then(|v| serde_json::from_value(v)) {
                    Ok(n) => n,
                    Err(_) => return Err(ConfigError::MissingName(section_name.to_string())),
                };

            if named.name.is_none() {
                return Err(ConfigError::MissingName(section_name.to_string()));
            }
        }
    }
    Ok(())
}

fn load_config(config_path: &PathBuf) -> Result<Config, ConfigError> {
    let content = fs::read_to_string(config_path)?;
    let config: Config = toml::from_str(&content)?;

    // Validate that multiple sections have names
    validate_section_names(&config.time, "time")?;
    validate_section_names(&config.hostname, "hostname")?;
    validate_section_names(&config.ip, "ip")?;
    validate_section_names(&config.cwd, "cwd")?;

    validate_time_format(&config.time[0].format)?;
    Ok(config)
}

// Helper function to check if a variable is used in the format string
fn format_uses_variable(format: &str, var_name: &str) -> bool {
    format.contains(&format!("{{{}", var_name))
}

// Helper function to get variable name for a config
fn get_var_name<T>(config: &T, section_name: &str, index: usize) -> String
where
    T: serde::de::DeserializeOwned + Default + Serialize,
{
    #[derive(Deserialize)]
    struct NamedConfig {
        name: Option<String>,
    }

    // Try to deserialize just the name field
    let named: NamedConfig =
        match serde_json::to_value(config).and_then(|v| serde_json::from_value(v)) {
            Ok(n) => n,
            Err(_) => {
                return if index == 0 {
                    section_name.to_string()
                } else {
                    format!("{}_{}", section_name, index + 1)
                }
            }
        };

    named.name.unwrap_or_else(|| {
        if index == 0 {
            section_name.to_string()
        } else {
            format!("{}_{}", section_name, index + 1)
        }
    })
}

fn main() {
    let start = Instant::now();
    let cli = Cli::parse();

    let result: Result<(), Box<dyn Error>> = (|| {
        // Get config path and ensure it exists
        let config_path = get_config_path(&cli.config)?;
        ensure_config_exists(&config_path)?;

        // Time the config loading
        let config_start = Instant::now();
        let config = load_config(&config_path)?;
        let config_duration = config_start.elapsed();

        // Time the time formatting
        let time_start = Instant::now();
        let _formatted_time = format_current_time(&config.time[0].format)?;
        let time_duration = time_start.elapsed();

        // Time the template formatting
        let template_start = Instant::now();

        // Time the variable gathering
        let vars_start = Instant::now();

        let mut variables = Vec::new();

        // Handle time variables
        for (i, time_config) in config.time.iter().enumerate() {
            match format_current_time(&time_config.format) {
                Ok(time) => {
                    let var_name = get_var_name(time_config, "time", i);
                    variables.push((var_name, time));
                }
                Err(e) => {
                    if cli.validate {
                        eprintln!("Warning: couldn't format time: {}", e);
                    }
                }
            }
        }

        // Handle hostname variables
        for (i, hostname_config) in config.hostname.iter().enumerate() {
            let var_name = get_var_name(hostname_config, "hostname", i);
            if format_uses_variable(&config.prompt.format, &var_name) {
                match hostname::get_hostname(hostname_config) {
                    Ok(hostname) => {
                        variables.push((var_name, hostname));
                    }
                    Err(e) => {
                        if cli.validate {
                            eprintln!("Warning: couldn't get hostname: {}", e);
                        }
                    }
                }
            }
        }

        // Handle IP variables
        for (i, ip_config) in config.ip.iter().enumerate() {
            let var_name = get_var_name(ip_config, "ip", i);
            if format_uses_variable(&config.prompt.format, &var_name) {
                match ip::get_ip(ip_config) {
                    Ok(ip) => {
                        variables.push((var_name, ip.to_string()));
                    }
                    Err(e) => {
                        if cli.validate {
                            eprintln!("Warning: couldn't get IP: {}", e);
                        }
                    }
                }
            }
        }

        // Handle CWD variables
        for (i, cwd_config) in config.cwd.iter().enumerate() {
            let var_name = get_var_name(cwd_config, "cwd", i);
            if format_uses_variable(&config.prompt.format, &var_name) {
                match cwd::get_cwd(cwd_config) {
                    Ok(dir) => {
                        variables.push((var_name, dir));
                    }
                    Err(e) => {
                        if cli.validate {
                            eprintln!("Warning: couldn't get current directory: {}", e);
                        }
                    }
                }
            }
        }

        let vars_duration = vars_start.elapsed();

        // Convert variables for template formatting
        let template_vars: Vec<(&str, &str)> = variables
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();

        let output = format_template(
            &config.prompt.format,
            &template_vars,
            cli.validate,
            cli.mode.as_deref(),
        )?;
        println!("{}", output);

        if cli.timing {
            eprintln!("\nTiming information:");
            eprintln!("  Config loading: {:?}", config_duration);
            eprintln!("  Variable gathering: {:?}", vars_duration);
            eprintln!("  Time formatting: {:?}", time_duration);
            eprintln!("  Template formatting: {:?}", template_start.elapsed());
            eprintln!("  Total time: {:?}", start.elapsed());
        }

        Ok(())
    })();

    if let Err(e) = result {
        if cli.validate {
            eprintln!("Error: {}", e);
        }
        std::process::exit(1);
    }
}
