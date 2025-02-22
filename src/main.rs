use clap::Parser;
use directories::BaseDirs;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use serde_json;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio;

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

fn get_env_vars_from_format(format: &str) -> Vec<String> {
    let mut env_vars = Vec::new();
    let mut chars = format.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'$') {
            chars.next(); // consume $
            let mut var_name = String::new();
            while let Some(&next_char) = chars.peek() {
                if next_char == '}' || next_char == ':' {
                    // If we hit a color specification or end, stop collecting the var name
                    if next_char == ':' {
                        // Skip over the color specification until we find '}'
                        while let Some(&c) = chars.peek() {
                            chars.next();
                            if c == '}' {
                                break;
                            }
                        }
                    } else {
                        chars.next(); // consume the '}'
                    }
                    if !var_name.is_empty() {
                        env_vars.push(var_name);
                    }
                    break;
                }
                var_name.push(chars.next().unwrap());
            }
        }
    }
    env_vars
}

#[tokio::main]
async fn main() {
    let start = Instant::now();
    let cli = Cli::parse();

    let result: Result<(), Box<dyn Error>> = (|| async {
        // Get config path and ensure it exists
        let config_path = get_config_path(&cli.config)?;
        ensure_config_exists(&config_path)?;

        // Time the config loading
        let config_start = Instant::now();
        let config = load_config(&config_path)?;
        let config_duration = config_start.elapsed();

        // Time the variable gathering
        let vars_start = Instant::now();

        // Create shared config and cli references
        let config = Arc::new(config);
        let validate = cli.validate;
        let prompt_format = config.prompt.format.clone();

        // Create parallel tasks for each config section
        let mut tasks = Vec::new();
        let mut task_names = Vec::new();

        // Handle time variables
        let config_clone = Arc::clone(&config);
        tasks.push(tokio::spawn(async move {
            let start = Instant::now();
            let mut time_vars = Vec::new();
            for (i, time_config) in config_clone.time.iter().enumerate() {
                match format_current_time(&time_config.format) {
                    Ok(time) => {
                        let var_name = get_var_name(time_config, "time", i);
                        time_vars.push((var_name, time));
                    }
                    Err(e) => {
                        if validate {
                            eprintln!("Warning: couldn't format time: {}", e);
                        }
                    }
                }
            }
            (time_vars, start.elapsed())
        }));
        task_names.push("Time variables");

        // Handle hostname variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        tasks.push(tokio::spawn(async move {
            let start = Instant::now();
            let mut hostname_vars = Vec::new();
            for (i, hostname_config) in config_clone.hostname.iter().enumerate() {
                let var_name = get_var_name(hostname_config, "hostname", i);
                if format_uses_variable(&format_clone, &var_name) {
                    match hostname::get_hostname(hostname_config) {
                        Ok(hostname) => {
                            hostname_vars.push((var_name, hostname));
                        }
                        Err(e) => {
                            if validate {
                                eprintln!("Warning: couldn't get hostname: {}", e);
                            }
                        }
                    }
                }
            }
            (hostname_vars, start.elapsed())
        }));
        task_names.push("Hostname variables");

        // Handle IP variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        tasks.push(tokio::spawn(async move {
            let start = Instant::now();
            let mut ip_vars = Vec::new();
            for (i, ip_config) in config_clone.ip.iter().enumerate() {
                let var_name = get_var_name(ip_config, "ip", i);
                if format_uses_variable(&format_clone, &var_name) {
                    match ip::get_ip(ip_config) {
                        Ok(ip) => {
                            ip_vars.push((var_name, ip.to_string()));
                        }
                        Err(e) => {
                            if validate {
                                eprintln!("Warning: couldn't get IP: {}", e);
                            }
                        }
                    }
                }
            }
            (ip_vars, start.elapsed())
        }));
        task_names.push("IP variables");

        // Handle CWD variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        tasks.push(tokio::spawn(async move {
            let start = Instant::now();
            let mut cwd_vars = Vec::new();
            for (i, cwd_config) in config_clone.cwd.iter().enumerate() {
                let var_name = get_var_name(cwd_config, "cwd", i);
                if format_uses_variable(&format_clone, &var_name) {
                    match cwd::get_cwd(cwd_config) {
                        Ok(dir) => {
                            cwd_vars.push((var_name, dir));
                        }
                        Err(e) => {
                            if validate {
                                eprintln!("Warning: couldn't get current directory: {}", e);
                            }
                        }
                    }
                }
            }
            (cwd_vars, start.elapsed())
        }));
        task_names.push("CWD variables");

        // Handle environment variables
        let format_clone = prompt_format.clone();
        tasks.push(tokio::spawn(async move {
            let start = Instant::now();
            let mut env_vars = Vec::new();

            for var_name in get_env_vars_from_format(&format_clone) {
                if let Ok(value) = env::var(&var_name) {
                    env_vars.push((format!("${}", var_name), value));
                }
            }

            (env_vars, start.elapsed())
        }));
        task_names.push("Environment variables");

        // Wait for all tasks to complete and combine results
        let results = join_all(tasks).await;
        let mut variables = Vec::new();
        let mut task_timings = Vec::new();

        for (result, task_name) in results.into_iter().zip(task_names.iter()) {
            match result {
                Ok((mut vars, duration)) => {
                    variables.append(&mut vars);
                    task_timings.push((task_name, duration));
                }
                Err(e) => {
                    if validate {
                        eprintln!("Warning: task failed: {}", e);
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

        let template_start = Instant::now();
        let output = format_template(
            &config.prompt.format,
            &template_vars,
            validate,
            cli.mode.as_deref(),
        )?;
        println!("{}", output);

        if cli.timing {
            let total_duration = start.elapsed();
            let total_nanos = total_duration.as_nanos() as f64;

            eprintln!("\nTiming information:");
            eprintln!("  Config loading: {:?} ({:.1}%)", config_duration, 
                (config_duration.as_nanos() as f64 / total_nanos * 100.0));
            eprintln!("  Variable gathering (total): {:?} ({:.1}%)", vars_duration,
                (vars_duration.as_nanos() as f64 / total_nanos * 100.0));
            eprintln!("    Parallel task timings:");
            for (name, duration) in task_timings {
                eprintln!("      {}: {:?} ({:.1}%)", name, duration,
                    (duration.as_nanos() as f64 / total_nanos * 100.0));
            }
            eprintln!("  Template formatting: {:?} ({:.1}%)", template_start.elapsed(),
                (template_start.elapsed().as_nanos() as f64 / total_nanos * 100.0));
            eprintln!("  Total time: {:?}", total_duration);
        }

        Ok(())
    })()
    .await;

    if let Err(e) = result {
        if cli.validate {
            eprintln!("Error: {}", e);
        }
        std::process::exit(1);
    }
}
