use chrono::Local;
use clap::Parser;
use directories::ProjectDirs;
use gethostname::gethostname;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "twig")]
#[command(about = "Shell prompt generator with daemon caching")]
struct Cli {
    /// Output only the prompt (for shell integration)
    #[arg(long)]
    prompt: bool,

    /// Path to config file (default: ~/.config/twig/config.toml)
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    #[serde(default)]
    time: Option<TimeConfig>,
    #[serde(default)]
    hostname: Option<HostnameConfig>,
    #[serde(default)]
    cwd: Option<CwdConfig>,
    prompt: PromptConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct TimeConfig {
    #[serde(default = "default_time_format")]
    format: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct HostnameConfig {}

#[derive(Debug, Deserialize, Serialize)]
struct CwdConfig {}

#[derive(Debug, Deserialize, Serialize)]
struct PromptConfig {
    format: String,
}

fn default_time_format() -> String {
    "%H:%M:%S".to_string()
}

fn main() {
    let cli = Cli::parse();

    let start = Instant::now();

    // Load config from file (or create default)
    let config_start = Instant::now();
    let (config, config_path) = load_config(cli.config.as_deref());
    let config_time = config_start.elapsed();

    // Collect all variables
    let render_start = Instant::now();
    let mut variables = HashMap::new();

    // Get current time with format from config
    if let Some(time_config) = &config.time {
        let time = Local::now().format(&time_config.format).to_string();
        variables.insert("time", time);
    }

    // Get hostname if configured
    if config.hostname.is_some() {
        let hostname = gethostname()
            .to_string_lossy()
            .to_string();
        variables.insert("host", hostname);
    }

    // Get current working directory if configured
    if config.cwd.is_some() {
        let cwd = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("?"))
            .to_string_lossy()
            .to_string();
        variables.insert("dir", cwd);
    }

    // Perform variable substitution with color support
    let output = substitute_variables(&config.prompt.format, &variables);
    let render_time = render_start.elapsed();

    let total_time = start.elapsed();

    // Output based on mode
    if cli.prompt {
        // Shell integration mode: just the prompt, no newline
        print!("{}", output);
    } else {
        // Development/testing mode: boxed output with timing
        print_boxed(&output, &config_path, config_time, render_time, total_time);
    }
}

/// Print the prompt in a box with timing information
fn print_boxed(
    prompt: &str,
    config_path: &PathBuf,
    config_time: std::time::Duration,
    render_time: std::time::Duration,
    total_time: std::time::Duration,
) {
    // Display config file path (dimmed)
    println!("\x1b[2mConfig: {}\x1b[0m", config_path.display());
    println!();

    // Strip ANSI codes to measure actual text length
    let text_only = strip_ansi_codes(prompt);
    let width = text_only.len().max(50);

    // Top border
    println!("┌{}┐", "─".repeat(width + 2));

    // Prompt content (with ANSI codes preserved)
    println!("│ {}{} │", prompt, " ".repeat(width - text_only.len()));

    // Bottom border
    println!("└{}┘", "─".repeat(width + 2));

    // Timing information (dimmed)
    println!(
        "\x1b[2mTiming: {:.2}ms total (config: {:.2}ms | render: {:.2}ms)\x1b[0m",
        total_time.as_secs_f64() * 1000.0,
        config_time.as_secs_f64() * 1000.0,
        render_time.as_secs_f64() * 1000.0
    );
}

/// Strip ANSI escape codes from a string to get actual text length
fn strip_ansi_codes(s: &str) -> String {
    let re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").to_string()
}

/// Load config from specified path or ~/.config/twig/config.toml
/// Creates default config if it doesn't exist (only for default path)
/// Returns (config, path_used)
fn load_config(custom_path: Option<&std::path::Path>) -> (Config, PathBuf) {
    let config_path = custom_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(get_config_path);

    // If config exists, load it
    let config = if config_path.exists() {
        let contents = fs::read_to_string(&config_path)
            .expect("Failed to read config file");

        toml::from_str(&contents)
            .expect("Failed to parse config file")
    } else {
        // Only auto-create if using default path
        if custom_path.is_none() {
            // Create default config and save it
            let default_config = create_default_config();

            // Ensure parent directory exists
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)
                    .expect("Failed to create config directory");
            }

            // Write default config
            let toml_string = toml::to_string_pretty(&default_config)
                .expect("Failed to serialize config");

            fs::write(&config_path, toml_string)
                .expect("Failed to write config file");

            default_config
        } else {
            panic!("Config file not found: {}", config_path.display());
        }
    };

    (config, config_path)
}

/// Get config file path: ~/.config/twig/config.toml
fn get_config_path() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("", "", "twig") {
        proj_dirs.config_dir().join("config.toml")
    } else {
        // Fallback to ~/.config/twig/config.toml
        let mut path = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        path.push(".config");
        path.push("twig");
        path.push("config.toml");
        path
    }
}

/// Create default config
fn create_default_config() -> Config {
    Config {
        time: Some(TimeConfig {
            format: "%H:%M:%S".to_string(),
        }),
        hostname: Some(HostnameConfig {}),
        cwd: Some(CwdConfig {}),
        prompt: PromptConfig {
            format: "{time:cyan} {\"@\":yellow,bold} {host:magenta} {dir:green} {\"$\":white,bold} ".to_string(),
        },
    }
}

/// Template substitution with color/style support
/// Supports:
/// - {var} - plain variable
/// - {var:color} - variable with color
/// - {var:color,style} - variable with color and style
/// - {"text":color} - literal text with color
fn substitute_variables(template: &str, variables: &HashMap<&str, String>) -> String {
    // Match {anything} patterns
    let re = Regex::new(r"\{([^}]+)\}").unwrap();

    re.replace_all(template, |caps: &regex::Captures| {
        let content = &caps[1];

        // Check if it's a literal: "text":color
        if content.starts_with('"') {
            return handle_literal(content);
        }

        // Otherwise it's a variable: var or var:color or var:color,style
        handle_variable(content, variables)
    }).to_string()
}

/// Handle literal text: "text":color or "text":color,style
fn handle_literal(content: &str) -> String {
    // Parse: "text":color or "text":color,style
    if let Some(colon_pos) = content.find(':') {
        let text_part = &content[..colon_pos];
        let style_part = &content[colon_pos + 1..];

        // Extract text from quotes
        let text = text_part.trim_matches('"');

        // Apply color/style
        colorize(text, style_part)
    } else {
        // No color specified, just remove quotes
        content.trim_matches('"').to_string()
    }
}

/// Handle variable: var or var:color or var:color,style
fn handle_variable(content: &str, variables: &HashMap<&str, String>) -> String {
    // Parse: var or var:color or var:color,style
    let parts: Vec<&str> = content.split(':').collect();

    let var_name = parts[0];
    let style_spec = parts.get(1).copied();

    // Get variable value
    let value = variables.get(var_name)
        .map(|s| s.as_str())
        .unwrap_or(var_name); // Fallback to var name if not found

    // Apply color/style if specified
    if let Some(style) = style_spec {
        colorize(value, style)
    } else {
        value.to_string()
    }
}

/// Apply ANSI color and style codes to text
/// style_spec can be: "color" or "color,style" or "color,style1,style2"
fn colorize(text: &str, style_spec: &str) -> String {
    let parts: Vec<&str> = style_spec.split(',').map(|s| s.trim()).collect();

    let mut codes = Vec::new();

    for part in parts {
        if let Some(code) = get_ansi_code(part) {
            codes.push(code);
        }
    }

    if codes.is_empty() {
        // No valid codes, return text as-is
        text.to_string()
    } else {
        // Apply codes: \x1b[code1;code2;...m text \x1b[0m
        format!("\x1b[{}m{}\x1b[0m", codes.join(";"), text)
    }
}

/// Convert color/style name to ANSI code
fn get_ansi_code(name: &str) -> Option<&'static str> {
    match name {
        // Basic colors (30-37)
        "black" => Some("30"),
        "red" => Some("31"),
        "green" => Some("32"),
        "yellow" => Some("33"),
        "blue" => Some("34"),
        "magenta" => Some("35"),
        "cyan" => Some("36"),
        "white" => Some("37"),

        // Bright colors (90-97)
        "bright_black" | "gray" | "grey" => Some("90"),
        "bright_red" => Some("91"),
        "bright_green" => Some("92"),
        "bright_yellow" => Some("93"),
        "bright_blue" => Some("94"),
        "bright_magenta" => Some("95"),
        "bright_cyan" => Some("96"),
        "bright_white" => Some("97"),

        // Styles
        "bold" => Some("1"),
        "italic" => Some("3"),
        "underline" => Some("4"),
        "normal" => Some("0"),

        _ => None,
    }
}
