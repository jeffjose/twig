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
use std::process;
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

mod power;
use power::Config as PowerConfig;

mod colors;

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

    /// Show all available colors and styles
    #[arg(long)]
    colors: bool,

    /// Run as a daemon that collects and caches information
    #[arg(short = 'd', long = "daemon", alias = "daemon")]
    daemon: bool,

    /// When used with -d, run daemon in foreground instead of background
    #[arg(long = "fg", alias = "foreground")]
    fg: bool,
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
    power: Vec<PowerConfig>,
    #[serde(default)]
    daemon: DaemonConfig,
}

#[derive(Deserialize, Default)]
struct PromptConfig {
    #[serde(default = "default_format")]
    format: String,
}

#[derive(Deserialize)]
struct DaemonConfig {
    #[serde(
        default = "default_daemon_frequency",
        deserialize_with = "validate_daemon_frequency"
    )]
    frequency: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            frequency: default_daemon_frequency(),
        }
    }
}

fn validate_daemon_frequency<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = u64::deserialize(deserializer)?;
    if value == 0 {
        Ok(default_daemon_frequency())
    } else {
        Ok(value)
    }
}

fn default_format() -> String {
    "{time}".to_string()
}

fn default_daemon_frequency() -> u64 {
    1 // Default to 1 second
}

fn get_config_path(cli_config: &Option<PathBuf>) -> Result<PathBuf, ConfigError> {
    let path = if let Some(path) = cli_config {
        if path.as_os_str().is_empty() {
            return Err(ConfigError::EmptyConfigPath);
        }
        path.clone()
    } else {
        BaseDirs::new()
            .map(|base_dirs| base_dirs.config_dir().join("twig").join("config.toml"))
            .ok_or(ConfigError::NoConfigDir)?
    };
    Ok(path)
}

#[allow(dead_code)]
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

[prompt]
format = "{time}"

[daemon]
frequency = 1  # How often the daemon updates data (in seconds)
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
    validate_section_names(&config.power, "power")?;

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

// First, create structs to hold the raw data
// struct SystemData {
//     hostname: Result<String, hostname::HostnameError>,
//     ip: Result<IpAddr, ip::IpConfigError>,
//     cwd: Result<String, cwd::CwdError>,
//     // Add other expensive data here
// }

// Add timing structs
struct TimingData {
    fetch_time: std::time::Duration,
    format_time: std::time::Duration,
    fetch_count: usize,
    skip_count: usize,
    cached_count: usize,
}

// Add this at the top level
type TaskResult = Result<(Vec<(String, String)>, TimingData), Box<dyn Error + Send + Sync>>;

#[tokio::main]
async fn main() {
    let start = Instant::now();
    let cli = Cli::parse();

    if cli.colors {
        colors::print_color_test();
        return;
    }

    // Handle daemon mode
    if cli.daemon {
        if !cli.fg {
            // Fork to background if not in foreground mode
            match unsafe { libc::fork() } {
                -1 => {
                    eprintln!("Failed to fork process");
                    process::exit(1);
                }
                0 => {
                    // Child process continues
                }
                _ => {
                    // Parent process exits
                    process::exit(0);
                }
            }
        }

        // Run the daemon loop
        run_daemon(&cli).await;
        return;
    }

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
        let mut tasks: Vec<tokio::task::JoinHandle<TaskResult>> = Vec::new();
        let mut task_names = Vec::new();

        // Handle time variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        tasks.push(tokio::spawn(async move {
            let mut timing = TimingData {
                fetch_time: std::time::Duration::default(),
                format_time: std::time::Duration::default(),
                fetch_count: 0,
                skip_count: 0,
                cached_count: 0,
            };

            let format_start = Instant::now();
            let mut time_vars = Vec::new();
            for (i, time_config) in config_clone.time.iter().enumerate() {
                let var_name = get_var_name(time_config, "time", i);
                if format_uses_variable(&format_clone, &var_name) {
                    let fetch_start = Instant::now();
                    match format_current_time(&time_config.format) {
                        Ok(time) => {
                            timing.fetch_time += fetch_start.elapsed();
                            timing.fetch_count += 1;
                            time_vars.push((var_name, time));
                        }
                        Err(e) => {
                            if validate {
                                eprintln!("Warning: couldn't format time: {}", e);
                            }
                        }
                    }
                } else {
                    timing.skip_count += 1;
                }
            }
            timing.format_time = format_start.elapsed();

            Ok((time_vars, timing))
        }));
        task_names.push("Time variables");

        // Handle hostname variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        tasks.push(tokio::spawn(async move {
            let mut timing = TimingData {
                fetch_time: std::time::Duration::default(),
                format_time: std::time::Duration::default(),
                fetch_count: 0,
                skip_count: 0,
                cached_count: 0,
            };

            // Get hostname once - time the fetch
            let fetch_start = Instant::now();
            let hostname_data: Result<String, hostname::HostnameError> =
                hostname::get_hostname(&hostname::Config::default());
            timing.fetch_time = fetch_start.elapsed();
            timing.fetch_count = 1;

            // Time the formatting separately
            let format_start = Instant::now();
            let mut hostname_vars = Vec::new();
            for (i, hostname_config) in config_clone.hostname.iter().enumerate() {
                let var_name = get_var_name(hostname_config, "hostname", i);
                if format_uses_variable(&format_clone, &var_name) {
                    match &hostname_data {
                        Ok(hostname) => {
                            hostname_vars.push((var_name, hostname.clone()));
                        }
                        Err(e) => {
                            if validate {
                                eprintln!("Warning: couldn't get hostname: {}", e);
                            }
                        }
                    }
                } else {
                    timing.skip_count += 1;
                }
            }
            timing.format_time = format_start.elapsed();

            Ok((hostname_vars, timing))
        }));
        task_names.push("Hostname variables");

        // Handle IP variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        tasks.push(tokio::spawn(async move {
            let mut timing = TimingData {
                fetch_time: std::time::Duration::default(),
                format_time: std::time::Duration::default(),
                fetch_count: 0,
                skip_count: 0,
                cached_count: 0,
            };

            // Get IP data once - this is the expensive part
            let fetch_start = Instant::now();
            let ip_data = match &config_clone.ip.iter().find(|c| c.interface.is_some()) {
                Some(config) => ip::get_ip(config),
                None => local_ip_address::local_ip()
                    .map_err(|e| ip::IpConfigError::Lookup(e.to_string())),
            };
            timing.fetch_time = fetch_start.elapsed();
            timing.fetch_count = 1;

            // Format for each config - this is just string manipulation
            let format_start = Instant::now();
            let mut ip_vars = Vec::new();
            for (i, ip_config) in config_clone.ip.iter().enumerate() {
                let var_name = get_var_name(ip_config, "ip", i);
                if format_uses_variable(&format_clone, &var_name) {
                    match &ip_data {
                        Ok(ip) => {
                            ip_vars.push((var_name, ip.to_string()));
                        }
                        Err(e) => {
                            if validate {
                                eprintln!("Warning: couldn't get IP: {}", e);
                            }
                        }
                    }
                } else {
                    timing.skip_count += 1;
                }
            }
            timing.format_time = format_start.elapsed();

            Ok((ip_vars, timing))
        }));
        task_names.push("IP variables");

        // Handle CWD variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        tasks.push(tokio::spawn(async move {
            let mut timing = TimingData {
                fetch_time: std::time::Duration::default(),
                format_time: std::time::Duration::default(),
                fetch_count: 0,
                skip_count: 0,
                cached_count: 0,
            };

            let format_start = Instant::now();
            let mut cwd_vars = Vec::new();
            for (i, cwd_config) in config_clone.cwd.iter().enumerate() {
                let var_name = get_var_name(cwd_config, "cwd", i);
                if format_uses_variable(&format_clone, &var_name) {
                    let fetch_start = Instant::now();
                    match cwd::get_cwd(cwd_config) {
                        Ok(dir) => {
                            timing.fetch_time += fetch_start.elapsed();
                            timing.fetch_count += 1;
                            cwd_vars.push((var_name, dir));
                        }
                        Err(e) => {
                            if validate {
                                eprintln!("Warning: couldn't get current directory: {}", e);
                            }
                        }
                    }
                } else {
                    timing.skip_count += 1;
                }
            }
            timing.format_time = format_start.elapsed();

            Ok((cwd_vars, timing))
        }));
        task_names.push("CWD variables");

        // Handle power variables
        let config_clone = Arc::clone(&config);
        let format_clone = prompt_format.clone();
        tasks.push(tokio::spawn(async move {
            let mut timing = TimingData {
                fetch_time: std::time::Duration::default(),
                format_time: std::time::Duration::default(),
                fetch_count: 0,
                skip_count: 0,
                cached_count: 0,
            };

            let mut power_vars = Vec::new();
            let fetch_start = Instant::now();

            // Get battery info once
            if !config_clone.power.is_empty() {
                let _power_config = &config_clone.power[0];
                let battery_info = power::get_battery_info_internal();
                timing.fetch_time = fetch_start.elapsed();
                timing.fetch_count = 1;

                let _format_start = Instant::now();

                // Pre-format common values once
                if let Ok(info) = &battery_info {
                    for (i, power_config) in config_clone.power.iter().enumerate() {
                        let var_name = get_var_name(power_config, "power", i);
                        if format_uses_variable(&format_clone, &var_name) {
                            let formatted = power_config
                                .format
                                .replace("{percentage}", &info.percentage.to_string())
                                .replace("{status}", &info.status)
                                .replace("{time_left}", &info.time_left)
                                .replace(
                                    "{power_now}",
                                    &if info.power_now.abs() < 0.01 {
                                        "0.0".to_string()
                                    } else {
                                        format!("{:+.1}", info.power_now)
                                    },
                                )
                                .replace("{energy_now}", &format!("{:.1}", info.energy_now))
                                .replace("{energy_full}", &format!("{:.1}", info.energy_full))
                                .replace("{voltage}", &format!("{:.1}", info.voltage))
                                .replace("{temperature}", &format!("{:.1}", info.temperature))
                                .replace("{capacity}", &info.capacity.to_string())
                                .replace("{cycle_count}", &info.cycle_count.to_string())
                                .replace("{technology}", &info.technology)
                                .replace("{manufacturer}", &info.manufacturer)
                                .replace("{model}", &info.model)
                                .replace("{serial}", &info.serial);
                            power_vars.push((var_name, formatted));
                        } else {
                            timing.skip_count += 1;
                        }
                    }
                } else if let Err(e) = &battery_info {
                    if validate {
                        eprintln!("Warning: couldn't get battery info: {}", e);
                    }
                }
            }
            timing.format_time = fetch_start.elapsed();

            Ok((power_vars, timing))
        }));
        task_names.push("Power variables");

        // Handle environment variables
        let format_clone = prompt_format.clone();
        tasks.push(tokio::spawn(async move {
            let mut timing = TimingData {
                fetch_time: std::time::Duration::default(),
                format_time: std::time::Duration::default(),
                fetch_count: 0,
                skip_count: 0,
                cached_count: 0,
            };

            let format_start = Instant::now();
            let mut env_vars = Vec::new();
            for var_name in get_env_vars_from_format(&format_clone) {
                let fetch_start = Instant::now();
                if let Ok(value) = env::var(&var_name) {
                    timing.fetch_time += fetch_start.elapsed();
                    timing.fetch_count += 1;
                    env_vars.push((format!("${}", var_name), value));
                } else {
                    timing.skip_count += 1;
                }
            }
            timing.format_time = format_start.elapsed();

            Ok((env_vars, timing))
        }));
        task_names.push("Environment variables");

        // Wait for all tasks to complete and combine results
        let results = join_all(tasks).await;
        let mut variables = Vec::new();
        let mut task_timings = Vec::new();

        for (result, task_name) in results.into_iter().zip(task_names.iter()) {
            match result {
                Ok(Ok((mut vars, timing))) => {
                    variables.append(&mut vars);
                    task_timings.push((task_name, timing));
                }
                Ok(Err(e)) => {
                    if validate {
                        eprintln!("Warning: task failed: {}", e);
                    }
                }
                Err(e) => {
                    if validate {
                        eprintln!("Warning: task panicked: {}", e);
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
            eprintln!("    Parallel task details (fastest to slowest):");

            // Sort task timings by total time (fetch + format)
            let mut sorted_timings: Vec<_> = task_timings.into_iter().collect();
            sorted_timings.sort_by(|(_, a), (_, b)| {
                let a_total = a.fetch_time + a.format_time;
                let b_total = b.fetch_time + b.format_time;
                a_total.cmp(&b_total)
            });

            for (name, timing_data) in sorted_timings {
                let total_time = timing_data.fetch_time + timing_data.format_time;
                eprintln!("      {}: ", name);
                eprintln!(
                    "        Data fetch ({} processed, {} cached, {} skipped): {:?} ({:.1}%)",
                    timing_data.fetch_count - timing_data.cached_count,
                    timing_data.cached_count,
                    timing_data.skip_count,
                    timing_data.fetch_time,
                    (timing_data.fetch_time.as_nanos() as f64 / total_nanos * 100.0)
                );
                eprintln!(
                    "        Formatting: {:?} ({:.1}%)",
                    timing_data.format_time,
                    (timing_data.format_time.as_nanos() as f64 / total_nanos * 100.0)
                );
                eprintln!(
                    "        Total: {:?} ({:.1}%)",
                    total_time,
                    (total_time.as_nanos() as f64 / total_nanos * 100.0)
                );
            }

            eprintln!(
                "  Template formatting: {:?} ({:.1}%)",
                template_start.elapsed(),
                (template_start.elapsed().as_nanos() as f64 / total_nanos * 100.0)
            );
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

async fn run_daemon(cli: &Cli) {
    println!("Starting twig daemon...");

    // Get config path and ensure it exists
    let config_path = match get_config_path(&cli.config) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Failed to get config path: {}", e);
            process::exit(1);
        }
    };

    ensure_config_exists(&config_path).unwrap_or_else(|e| {
        eprintln!("Failed to ensure config exists: {}", e);
        process::exit(1);
    });

    let config = match load_config(&config_path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load config: {}", e);
            process::exit(1);
        }
    };

    let config = Arc::new(config);

    // Ensure data.json parent directory exists
    if let Some(parent) = config_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("Failed to create data directory: {}", e);
            process::exit(1);
        }
    }

    println!(
        "Daemon will update data every {} {}",
        config.daemon.frequency,
        if config.daemon.frequency == 1 {
            "second"
        } else {
            "seconds"
        }
    );

    // Create a channel for shutdown signal
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Handle Ctrl+C for graceful shutdown
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        if let Ok(()) = tokio::signal::ctrl_c().await {
            println!("\nReceived Ctrl+C, shutting down...");
            let _ = shutdown_tx_clone.send(()).await;
        }
    });

    // Main daemon loop with proper shutdown handling
    loop {
        let config = Arc::clone(&config);

        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(config.daemon.frequency)) => {
                // Update power info
                if let Ok(info) = power::get_battery_info_internal() {
                    // Save to data.json
                    let data_path = config_path.parent().unwrap().join("data.json");
                    let data = serde_json::json!({
                        "updated_at": info.updated_at,
                        "power": info
                    });
                    if let Err(e) = fs::write(&data_path, serde_json::to_string_pretty(&data).unwrap()) {
                        eprintln!("Failed to save data: {}", e);
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                println!("Shutting down daemon...");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_cli_flags() {
        // Test short form
        let args = vec!["twig", "-d"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.daemon);
        assert!(!cli.fg);

        // Test long form
        let args = vec!["twig", "--daemon"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.daemon);
        assert!(!cli.fg);

        // Test short form with fg
        let args = vec!["twig", "-d", "--fg"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.daemon);
        assert!(cli.fg);

        // Test long form with fg
        let args = vec!["twig", "--daemon", "--fg"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.daemon);
        assert!(cli.fg);

        // Test with foreground alias
        let args = vec!["twig", "-d", "--foreground"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.daemon);
        assert!(cli.fg);

        // Test all long form
        let args = vec!["twig", "--daemon", "--foreground"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.daemon);
        assert!(cli.fg);
    }

    #[test]
    fn test_daemon_frequency_config() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        // Test default frequency when section is missing
        let config_content = r#"
[[time]]
format = "%H:%M:%S"
"#;
        fs::write(&config_path, config_content).unwrap();
        let config = load_config(&config_path).unwrap();
        assert_eq!(
            config.daemon.frequency, 1,
            "Should default to 1 second when section is missing"
        );

        // Test default frequency when section exists but frequency is missing
        let config_content = r#"
[daemon]
"#;
        fs::write(&config_path, config_content).unwrap();
        let config = load_config(&config_path).unwrap();
        assert_eq!(
            config.daemon.frequency, 1,
            "Should default to 1 second when frequency is missing"
        );

        // Test explicit frequency of 1
        let config_content = r#"
[daemon]
frequency = 1
"#;
        fs::write(&config_path, config_content).unwrap();
        let config = load_config(&config_path).unwrap();
        assert_eq!(
            config.daemon.frequency, 1,
            "Should accept explicit frequency of 1"
        );

        // Test custom frequency
        let config_content = r#"
[daemon]
frequency = 5
"#;
        fs::write(&config_path, config_content).unwrap();
        let config = load_config(&config_path).unwrap();
        assert_eq!(config.daemon.frequency, 5, "Should accept custom frequency");

        // Test invalid zero frequency
        let config_content = r#"
[daemon]
frequency = 0
"#;
        fs::write(&config_path, config_content).unwrap();
        let config = load_config(&config_path).unwrap();
        assert_eq!(
            config.daemon.frequency, 1,
            "Should default to 1 second when frequency is 0"
        );
    }

    #[tokio::test]
    async fn test_daemon_data_file() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let data_path = temp_dir.path().join("data.json");

        // Create a minimal config file with custom daemon frequency
        let config_content = r#"
[daemon]
frequency = 2  # Update every 2 seconds
"#;
        fs::write(&config_path, config_content).unwrap();

        // Create CLI args for daemon mode
        let args = Cli {
            timing: false,
            config: Some(config_path.clone()),
            mode: None,
            validate: false,
            colors: false,
            daemon: true,
            fg: true,
        };

        // Create shutdown channel
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
        let shutdown_tx_clone = shutdown_tx.clone();

        // Run daemon in a separate task
        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = run_daemon(&args) => {},
                _ = shutdown_rx.recv() => {}
            }
        });

        // Wait a bit for the daemon to run
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        // Send shutdown signal
        shutdown_tx_clone.send(()).await.unwrap();

        // Wait for daemon to stop
        handle.await.unwrap();

        // Check that data.json was created
        assert!(data_path.exists());

        // Verify data.json contains expected fields
        let data_content = fs::read_to_string(data_path).unwrap();
        let data: serde_json::Value = serde_json::from_str(&data_content).unwrap();

        assert!(data.get("updated_at").is_some());
        assert!(data.get("power").is_some());
    }

    #[tokio::test]
    async fn test_daemon_data_updates() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let data_path = temp_dir.path().join("data.json");

        // Create config with very short update frequency
        let config_content = r#"
[daemon]
frequency = 1
"#;
        fs::write(&config_path, config_content).unwrap();

        let args = Cli {
            timing: false,
            config: Some(config_path.clone()),
            mode: None,
            validate: false,
            colors: false,
            daemon: true,
            fg: true,
        };

        // Create shutdown channel
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
        let shutdown_tx_clone = shutdown_tx.clone();

        // Run daemon in a separate task
        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = run_daemon(&args) => {},
                _ = shutdown_rx.recv() => {}
            }
        });

        // Wait for data.json to be created (up to 5 seconds)
        let mut attempts = 0;
        while !data_path.exists() && attempts < 50 {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            attempts += 1;
        }
        assert!(
            data_path.exists(),
            "data.json was not created within timeout"
        );

        // Check initial data
        let data1 = fs::read_to_string(&data_path).unwrap();

        // Wait for at least one update cycle plus a small buffer
        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
        let data2 = fs::read_to_string(&data_path).unwrap();

        // Data should be different (timestamps should have changed)
        assert_ne!(data1, data2, "Data should be updated between checks");

        // Send shutdown signal
        shutdown_tx_clone.send(()).await.unwrap();

        // Wait for daemon to stop
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_daemon_respects_frequency() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let data_path = temp_dir.path().join("data.json");

        // Create config with 2 second update frequency
        let config_content = r#"
[daemon]
frequency = 2
"#;
        fs::write(&config_path, config_content).unwrap();

        let args = Cli {
            timing: false,
            config: Some(config_path.clone()),
            mode: None,
            validate: false,
            colors: false,
            daemon: true,
            fg: true,
        };

        // Create shutdown channel
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
        let shutdown_tx_clone = shutdown_tx.clone();

        // Run daemon in a separate task
        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = run_daemon(&args) => {},
                _ = shutdown_rx.recv() => {}
            }
        });

        // Wait for data.json to be created (up to 5 seconds)
        let mut attempts = 0;
        while !data_path.exists() && attempts < 50 {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            attempts += 1;
        }
        assert!(
            data_path.exists(),
            "data.json was not created within timeout"
        );

        // Check after 1 second (should have initial data)
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        let data1 = fs::read_to_string(&data_path).unwrap();

        // Check just before next update
        tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
        let data2 = fs::read_to_string(&data_path).unwrap();
        assert_eq!(
            data1, data2,
            "Data should not update before frequency interval"
        );

        // Check after frequency duration (should have new data)
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        let data3 = fs::read_to_string(&data_path).unwrap();
        assert_ne!(data1, data3, "Data should update after frequency interval");

        // Send shutdown signal
        shutdown_tx_clone.send(()).await.unwrap();

        // Wait for daemon to stop
        handle.await.unwrap();
    }

    #[test]
    fn test_daemon_invalid_config() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        // Test with invalid daemon_frequency
        let config_content = r#"
[daemon]
frequency = 0  # Invalid: should be > 0
"#;
        fs::write(&config_path, config_content).unwrap();
        let config = load_config(&config_path).unwrap();

        // Should fall back to default frequency
        assert_eq!(config.daemon.frequency, 1);
    }

    #[test]
    fn test_daemon_message_grammar() {
        // Test singular form
        let config = Config {
            daemon: DaemonConfig { frequency: 1 },
            ..Default::default()
        };
        let msg = format!(
            "Daemon will update data every {} {}",
            config.daemon.frequency,
            if config.daemon.frequency == 1 {
                "second"
            } else {
                "seconds"
            }
        );
        assert_eq!(msg, "Daemon will update data every 1 second");

        // Test plural form
        let config = Config {
            daemon: DaemonConfig { frequency: 2 },
            ..Default::default()
        };
        let msg = format!(
            "Daemon will update data every {} {}",
            config.daemon.frequency,
            if config.daemon.frequency == 1 {
                "second"
            } else {
                "seconds"
            }
        );
        assert_eq!(msg, "Daemon will update data every 2 seconds");
    }
}
