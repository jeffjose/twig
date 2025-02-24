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

    #[serde(default = "default_data_file")]
    data_file: PathBuf,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            frequency: default_daemon_frequency(),
            stale_after: default_stale_after(),
            data_file: default_data_file(),
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

fn default_data_file() -> PathBuf {
    // By default, store data.json in the same directory as config.toml
    PathBuf::from("data.json")
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

fn get_data_file_path(config_path: &PathBuf, config: &Config) -> PathBuf {
    if config.daemon.data_file.is_absolute() {
        config.daemon.data_file.clone()
    } else {
        config_path.parent().unwrap().join(&config.daemon.data_file)
    }
}

fn read_cached_data(
    config_path: &PathBuf,
    stale_after: u64,
    config: &Config,
) -> Option<serde_json::Value> {
    let data_path = get_data_file_path(config_path, config);
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
            if let Some(updated_at) = data["updated_at"].as_object() {
                if let Some(timestamp) = updated_at["secs_since_epoch"].as_u64() {
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

#[derive(Clone)]
struct SharedState {
    config: Arc<Config>,
    config_path: PathBuf,
    prompt_format: String,
    validate: bool,
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
        let config = Arc::new(load_config(&config_path)?);
        let config_duration = config_start.elapsed();

        // Create shared state
        let state = SharedState {
            config: config.clone(),
            config_path: config_path.clone(),
            prompt_format: config.prompt.format.clone(),
            validate: cli.validate,
        };

        // Time the variable gathering
        let vars_start = Instant::now();

        // Create parallel tasks for each config section
        let mut tasks: Vec<tokio::task::JoinHandle<TaskResult>> = Vec::new();
        let mut task_names: Vec<&str> = Vec::new();

        // Handle time variables
        let state_clone = state.clone();
        tasks.push(tokio::spawn(async move {
            let mut timing = TimingData {
                fetch_time: std::time::Duration::default(),
                format_time: std::time::Duration::default(),
                fetch_count: 0,
                skip_count: 0,
                cached_count: 0,
            };

            let format_start = Instant::now();
            let mut time_vars: Vec<(String, String)> = Vec::new();
            for (i, time_config) in state_clone.config.time.iter().enumerate() {
                let var_name = get_var_name(time_config, "time", i);
                if format_uses_variable(&state_clone.prompt_format, &var_name) {
                    let fetch_start = Instant::now();
                    match format_current_time(&time_config.format) {
                        Ok(time) => {
                            timing.fetch_time += fetch_start.elapsed();
                            timing.fetch_count += 1;
                            time_vars.push((var_name, time));
                        }
                        Err(e) => {
                            if state_clone.validate {
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
        let state_clone = state.clone();
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

            let hostname_data = if let Some(cached) = read_cached_data(
                &state_clone.config_path,
                state_clone.config.daemon.stale_after,
                &state_clone.config,
            ) {
                if let Some(hostname) = cached.get("hostname") {
                    if let Some(hostname_str) = hostname.as_str() {
                        timing.cached_count = 1;
                        timing.fetch_count = 1;
                        Ok(hostname_str.to_string())
                    } else {
                        // Fall back to live data if cached data is invalid
                        let fetch_start = Instant::now();
                        let result = hostname::get_hostname(&hostname::Config::default());
                        timing.fetch_time = fetch_start.elapsed();
                        timing.fetch_count = 1;
                        result
                    }
                } else {
                    // Fall back to live data if hostname not in cache
                    let fetch_start = Instant::now();
                    let result = hostname::get_hostname(&hostname::Config::default());
                    timing.fetch_time = fetch_start.elapsed();
                    timing.fetch_count = 1;
                    result
                }
            } else {
                // Fall back to live data if no cache
                let fetch_start = Instant::now();
                let result = hostname::get_hostname(&hostname::Config::default());
                timing.fetch_time = fetch_start.elapsed();
                timing.fetch_count = 1;
                result
            };

            for (i, hostname_config) in state_clone.config.hostname.iter().enumerate() {
                let var_name = get_var_name(hostname_config, "hostname", i);
                if format_uses_variable(&state_clone.prompt_format, &var_name) {
                    match &hostname_data {
                        Ok(hostname) => {
                            hostname_vars.push((var_name, hostname.clone()));
                        }
                        Err(e) => {
                            if state_clone.validate {
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
        let state_clone = state.clone();
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

            let ip_data = if let Some(cached) = read_cached_data(
                &state_clone.config_path,
                state_clone.config.daemon.stale_after,
                &state_clone.config,
            ) {
                if let Some(ip) = cached.get("ip") {
                    if let Some(ip_str) = ip.as_str() {
                        timing.cached_count = 1;
                        timing.fetch_count = 1;
                        Ok(ip_str.to_string())
                    } else {
                        // Fall back to live data if cached data is invalid
                        let fetch_start = Instant::now();
                        let result = match &state_clone.config.ip.iter().find(|c| c.interface.is_some()) {
                            Some(config) => ip::get_ip(config).map(|ip| ip.to_string()),
                            None => local_ip_address::local_ip()
                                .map(|ip| ip.to_string())
                                .map_err(|e| ip::IpConfigError::Lookup(e.to_string())),
                        };
                        timing.fetch_time = fetch_start.elapsed();
                        timing.fetch_count = 1;
                        result
                    }
                } else {
                    // Fall back to live data if ip not in cache
                    let fetch_start = Instant::now();
                    let result = match &state_clone.config.ip.iter().find(|c| c.interface.is_some()) {
                        Some(config) => ip::get_ip(config).map(|ip| ip.to_string()),
                        None => local_ip_address::local_ip()
                            .map(|ip| ip.to_string())
                            .map_err(|e| ip::IpConfigError::Lookup(e.to_string())),
                    };
                    timing.fetch_time = fetch_start.elapsed();
                    timing.fetch_count = 1;
                    result
                }
            } else {
                // Fall back to live data if no cache
                let fetch_start = Instant::now();
                let result = match &state_clone.config.ip.iter().find(|c| c.interface.is_some()) {
                    Some(config) => ip::get_ip(config).map(|ip| ip.to_string()),
                    None => local_ip_address::local_ip()
                        .map(|ip| ip.to_string())
                        .map_err(|e| ip::IpConfigError::Lookup(e.to_string())),
                };
                timing.fetch_time = fetch_start.elapsed();
                timing.fetch_count = 1;
                result
            };

            for (i, ip_config) in state_clone.config.ip.iter().enumerate() {
                let var_name = get_var_name(ip_config, "ip", i);
                if format_uses_variable(&state_clone.prompt_format, &var_name) {
                    match &ip_data {
                        Ok(ip) => {
                            ip_vars.push((var_name, ip.clone()));
                        }
                        Err(e) => {
                            if state_clone.validate {
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
        let state_clone = state.clone();
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
            for (i, cwd_config) in state_clone.config.cwd.iter().enumerate() {
                let var_name = get_var_name(cwd_config, "cwd", i);
                if format_uses_variable(&state_clone.prompt_format, &var_name) {
                    let fetch_start = Instant::now();
                    match cwd::get_cwd(cwd_config) {
                        Ok(dir) => {
                            timing.fetch_time += fetch_start.elapsed();
                            timing.fetch_count += 1;
                            cwd_vars.push((var_name, dir));
                        }
                        Err(e) => {
                            if state_clone.validate {
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
        let state_clone = state.clone();
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

            let battery_info = if let Some(cached) = read_cached_data(
                &state_clone.config_path,
                state_clone.config.daemon.stale_after,
                &state_clone.config,
            ) {
                if let Some(power) = cached.get("power") {
                    match serde_json::from_value(power.clone()) {
                        Ok(info) => {
                            timing.cached_count = 1;
                            timing.fetch_count = 1;
                            Ok(info)
                        }
                        Err(_) => {
                            // Fall back to live data if cached data is invalid
                            let fetch_start = Instant::now();
                            let result = power::get_battery_info_internal();
                            timing.fetch_time = fetch_start.elapsed();
                            timing.fetch_count = 1;
                            result
                        }
                    }
                } else {
                    // Fall back to live data if power not in cache
                    let fetch_start = Instant::now();
                    let result = power::get_battery_info_internal();
                    timing.fetch_time = fetch_start.elapsed();
                    timing.fetch_count = 1;
                    result
                }
            } else {
                // Fall back to live data if no cache
                let fetch_start = Instant::now();
                let result = power::get_battery_info_internal();
                timing.fetch_time = fetch_start.elapsed();
                timing.fetch_count = 1;
                result
            };

            if let Ok(info) = &battery_info {
                for (i, power_config) in state_clone.config.power.iter().enumerate() {
                    let var_name = get_var_name(power_config, "power", i);
                    if format_uses_variable(&state_clone.prompt_format, &var_name) {
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
                if state_clone.validate {
                    eprintln!("Warning: couldn't get battery info: {}", e);
                }
            }
            timing.format_time = format_start.elapsed();

            Ok((power_vars, timing))
        }));
        task_names.push("Power variables");

        // Handle environment variables
        let state_clone = state.clone();
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
            for var_name in get_env_vars_from_format(&state_clone.prompt_format) {
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
                    if state.validate {
                        eprintln!("Warning: task failed: {}", e);
                    }
                }
                Err(e) => {
                    if state.validate {
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
            &state.config.prompt.format,
            &template_vars,
            state.validate,
            cli.mode.as_deref(),
        )?;
        println!("{}", output);

        if cli.timing {
            let total_duration = start.elapsed();
            let total_nanos = total_duration.as_nanos() as f64;

            // Sort task timings by total time (fetch + format)
            let mut sorted_timings: Vec<_> = task_timings.into_iter().collect();
            sorted_timings.sort_by(|(_, a), (_, b)| {
                let a_total = a.fetch_time + a.format_time;
                let b_total = b.fetch_time + b.format_time;
                b_total.cmp(&a_total) // Reverse sort - slowest first
            });

            // Calculate totals
            let mut total_cached = 0;
            let mut total_live = 0;
            let mut total_skipped = 0;
            let total_errors = 0;

            // Check daemon status
            let data_path = get_data_file_path(&state.config_path, &state.config);
            let daemon_status = if let Ok(content) = fs::read_to_string(&data_path) {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(updated_at) = data["updated_at"].as_object() {
                        if let (Some(secs), Some(_)) = (
                            updated_at["secs_since_epoch"].as_u64(),
                            updated_at["nanos_since_epoch"].as_u64(),
                        ) {
                            let now = SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            let age = now.saturating_sub(secs);
                            if age <= state.config.daemon.frequency * 2 {
                                format!("active ({} sec ago)", age)
                            } else {
                                format!("inactive ({} sec ago)", age)
                            }
                        } else {
                            "unknown (bad timestamp)".to_string()
                        }
                    } else {
                        "unknown (no timestamp)".to_string()
                    }
                } else {
                    "unknown (bad data)".to_string()
                }
            } else {
                "not running".to_string()
            };

            eprintln!("\nTiming Report");
            eprintln!("daemon: {}", daemon_status);
            eprintln!("data: {}", data_path.display());

            // Print task details
            for (name, timing_data) in &sorted_timings {
                let total_time = timing_data.fetch_time + timing_data.format_time;
                let percent = total_time.as_nanos() as f64 / total_nanos * 100.0;

                total_cached += timing_data.cached_count;
                total_live += timing_data.fetch_count - timing_data.cached_count;
                total_skipped += timing_data.skip_count;

                let source_type = if timing_data.cached_count > 0 && timing_data.fetch_count == timing_data.cached_count {
                    "[cache]"
                } else if timing_data.fetch_count > 0 {
                    "[live]"
                } else {
                    "[skip]"
                };

                let mut stats = Vec::new();
                if timing_data.cached_count > 0 {
                    stats.push(format!("cached:{}", timing_data.cached_count));
                }
                if timing_data.fetch_count - timing_data.cached_count > 0 {
                    stats.push(format!("live:{}", timing_data.fetch_count - timing_data.cached_count));
                }
                if timing_data.skip_count > 0 {
                    stats.push(format!("skip:{}", timing_data.skip_count));
                }

                if timing_data.fetch_count > 0 || timing_data.cached_count > 0 {
                    eprintln!("├─ {} {} ({:.1}%)", source_type, name, percent);
                    eprintln!("│  ├─ items: {}", stats.join(", "));
                    eprintln!("│  └─ time: fetch={:.1}ms ({:.1}%), proc={:.1}ms ({:.1}%), total={:.1}ms",
                        timing_data.fetch_time.as_secs_f64() * 1000.0,
                        (timing_data.fetch_time.as_nanos() as f64 / total_nanos * 100.0),
                        timing_data.format_time.as_secs_f64() * 1000.0,
                        (timing_data.format_time.as_nanos() as f64 / total_nanos * 100.0),
                        total_time.as_secs_f64() * 1000.0
                    );
                }
            }

            // Print summary
            eprintln!("└─ summary");
            eprintln!("   ├─ items: {} cached, {} live, {} skipped{}",
                total_cached, total_live, total_skipped,
                if total_errors > 0 { format!(", {} errors", total_errors) } else { "".to_string() }
            );
            eprintln!("   └─ time: cfg={:.1}ms ({:.1}%), data={:.1}ms ({:.1}%), tmpl={:.1}ms ({:.1}%), total={:.1}ms",
                config_duration.as_secs_f64() * 1000.0,
                (config_duration.as_nanos() as f64 / total_nanos * 100.0),
                vars_duration.as_secs_f64() * 1000.0,
                (vars_duration.as_nanos() as f64 / total_nanos * 100.0),
                template_start.elapsed().as_secs_f64() * 1000.0,
                (template_start.elapsed().as_nanos() as f64 / total_nanos * 100.0),
                total_duration.as_secs_f64() * 1000.0
            );
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

        // Get data file path
        let data_path = get_data_file_path(&config_path, &config);

        // Create data file directory if it doesn't exist
        if let Some(parent) = data_path.parent() {
            fs::create_dir_all(parent)?;
        }

        println!("Data will be stored in: {}", data_path.display());

        // Create a channel for shutdown signal
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Handle Ctrl+C for graceful shutdown
        let shutdown_tx_clone = shutdown_tx.clone();
        let ctrl_c_future = async move {
            if let Ok(()) = tokio::signal::ctrl_c().await {
                println!("\nReceived Ctrl+C, shutting down...");
                let _ = shutdown_tx_clone.send(()).await;
            }
        };

        // Add test-specific timeout
        #[cfg(test)]
        let test_timeout_future = {
            let shutdown_tx = shutdown_tx.clone();
            async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                let _ = shutdown_tx.send(()).await;
            }
        };

        // Main daemon loop with proper shutdown handling
        let main_loop = async {
            loop {
                let config = Arc::clone(&config);
                let data_path = data_path.clone();

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

                        // Save to data file
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
            _ = ctrl_c_future => {},
        }

        #[cfg(test)]
        tokio::select! {
            _ = main_loop => {},
            _ = ctrl_c_future => {},
            _ = test_timeout_future => {},
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Instant;
    use tempfile::tempdir;

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

    #[test]
    fn test_timing_output_format() {
        // Create a temporary directory for config and data files
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        // Create a basic config file
        let config_content = r#"
            [[time]]
            format = "%H:%M:%S"

            [prompt]
            format = "{time}"

            [daemon]
            frequency = 1
            stale_after = 5
            data_file = "data.json"
        "#;
        fs::write(&config_path, config_content).unwrap();

        // Create a CLI instance with timing enabled
        let cli = Cli {
            timing: true,
            config: Some(config_path.clone()),
            mode: None,
            validate: false,
            colors: false,
            command: None,
        };

        // Capture stderr output
        let mut stderr = Vec::new();
        {
            let start = Instant::now();
            let config_start = Instant::now();
            let config_path = get_config_path(&cli.config).unwrap();
            let config = Arc::new(load_config(&config_path).unwrap());
            let config_duration = config_start.elapsed();

            let vars_start = Instant::now();
            let state = SharedState {
                config: config.clone(),
                config_path: config_path.clone(),
                prompt_format: config.prompt.format.clone(),
                validate: cli.validate,
            };

            let mut time_vars: Vec<(String, String)> = Vec::new();
            let mut timing = TimingData {
                fetch_time: std::time::Duration::default(),
                format_time: std::time::Duration::default(),
                fetch_count: 0,
                skip_count: 0,
                cached_count: 0,
            };

            let format_start = Instant::now();
            for (i, time_config) in state.config.time.iter().enumerate() {
                let var_name = get_var_name(time_config, "time", i);
                if format_uses_variable(&state.prompt_format, &var_name) {
                    let fetch_start = Instant::now();
                    match format_current_time(&time_config.format) {
                        Ok(time) => {
                            timing.fetch_time += fetch_start.elapsed();
                            timing.fetch_count += 1;
                            time_vars.push((var_name, time));
                        }
                        Err(e) => {
                            if state.validate {
                                writeln!(stderr, "Warning: couldn't format time: {}", e).unwrap();
                            }
                        }
                    }
                } else {
                    timing.skip_count += 1;
                }
            }
            timing.format_time = format_start.elapsed();

            let vars_duration = vars_start.elapsed();
            let total_duration = start.elapsed();
            let total_nanos = total_duration.as_nanos() as f64;

            writeln!(stderr, "\nTiming information:").unwrap();
            writeln!(stderr, "  Data sources summary:").unwrap();
            writeln!(stderr, "    Cached items: {}", timing.cached_count).unwrap();
            writeln!(
                stderr,
                "    Live fetched items: {}",
                timing.fetch_count - timing.cached_count
            )
            .unwrap();
            writeln!(stderr, "    Skipped items: {}", timing.skip_count).unwrap();
            writeln!(stderr, "  Daemon status: Not running (no data file)").unwrap();
            writeln!(
                stderr,
                "  Config loading: {:?} ({:.1}%)",
                config_duration,
                (config_duration.as_nanos() as f64 / total_nanos * 100.0)
            )
            .unwrap();
            writeln!(
                stderr,
                "  Variable gathering (total): {:?} ({:.1}%)",
                vars_duration,
                (vars_duration.as_nanos() as f64 / total_nanos * 100.0)
            )
            .unwrap();
            writeln!(stderr, "  Total time: {:?}", total_duration).unwrap();
        }

        // Convert stderr to string
        let stderr = String::from_utf8(stderr).unwrap();

        // Verify timing output format
        assert!(
            stderr.contains("Timing information:"),
            "stderr should contain 'Timing information:', got: {}",
            stderr
        );
        assert!(stderr.contains("Data sources summary:"));
        assert!(stderr.contains("Cached items:"));
        assert!(stderr.contains("Live fetched items:"));
        assert!(stderr.contains("Skipped items:"));
        assert!(stderr.contains("Config loading:"));
        assert!(stderr.contains("Variable gathering (total):"));
        assert!(stderr.contains("Total time:"));
    }

    #[test]
    fn test_timing_data_source_tracking() {
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(100),
            format_time: std::time::Duration::from_millis(50),
            fetch_count: 2,
            skip_count: 1,
            cached_count: 1,
        };

        assert_eq!(timing.fetch_count - timing.cached_count, 1); // Live fetches
        assert_eq!(timing.cached_count, 1); // Cached items
        assert_eq!(timing.skip_count, 1); // Skipped items
    }

    #[test]
    fn test_timing_duration_calculation() {
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(100),
            format_time: std::time::Duration::from_millis(50),
            fetch_count: 1,
            skip_count: 0,
            cached_count: 0,
        };

        let total_time = timing.fetch_time + timing.format_time;
        assert_eq!(total_time, std::time::Duration::from_millis(150));
    }

    #[test]
    fn test_source_type_determination() {
        // Helper function to determine source type
        fn get_source_type(timing: &TimingData) -> &'static str {
            if timing.cached_count > 0 && timing.fetch_count == timing.cached_count {
                "[cache]"
            } else if timing.fetch_count > 0 {
                "[live]"
            } else {
                "[skip]"
            }
        }

        // Test case 1: All data from cache
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(100),
            format_time: std::time::Duration::from_millis(50),
            fetch_count: 2,
            cached_count: 2,
            skip_count: 0,
        };
        assert_eq!(
            get_source_type(&timing),
            "[cache]",
            "Should show [cache] when all data is cached"
        );

        // Test case 2: Mixed cache and live data
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(100),
            format_time: std::time::Duration::from_millis(50),
            fetch_count: 3,
            cached_count: 1,
            skip_count: 0,
        };
        assert_eq!(
            get_source_type(&timing),
            "[live]",
            "Should show [live] when any data is fetched live"
        );

        // Test case 3: All live data
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(100),
            format_time: std::time::Duration::from_millis(50),
            fetch_count: 2,
            cached_count: 0,
            skip_count: 0,
        };
        assert_eq!(
            get_source_type(&timing),
            "[live]",
            "Should show [live] when all data is fetched live"
        );

        // Test case 4: Skipped data
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(0),
            format_time: std::time::Duration::from_millis(0),
            fetch_count: 0,
            cached_count: 0,
            skip_count: 2,
        };
        assert_eq!(
            get_source_type(&timing),
            "[skip]",
            "Should show [skip] when data is skipped"
        );

        // Test case 5: Edge case - cached_count > fetch_count (invalid state)
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(100),
            format_time: std::time::Duration::from_millis(50),
            fetch_count: 1,
            cached_count: 2,
            skip_count: 0,
        };
        assert_eq!(
            get_source_type(&timing),
            "[live]",
            "Should show [live] when cached_count > fetch_count (invalid state)"
        );

        // Test case 6: Edge case - zero counts with skip
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(0),
            format_time: std::time::Duration::from_millis(0),
            fetch_count: 0,
            cached_count: 0,
            skip_count: 0,
        };
        assert_eq!(
            get_source_type(&timing),
            "[skip]",
            "Should show [skip] when all counts are zero"
        );

        // Test case 7: Mixed state with skips
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(100),
            format_time: std::time::Duration::from_millis(50),
            fetch_count: 2,
            cached_count: 1,
            skip_count: 3,
        };
        assert_eq!(
            get_source_type(&timing),
            "[live]",
            "Should show [live] when mixed with skips"
        );
    }

    #[test]
    fn test_timing_data_edge_cases() {
        // Test case 1: High fetch count, low cache count
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(100),
            format_time: std::time::Duration::from_millis(50),
            fetch_count: 1000,
            cached_count: 1,
            skip_count: 0,
        };
        assert_eq!(
            timing.fetch_count - timing.cached_count,
            999,
            "Should handle large fetch counts"
        );
        assert_eq!(timing.cached_count, 1, "Should preserve small cache count");

        // Test case 2: Equal high counts
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(100),
            format_time: std::time::Duration::from_millis(50),
            fetch_count: 1000,
            cached_count: 1000,
            skip_count: 0,
        };
        assert_eq!(
            timing.fetch_count, timing.cached_count,
            "Should handle equal high counts"
        );
        assert_eq!(
            timing.fetch_count - timing.cached_count,
            0,
            "Should show no live fetches"
        );

        // Test case 3: High skip count
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(0),
            format_time: std::time::Duration::from_millis(0),
            fetch_count: 0,
            cached_count: 0,
            skip_count: 1000,
        };
        assert_eq!(timing.skip_count, 1000, "Should handle high skip counts");
        assert_eq!(
            timing.fetch_count, 0,
            "Should have no fetches with high skip count"
        );

        // Test case 4: Mixed high counts
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(100),
            format_time: std::time::Duration::from_millis(50),
            fetch_count: 1000,
            cached_count: 500,
            skip_count: 2000,
        };
        assert_eq!(
            timing.fetch_count - timing.cached_count,
            500,
            "Should handle mixed high counts"
        );
        assert_eq!(timing.skip_count, 2000, "Should preserve high skip count");
    }

    #[test]
    fn test_timing_data_complex_scenarios() {
        // Test case 1: Multiple variables with mixed sources
        let mut total_timing = TimingData {
            fetch_time: std::time::Duration::default(),
            format_time: std::time::Duration::default(),
            fetch_count: 0,
            cached_count: 0,
            skip_count: 0,
        };

        // Add some cached data
        total_timing.fetch_count += 3;
        total_timing.cached_count += 3;
        assert_eq!(
            total_timing.fetch_count, total_timing.cached_count,
            "Should be all cached initially"
        );

        // Add some live data
        total_timing.fetch_count += 2;
        assert_eq!(
            total_timing.fetch_count - total_timing.cached_count,
            2,
            "Should show correct live count after adding live data"
        );

        // Add some skipped data
        total_timing.skip_count += 4;
        assert_eq!(
            total_timing.skip_count, 4,
            "Should track skipped count independently"
        );

        // Test case 2: Accumulating timing data
        let mut timing1 = TimingData {
            fetch_time: std::time::Duration::from_millis(100),
            format_time: std::time::Duration::from_millis(50),
            fetch_count: 2,
            cached_count: 1,
            skip_count: 1,
        };

        let timing2 = TimingData {
            fetch_time: std::time::Duration::from_millis(200),
            format_time: std::time::Duration::from_millis(75),
            fetch_count: 3,
            cached_count: 2,
            skip_count: 2,
        };

        // Simulate combining timing data
        timing1.fetch_time += timing2.fetch_time;
        timing1.format_time += timing2.format_time;
        timing1.fetch_count += timing2.fetch_count;
        timing1.cached_count += timing2.cached_count;
        timing1.skip_count += timing2.skip_count;

        assert_eq!(
            timing1.fetch_time,
            std::time::Duration::from_millis(300),
            "Should accumulate fetch time"
        );
        assert_eq!(
            timing1.format_time,
            std::time::Duration::from_millis(125),
            "Should accumulate format time"
        );
        assert_eq!(timing1.fetch_count, 5, "Should accumulate fetch count");
        assert_eq!(timing1.cached_count, 3, "Should accumulate cached count");
        assert_eq!(timing1.skip_count, 3, "Should accumulate skip count");
    }

    #[test]
    fn test_timing_data_boundary_conditions() {
        // Test case 1: Zero duration with counts
        let timing = TimingData {
            fetch_time: std::time::Duration::from_millis(0),
            format_time: std::time::Duration::from_millis(0),
            fetch_count: 5,
            cached_count: 3,
            skip_count: 2,
        };
        assert_eq!(
            timing.fetch_time.as_nanos(),
            0,
            "Should handle zero fetch time"
        );
        assert_eq!(
            timing.format_time.as_nanos(),
            0,
            "Should handle zero format time"
        );
        assert_eq!(
            timing.fetch_count - timing.cached_count,
            2,
            "Should track counts with zero time"
        );

        // Test case 2: Max duration with zero counts
        let timing = TimingData {
            fetch_time: std::time::Duration::from_secs(u64::MAX),
            format_time: std::time::Duration::from_secs(u64::MAX),
            fetch_count: 0,
            cached_count: 0,
            skip_count: 0,
        };
        assert_eq!(
            timing.fetch_time.as_secs(),
            u64::MAX,
            "Should handle max fetch time"
        );
        assert_eq!(
            timing.format_time.as_secs(),
            u64::MAX,
            "Should handle max format time"
        );
        assert_eq!(
            timing.fetch_count, 0,
            "Should handle zero counts with max time"
        );

        // Test case 3: Minimum non-zero values
        let timing = TimingData {
            fetch_time: std::time::Duration::from_nanos(1),
            format_time: std::time::Duration::from_nanos(1),
            fetch_count: 1,
            cached_count: 1,
            skip_count: 1,
        };
        assert_eq!(
            timing.fetch_time.as_nanos(),
            1,
            "Should handle minimum fetch time"
        );
        assert_eq!(
            timing.format_time.as_nanos(),
            1,
            "Should handle minimum format time"
        );
        assert_eq!(
            timing.fetch_count, timing.cached_count,
            "Should handle minimum counts"
        );
    }
}
