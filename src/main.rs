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
use time::{format_current_time, Config as TimeConfig};

mod template;
use template::{format_template, parse_template_var};

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
            let mut var_spec = String::new();
            while let Some(&next_char) = chars.peek() {
                if next_char == '}' {
                    chars.next(); // consume '}'
                    if !var_spec.is_empty() {
                        // Use the shared parsing logic
                        let (var_name, _) = parse_template_var(&format!("${}", var_spec));
                        // Remove the $ prefix we just added
                        env_vars.push(var_name[1..].to_string());
                    }
                    break;
                }
                var_spec.push(chars.next().unwrap());
            }
        }
    }
    env_vars
}

// Add this helper function near the other helper functions
fn format_uses_named_variable(format: &str, section: &str, name: &str) -> bool {
    let var_name = if name.is_empty() {
        section.to_string()
    } else {
        name.to_string()
    };
    format_uses_variable(format, &var_name)
}

// Add this helper function to print debug info about variable usage
fn debug_variable_usage(format: &str, section: &str, var_name: &str, validate: bool) {
    if validate {
        if format_uses_variable(format, var_name) {
            eprintln!(
                "Debug: Will process {} section for variable '{}'",
                section, var_name
            );
        } else {
            eprintln!(
                "Debug: Skipping {} section - variable '{}' not used",
                section, var_name
            );
        }
    }
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
        let format_clone = prompt_format.clone();
        tasks.push(tokio::spawn(async move {
            let start = Instant::now();
            let mut time_vars = Vec::new();
            for (i, time_config) in config_clone.time.iter().enumerate() {
                let var_name = get_var_name(time_config, "time", i);
                debug_variable_usage(&format_clone, "time", &var_name, validate);
                if format_uses_variable(&format_clone, &var_name) {
                    let time = match format_current_time(&time_config.format) {
                        Ok(time) => time,
                        Err(e) => {
                            if validate {
                                eprintln!("Warning: couldn't format time: {}", e);
                            }
                            time_config.error.clone()
                        }
                    };
                    time_vars.push((var_name, time));
                }
            }
            (time_vars, start.elapsed())
        }));
        task_names.push("Time variables");

        // Handle hostname variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        let validate_clone = validate;
        tasks.push(tokio::spawn(async move {
            let start = Instant::now();
            let mut hostname_vars = Vec::new();
            for (i, hostname_config) in config_clone.hostname.iter().enumerate() {
                let var_name = get_var_name(hostname_config, "hostname", i);
                debug_variable_usage(&format_clone, "hostname", &var_name, validate_clone);
                if format_uses_variable(&format_clone, &var_name) {
                    let hostname = match hostname::get_hostname(hostname_config) {
                        Ok(hostname) => hostname,
                        Err(e) => {
                            if validate {
                                eprintln!("Warning: couldn't get hostname: {}", e);
                            }
                            hostname_config.error.clone()
                        }
                    };
                    hostname_vars.push((var_name, hostname));
                }
            }
            (hostname_vars, start.elapsed())
        }));
        task_names.push("Hostname variables");

        // Handle IP variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        let validate_clone = validate;
        tasks.push(tokio::spawn(async move {
            let start = Instant::now();
            let mut ip_vars = Vec::new();
            for (i, ip_config) in config_clone.ip.iter().enumerate() {
                let var_name = get_var_name(ip_config, "ip", i);
                debug_variable_usage(&format_clone, "ip", &var_name, validate_clone);
                if format_uses_variable(&format_clone, &var_name) {
                    let ip = match ip::get_ip(ip_config) {
                        Ok(ip) => ip.to_string(),
                        Err(e) => {
                            if validate {
                                eprintln!("Warning: couldn't get IP: {}", e);
                            }
                            ip_config.error.clone()
                        }
                    };
                    ip_vars.push((var_name, ip));
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
                debug_variable_usage(&format_clone, "cwd", &var_name, validate);
                if format_uses_variable(&format_clone, &var_name) {
                    let dir = match cwd::get_cwd(cwd_config) {
                        Ok(dir) => dir,
                        Err(e) => {
                            if validate {
                                eprintln!("Warning: couldn't get current directory: {}", e);
                            }
                            cwd_config.error.clone()
                        }
                    };
                    cwd_vars.push((var_name, dir));
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
            .map(|(name, value): &(String, String)| (name.as_str(), value.as_str()))
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
            eprintln!("\nTiming information:");
            eprintln!("  Config loading: {:?}", config_duration);
            eprintln!("  Variable gathering (total): {:?}", vars_duration);
            eprintln!("    Parallel task timings:");
            for (name, duration) in task_timings {
                eprintln!("      {}: {:?}", name, duration);
            }
            eprintln!("  Template formatting: {:?}", template_start.elapsed());
            eprintln!("  Total time: {:?}", start.elapsed());
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
