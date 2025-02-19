use clap::Parser;
use directories::BaseDirs;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use serde_json;
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
use template::format_template;

mod hostname;
mod ip;
use hostname::Config as HostnameConfig;
use ip::Config as IpConfig;

mod cwd;
use cwd::Config as CwdConfig;

mod template_test;

mod variable;
use variable::{process_section, ProcessingResult};

mod env_var;

mod env_var_test;

mod cwd_test;

mod hostname_test;

mod ip_test;

mod time_test;

mod git;
use git::Config as GitConfig;

mod git_test;

#[derive(Parser)]
#[command(version, about = "A configurable time display utility")]
struct Cli {
    /// Show timing information for each step
    #[arg(short, long)]
    timing: bool,

    /// Use an alternate config file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Output mode (e.g. 'tcsh', 'tcsh_debug')
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
    #[serde(default)]
    git: Vec<GitConfig>,
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
    validate_section_names(&config.git, "git")?;

    validate_time_format(&config.time[0].format)?;
    Ok(config)
}

fn get_env_vars_from_format(format: &str) -> Vec<String> {
    let mut env_vars = Vec::new();
    let mut chars = format.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'$') {
            chars.next(); // consume $
            let mut var_spec = String::new();
            while let Some(&next_char) = chars.peek() {
                if next_char == '}' || next_char == ':' {
                    // Add check for color separator
                    if !var_spec.is_empty() {
                        env_vars.push(var_spec);
                    }
                    // Skip to closing brace
                    while let Some(&c) = chars.peek() {
                        chars.next();
                        if c == '}' {
                            break;
                        }
                    }
                    break;
                }
                var_spec.push(chars.next().unwrap());
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

        let vars_start = Instant::now();

        // Create shared config and cli references
        let config = Arc::new(config);
        let validate = cli.validate;
        let prompt_format = config.prompt.format.clone();

        // Create parallel tasks for each config section
        let mut tasks = Vec::new();
        let mut task_names = Vec::new();
        let mut section_counts = Vec::new();

        // Handle time variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        section_counts.push(config_clone.time.len());
        tasks.push(tokio::spawn(async move {
            process_section::<time::TimeProvider>(&config_clone.time, &format_clone, validate).await
        }));
        task_names.push("Time variables");

        // Handle hostname variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        section_counts.push(config_clone.hostname.len());
        tasks.push(tokio::spawn(async move {
            process_section::<hostname::HostnameProvider>(
                &config_clone.hostname,
                &format_clone,
                validate,
            )
            .await
        }));
        task_names.push("Hostname variables");

        // Handle IP variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        section_counts.push(config_clone.ip.len());
        tasks.push(tokio::spawn(async move {
            process_section::<ip::IpProvider>(&config_clone.ip, &format_clone, validate).await
        }));
        task_names.push("IP variables");

        // Handle CWD variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        section_counts.push(config_clone.cwd.len());
        tasks.push(tokio::spawn(async move {
            process_section::<cwd::CwdProvider>(&config_clone.cwd, &format_clone, validate).await
        }));
        task_names.push("CWD variables");

        // Handle git variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        section_counts.push(config_clone.git.len());
        tasks.push(tokio::spawn(async move {
            process_section::<git::GitProvider>(&config_clone.git, &format_clone, validate).await
        }));
        task_names.push("Git variables");

        // Handle environment variables
        let format_clone = prompt_format.clone();
        let env_vars = get_env_vars_from_format(&format_clone);
        section_counts.push(env_vars.len());
        tasks.push(tokio::spawn(async move {
            let mut configs = Vec::new();
            if validate {
                eprintln!("Found environment variables: {:?}", env_vars);
            }
            for var_name in env_vars {
                configs.push(env_var::Config {
                    name: format!("${}", var_name),
                    error: String::new(),
                });
            }
            process_section::<env_var::EnvProvider>(&configs, &format_clone, validate).await
        }));
        task_names.push("Environment variables");

        // Wait for all tasks to complete and combine results
        let results = join_all(tasks).await;
        let mut all_variables = Vec::new();
        let mut task_timings = Vec::new();
        let mut processed_vars = Vec::new();

        for ((result, task_name), section_count) in results
            .into_iter()
            .zip(task_names.iter())
            .zip(section_counts.iter())
        {
            match result {
                Ok(ProcessingResult {
                    mut variables,
                    duration,
                }) => {
                    processed_vars.push(variables.len());
                    all_variables.append(&mut variables);
                    task_timings.push((task_name, duration, *section_count));
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
        let template_vars: Vec<(&str, &str)> = all_variables
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
            let total_time = start.elapsed();
            let total_nanos = total_time.as_nanos() as f64;

            eprintln!("\nTiming information:");
            eprintln!(
                "  Config loading: {:?} ({:.1}%)",
                config_duration,
                (config_duration.as_nanos() as f64 / total_nanos * 100.0)
            );
            eprintln!(
                "  Variable gathering (total): {:?} ({:.1}%)",
                vars_duration,
                (vars_duration.as_nanos() as f64 / total_nanos * 100.0)
            );
            eprintln!("    Parallel task timings:");

            // Sort timings by duration for better visibility
            let mut timing_data: Vec<_> = task_timings.iter().zip(processed_vars.iter()).collect();
            timing_data.sort_by_key(|((_, duration, _), _)| std::cmp::Reverse(*duration));

            for ((name, duration, config_count), processed_count) in timing_data {
                let percentage = duration.as_nanos() as f64 / total_nanos * 100.0;
                eprintln!(
                    "      {} ({} configs, {} vars processed): {:?} ({:.1}%){}",
                    name,
                    config_count,
                    processed_count,
                    duration,
                    percentage,
                    if percentage > 20.0 { " ⚠️" } else { "" } // Flag high-impact operations
                );
            }
            eprintln!(
                "  Template formatting: {:?} ({:.1}%)",
                template_start.elapsed(),
                (template_start.elapsed().as_nanos() as f64 / total_nanos * 100.0)
            );
            eprintln!("  Total time: {:?}", total_time);
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
