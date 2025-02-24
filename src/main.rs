use clap::Parser;
use directories::BaseDirs;
use fs2::FileExt;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use serde_json;
use std::env;
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::time::{Instant, SystemTime};
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

#[derive(Parser, Clone)]
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

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Parser, Clone)]
enum Commands {
    /// Run as a daemon that collects and caches information
    Daemon(DaemonCommand),
}

#[derive(Parser, Clone)]
struct DaemonCommand {
    /// Run daemon in foreground instead of background
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

    #[serde(default = "default_stale_after")]
    stale_after: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            frequency: default_daemon_frequency(),
            stale_after: default_stale_after(),
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

fn default_stale_after() -> u64 {
    5 // Default to 5 seconds
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
stale_after = 5  # How long before cached data is considered stale (in seconds)
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

#[derive(Debug)]
enum DaemonError {
    AlreadyRunning,
    LockFileError(std::io::Error),
    ConfigError(String),
}

impl fmt::Display for DaemonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DaemonError::AlreadyRunning => write!(f, "Daemon is already running"),
            DaemonError::LockFileError(e) => write!(f, "Lock file error: {}", e),
            DaemonError::ConfigError(e) => write!(f, "Configuration error: {}", e),
        }
    }
}

impl Error for DaemonError {}

impl From<ConfigError> for DaemonError {
    fn from(err: ConfigError) -> Self {
        DaemonError::ConfigError(err.to_string())
    }
}

impl From<std::io::Error> for DaemonError {
    fn from(err: std::io::Error) -> Self {
        DaemonError::LockFileError(err)
    }
}

struct DaemonLock {
    _file: File,
}

impl DaemonLock {
    fn new(config_dir: &std::path::Path) -> Result<Self, DaemonError> {
        let lock_path = config_dir.join("twig.lock");
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&lock_path)?;

        // Try to acquire exclusive lock
        match file.try_lock_exclusive() {
            Ok(_) => Ok(DaemonLock { _file: file }),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    Err(DaemonError::AlreadyRunning)
                } else {
                    Err(DaemonError::LockFileError(e))
                }
            }
        }
    }
}

impl Drop for DaemonLock {
    fn drop(&mut self) {
        // Lock will be automatically released when file is closed
        let _ = fs2::FileExt::unlock(&self._file);
    }
}

fn read_cached_data(config_path: &PathBuf, stale_after: u64) -> Option<serde_json::Value> {
    let data_path = config_path.parent().unwrap().join("data.json");
    if !data_path.exists() {
        return None;
    }

    // Try to read and parse the data
    match fs::read_to_string(&data_path).and_then(|content| {
        serde_json::from_str::<serde_json::Value>(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }) {
        Ok(data) => {
            // Check if the data is fresh enough
            if let Some(updated_at) = data["updated_at"].as_str() {
                if let Ok(timestamp) = updated_at.parse::<u64>() {
                    let now = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    if now - timestamp < stale_after {
                        return Some(data);
                    }
                }
            }
            None
        }
        Err(_) => None,
    }
}

#[tokio::main]
async fn main() {
    let start = Instant::now();
    let cli = Cli::parse();

    if cli.colors {
        colors::print_color_test();
        return;
    }

    // Handle daemon mode
    if let Some(Commands::Daemon(daemon_cmd)) = &cli.command {
        if !daemon_cmd.fg {
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
                    println!("Daemon started in background");
                    process::exit(0);
                }
            }
        }

        // Run the daemon loop
        match run_daemon(&cli).await {
            Ok(_) => return,
            Err(DaemonError::AlreadyRunning) => {
                eprintln!("Error: Daemon is already running");
                process::exit(1);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        }
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
        let config_path_clone = config_path.clone();
        tasks.push(tokio::spawn(async move {
            let mut timing = TimingData {
                fetch_time: std::time::Duration::default(),
                format_time: std::time::Duration::default(),
                fetch_count: 0,
                skip_count: 0,
                cached_count: 0,
            };

            let format_start = Instant::now();
            let mut hostname_vars = Vec::new();

            // Try to get hostname from cache first
            let hostname_data = if let Some(cached) =
                read_cached_data(&config_path_clone, config_clone.daemon.stale_after)
            {
                timing.cached_count = 1;
                Ok(cached["hostname"].as_str().unwrap().to_string())
            } else {
                // Fall back to live data
                let fetch_start = Instant::now();
                let result = hostname::get_hostname(&hostname::Config::default());
                timing.fetch_time = fetch_start.elapsed();
                timing.fetch_count = 1;
                result
            };

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
        let config_path_clone = config_path.clone();
        tasks.push(tokio::spawn(async move {
            let mut timing = TimingData {
                fetch_time: std::time::Duration::default(),
                format_time: std::time::Duration::default(),
                fetch_count: 0,
                skip_count: 0,
                cached_count: 0,
            };

            let format_start = Instant::now();
            let mut ip_vars = Vec::new();

            // Try to get IP from cache first
            let ip_data = if let Some(cached) =
                read_cached_data(&config_path_clone, config_clone.daemon.stale_after)
            {
                timing.cached_count = 1;
                Ok(cached["ip"].as_str().unwrap().to_string())
            } else {
                // Fall back to live data
                let fetch_start = Instant::now();
                let result = match &config_clone.ip.iter().find(|c| c.interface.is_some()) {
                    Some(config) => ip::get_ip(config).map(|ip| ip.to_string()),
                    None => local_ip_address::local_ip()
                        .map(|ip| ip.to_string())
                        .map_err(|e| ip::IpConfigError::Lookup(e.to_string())),
                };
                timing.fetch_time = fetch_start.elapsed();
                timing.fetch_count = 1;
                result
            };

            for (i, ip_config) in config_clone.ip.iter().enumerate() {
                let var_name = get_var_name(ip_config, "ip", i);
                if format_uses_variable(&format_clone, &var_name) {
                    match &ip_data {
                        Ok(ip) => {
                            ip_vars.push((var_name, ip.clone()));
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
        let config_path_clone = config_path.clone();
        tasks.push(tokio::spawn(async move {
            let mut timing = TimingData {
                fetch_time: std::time::Duration::default(),
                format_time: std::time::Duration::default(),
                fetch_count: 0,
                skip_count: 0,
                cached_count: 0,
            };

            let mut power_vars = Vec::new();
            let format_start = Instant::now();

            // Try to get power info from cache first
            let battery_info = if let Some(cached) =
                read_cached_data(&config_path_clone, config_clone.daemon.stale_after)
            {
                timing.cached_count = 1;
                Ok(serde_json::from_value(cached["power"].clone()).unwrap())
            } else {
                // Fall back to live data
                let fetch_start = Instant::now();
                let result = power::get_battery_info_internal();
                timing.fetch_time = fetch_start.elapsed();
                timing.fetch_count = 1;
                result
            };

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
            timing.format_time = format_start.elapsed();

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

            // Check daemon status by examining data.json
            let config_path = get_config_path(&cli.config)?;
            let data_path = config_path.parent().unwrap().join("data.json");
            let daemon_status = if let Ok(content) = fs::read_to_string(&data_path) {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(updated_at) = data["updated_at"].as_object() {
                        // Parse SystemTime format
                        let secs = updated_at["secs_since_epoch"].as_u64();
                        let nanos = updated_at["nanos_since_epoch"].as_u64();

                        if let (Some(secs), Some(_)) = (secs, nanos) {
                            let now = SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            let age = now.saturating_sub(secs);
                            if age <= config.daemon.frequency * 2 {
                                format!("Running (last update {} seconds ago)", age)
                            } else {
                                format!("Not running (last update {} seconds ago)", age)
                            }
                        } else {
                            "Status unknown (invalid timestamp format)".to_string()
                        }
                    } else {
                        "Status unknown (no timestamp)".to_string()
                    }
                } else {
                    "Status unknown (invalid JSON)".to_string()
                }
            } else {
                "Not running (no data file)".to_string()
            };

            eprintln!("  Daemon status: {}", daemon_status);
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
                if timing_data.cached_count > 0 {
                    eprintln!(
                        "        Data source: Cache ({} items)",
                        timing_data.cached_count
                    );
                } else if timing_data.fetch_count > 0 {
                    eprintln!(
                        "        Data source: Live fetch ({} items)",
                        timing_data.fetch_count
                    );
                }
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

async fn run_daemon(cli: &Cli) -> Result<(), DaemonError> {
    println!("Starting twig daemon...");

    // Load config
    let config_path =
        get_config_path(&cli.config).map_err(|e| DaemonError::ConfigError(e.to_string()))?;
    let config =
        Arc::new(load_config(&config_path).map_err(|e| DaemonError::ConfigError(e.to_string()))?);

    // Create lock in config directory
    let config_dir = config_path.parent().ok_or_else(|| {
        DaemonError::LockFileError(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Could not determine config directory",
        ))
    })?;

    // Try to acquire lock first before doing anything else
    let _lock = DaemonLock::new(config_dir)?;

    if let Some(Commands::Daemon(_daemon_cmd)) = &cli.command {
        println!(
            "Daemon will update data every {} second{}",
            config.daemon.frequency,
            if config.daemon.frequency == 1 {
                ""
            } else {
                "s"
            }
        );

        // Create a channel for shutdown signal
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Handle Ctrl+C for graceful shutdown
        let shutdown_tx_clone = shutdown_tx.clone();
        let ctrl_c_handle = tokio::spawn(async move {
            if let Ok(()) = tokio::signal::ctrl_c().await {
                println!("\nReceived Ctrl+C, shutting down...");
                let _ = shutdown_tx_clone.send(()).await;
            }
        });

        // Add test-specific timeout
        #[cfg(test)]
        let test_timeout = {
            let shutdown_tx = shutdown_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                let _ = shutdown_tx.send(()).await;
            })
        };

        // Main daemon loop with proper shutdown handling
        let main_loop = async {
            loop {
                let config = Arc::clone(&config);

                tokio::select! {
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(config.daemon.frequency)) => {
                        let update_start = Instant::now();
                        let mut blocks_processed = 0;
                        let mut blocks_failed = 0;

                        let now = SystemTime::now();
                        let duration = now.duration_since(SystemTime::UNIX_EPOCH).unwrap();
                        let mut data = serde_json::json!({
                            "updated_at": {
                                "secs_since_epoch": duration.as_secs(),
                                "nanos_since_epoch": duration.subsec_nanos(),
                            }
                        });

                        // Update power info
                        match power::get_battery_info_internal() {
                            Ok(info) => {
                                data["power"] = serde_json::to_value(info).unwrap();
                                blocks_processed += 1;
                            }
                            Err(_) => blocks_failed += 1,
                        }

                        // Update hostname info
                        match hostname::get_hostname(&hostname::Config::default()) {
                            Ok(hostname) => {
                                data["hostname"] = serde_json::to_value(hostname).unwrap();
                                blocks_processed += 1;
                            }
                            Err(_) => blocks_failed += 1,
                        }

                        // Update IP info
                        match local_ip_address::local_ip() {
                            Ok(ip) => {
                                data["ip"] = serde_json::to_value(ip.to_string()).unwrap();
                                blocks_processed += 1;
                            }
                            Err(_) => blocks_failed += 1,
                        }

                        // Save to data.json
                        let data_path = config_path.parent().unwrap().join("data.json");
                        match fs::write(&data_path, serde_json::to_string_pretty(&data).unwrap()) {
                            Ok(_) => blocks_processed += 1,
                            Err(e) => {
                                eprintln!("Failed to save data: {}", e);
                                blocks_failed += 1;
                            }
                        }

                        let update_duration = update_start.elapsed();
                        println!(
                            "[{}] Updated {} blocks ({} failed) in {:.2}ms",
                            chrono::Local::now().format("%H:%M:%S"),
                            blocks_processed,
                            blocks_failed,
                            update_duration.as_secs_f64() * 1000.0
                        );
                    }
                    _ = shutdown_rx.recv() => {
                        println!("Shutting down daemon...");
                        break;
                    }
                }
            }
        };

        // Run both the main loop and Ctrl+C handler
        #[cfg(not(test))]
        tokio::select! {
            _ = main_loop => {},
            _ = ctrl_c_handle => {},
        }

        #[cfg(test)]
        tokio::select! {
            _ = main_loop => {},
            _ = ctrl_c_handle => {},
            _ = test_timeout => {},
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_flags() {
        // Test subcommand form
        let args = vec!["twig", "daemon"];
        let cli = Cli::try_parse_from(args).unwrap();
        if let Some(Commands::Daemon(daemon_cmd)) = cli.command {
            assert!(!daemon_cmd.fg);
        } else {
            panic!("Expected daemon command");
        }

        // Test with foreground
        let args = vec!["twig", "daemon", "--fg"];
        let cli = Cli::try_parse_from(args).unwrap();
        if let Some(Commands::Daemon(daemon_cmd)) = cli.command {
            assert!(daemon_cmd.fg);
        } else {
            panic!("Expected daemon command");
        }

        // Test with foreground alias
        let args = vec!["twig", "daemon", "--foreground"];
        let cli = Cli::try_parse_from(args).unwrap();
        if let Some(Commands::Daemon(daemon_cmd)) = cli.command {
            assert!(daemon_cmd.fg);
        } else {
            panic!("Expected daemon command");
        }
    }
}
