mod config;
mod providers;
mod shell;

use clap::Parser;
use config::{Config, CwdConfig, HostnameConfig, PromptConfig, TimeConfig};
use directories::ProjectDirs;
use regex::Regex;
use shell::{get_formatter, ShellFormatter, ShellMode};
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

    /// Shell output mode (tcsh, bash, zsh) - outputs shell-specific prompt format
    #[arg(long, value_name = "SHELL")]
    mode: Option<String>,

    /// Show debug information before prompt (only with --mode). Can also use TWIG_DEBUG env var
    #[arg(long)]
    debug: bool,

    /// Validate provider configurations and show any errors
    #[arg(long)]
    validate: bool,
}

fn main() {
    let cli = Cli::parse();

    let start = Instant::now();

    // Load config from file (or create default)
    let config_start = Instant::now();
    let (mut config, config_path) = load_config(cli.config.as_deref());

    // Apply implicit sections for variables used in template
    let format = config.prompt.format.clone();
    apply_implicit_sections(&mut config, &format);

    let config_time = config_start.elapsed();

    // Collect all variables from providers
    let render_start = Instant::now();
    let registry = providers::ProviderRegistry::new();

    let (variables, provider_timings) = match registry.collect_all(&config, cli.validate) {
        Ok(result) => (result.variables, result.timings),
        Err(e) if cli.validate => {
            eprintln!("Provider error: {:?}", e);
            std::process::exit(1);
        }
        Err(_) => (HashMap::new(), Vec::new()), // Should not happen - providers catch errors in non-validate mode
    };

    // If in validate mode, show success and exit
    if cli.validate {
        println!("✓ All providers validated successfully");
        return;
    }

    // Determine shell mode and output format
    let (shell_mode, show_box) = if let Some(mode) = &cli.mode {
        // --mode flag: use specified shell formatter, no box
        let mode = match mode.as_str() {
            "tcsh" => ShellMode::Tcsh,
            "bash" => ShellMode::Bash,
            "zsh" => ShellMode::Zsh,
            other => {
                eprintln!("Unknown shell mode: {}. Valid options: tcsh, bash, zsh", other);
                std::process::exit(1);
            }
        };
        (mode, false)
    } else if cli.prompt {
        // --prompt flag: raw ANSI codes, no box
        (ShellMode::Raw, false)
    } else {
        // Default: raw ANSI codes, show box
        (ShellMode::Raw, true)
    };

    // Create formatter for the selected shell mode
    let formatter = get_formatter(shell_mode);

    // Perform variable substitution with color support
    let output = substitute_variables(&config.prompt.format, &variables, formatter.as_ref());

    // Post-process output for shell-specific requirements (e.g., escape newlines for TCSH/Zsh)
    let output = formatter.finalize(&output);
    let render_time = render_start.elapsed();

    let total_time = start.elapsed();

    // Output based on show_box and debug flags
    if show_box {
        // Development/testing mode: boxed output with timing
        print_boxed(
            &output,
            &config_path,
            config_time,
            render_time,
            total_time,
            &provider_timings,
        );
    } else if (cli.debug || std::env::var("TWIG_DEBUG").is_ok()) && cli.mode.is_some() {
        // Debug mode for shell integration: show debug info to stderr, prompt to stdout
        // Enabled via --debug flag or TWIG_DEBUG environment variable
        print_debug_box(&config_path, config_time, render_time, total_time, &provider_timings);
        print!("{}", output);
    } else {
        // Shell integration or prompt mode: just the prompt, no newline
        print!("{}", output);
    }
}

/// Print the prompt in a box with timing information
fn print_boxed(
    prompt: &str,
    config_path: &PathBuf,
    config_time: std::time::Duration,
    render_time: std::time::Duration,
    total_time: std::time::Duration,
    provider_timings: &[providers::ProviderTiming],
) {
    // Display config file path (dimmed)
    println!("\x1b[2mConfig: {}\x1b[0m", config_path.display());
    println!();

    // Split prompt into lines and strip ANSI codes from each
    let lines: Vec<&str> = prompt.split('\n').collect();
    let text_lines: Vec<String> = lines.iter().map(|line| strip_ansi_codes(line)).collect();

    // Find the maximum width across all lines
    let max_width = text_lines.iter().map(|line| line.len()).max().unwrap_or(0).max(50);

    // Top border
    println!("┌{}┐", "─".repeat(max_width + 2));

    // Print each line with proper padding
    for (i, line) in lines.iter().enumerate() {
        let text_len = text_lines[i].len();
        let padding = " ".repeat(max_width - text_len);
        println!("│ {}{} │", line, padding);
    }

    // Bottom border
    println!("└{}┘", "─".repeat(max_width + 2));

    // Timing information (dimmed)
    println!(
        "\x1b[2mTiming: {:.2}ms total (config: {:.2}ms | render: {:.2}ms)\x1b[0m",
        total_time.as_secs_f64() * 1000.0,
        config_time.as_secs_f64() * 1000.0,
        render_time.as_secs_f64() * 1000.0
    );

    // Provider timing breakdown (dimmed)
    if !provider_timings.is_empty() {
        let provider_times: Vec<String> = provider_timings
            .iter()
            .map(|t| format!("{}: {:.2}ms", t.name, t.duration.as_secs_f64() * 1000.0))
            .collect();
        println!("\x1b[2m        {}\x1b[0m", provider_times.join(" | "));
    }
}

/// Print debug information in a classy box to stderr
fn print_debug_box(
    config_path: &PathBuf,
    config_time: std::time::Duration,
    render_time: std::time::Duration,
    total_time: std::time::Duration,
    provider_timings: &[providers::ProviderTiming],
) {
    let config_str = format!("📄 Config: {}", config_path.display());
    let timing_str = format!(
        "⏱️  Timing: {:.2}ms (config: {:.2}ms | render: {:.2}ms)",
        total_time.as_secs_f64() * 1000.0,
        config_time.as_secs_f64() * 1000.0,
        render_time.as_secs_f64() * 1000.0
    );

    // Build provider timing strings
    let provider_strs: Vec<String> = provider_timings
        .iter()
        .map(|t| format!("   {}: {:.2}ms", t.name, t.duration.as_secs_f64() * 1000.0))
        .collect();

    // Calculate display width (accounting for emoji being 2 chars wide)
    // Each line has 1 emoji (2 char width) but counts as more bytes
    let display_width = |s: &str| {
        // Count chars but emojis display as 2 wide
        let char_count = s.chars().count();
        let emoji_count = s.chars().filter(|c| *c as u32 > 0x1F000).count();
        char_count + emoji_count // Add extra width for emojis
    };

    let config_width = display_width(&config_str);
    let timing_width = display_width(&timing_str);

    // Calculate widths for provider timings
    let provider_widths: Vec<usize> = provider_strs.iter().map(|s| display_width(s)).collect();
    let max_provider_width = provider_widths.iter().max().copied().unwrap_or(0);

    let max_width = config_width.max(timing_width).max(max_provider_width).max(40);

    // Top border (account for emoji in header)
    let header = "┌─ 🔍 twig debug ";
    let header_width = display_width(header);
    eprintln!("{}{}┐", header, "─".repeat(max_width + 2 - header_width));

    // Content lines
    eprintln!("│ {}{} │", config_str, " ".repeat(max_width - config_width));
    eprintln!("│ {}{} │", timing_str, " ".repeat(max_width - timing_width));

    // Provider timing lines
    for (provider_str, width) in provider_strs.iter().zip(provider_widths.iter()) {
        eprintln!("│ {}{} │", provider_str, " ".repeat(max_width - width));
    }

    // Bottom border
    eprintln!("└{}┘", "─".repeat(max_width + 2));
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
            name: None,
            format: "%H:%M:%S".to_string(),
        }),
        hostname: Some(HostnameConfig { name: None }),
        cwd: Some(CwdConfig { name: None }),
        git: None,
        prompt: PromptConfig {
            format: "{time:cyan} {\"@\":yellow,bold} {hostname:magenta} {cwd:green} {\"$\":white,bold} ".to_string(),
        },
    }
}

/// Template substitution with color/style support
/// Supports:
/// - {var} - plain variable
/// - {var:color} - variable with color
/// - {var:color,style} - variable with color and style
/// - {"text":color} - literal text with color
/// - {$ENV_VAR} - environment variable
/// - {$ENV_VAR:color} - environment variable with color
fn substitute_variables(
    template: &str,
    variables: &HashMap<String, String>,
    formatter: &dyn ShellFormatter,
) -> String {
    // Match {anything} patterns
    let re = Regex::new(r"\{([^}]+)\}").unwrap();

    re.replace_all(template, |caps: &regex::Captures| {
        let content = &caps[1];

        // Check if it's a literal: "text":color
        if content.starts_with('"') {
            return handle_literal(content, formatter);
        }

        // Otherwise it's a variable: var or var:color or var:color,style
        handle_variable(content, variables, formatter)
    })
    .to_string()
}

/// Handle literal text: "text":color or "text":color,style
fn handle_literal(content: &str, formatter: &dyn ShellFormatter) -> String {
    // Parse: "text":color or "text":color,style
    if let Some(colon_pos) = content.find(':') {
        let text_part = &content[..colon_pos];
        let style_part = &content[colon_pos + 1..];

        // Extract text from quotes
        let text = text_part.trim_matches('"');

        // Apply color/style
        colorize(text, style_part, formatter)
    } else {
        // No color specified, just remove quotes
        content.trim_matches('"').to_string()
    }
}

/// Handle variable: var or var:color or var:color,style
/// Also handles environment variables: $VAR or $VAR:color
fn handle_variable(
    content: &str,
    variables: &HashMap<String, String>,
    formatter: &dyn ShellFormatter,
) -> String {
    // Parse: var or var:color or var:color,style
    let parts: Vec<&str> = content.split(':').collect();

    let var_name = parts[0];
    let style_spec = parts.get(1).copied();

    // Get variable value
    let value = if var_name.starts_with('$') {
        // Environment variable: {$USER}, {$HOME}, etc.
        let env_var = &var_name[1..]; // Strip the '$'
        std::env::var(env_var).unwrap_or_else(|_| String::new()) // Empty string if not found
    } else {
        // Regular variable from config
        variables
            .get(var_name)
            .cloned()
            .unwrap_or_else(String::new) // Return empty string if variable not found
    };

    // Apply color/style if specified
    if let Some(style) = style_spec {
        colorize(&value, style, formatter)
    } else {
        value
    }
}

/// Apply ANSI color and style codes to text
/// style_spec can be: "color" or "color,style" or "color,style1,style2"
fn colorize(text: &str, style_spec: &str, formatter: &dyn ShellFormatter) -> String {
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
        // Build ANSI codes
        let ansi_code = format!("\x1b[{}m", codes.join(";"));
        let reset_code = "\x1b[0m";

        // Use formatter to wrap codes appropriately for the shell
        formatter.format_ansi(&ansi_code, text, reset_code)
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

/// Discover all variables used in a template (excluding $ENV vars and literals)
fn discover_variables(template: &str) -> Vec<String> {
    let re = Regex::new(r"\{([^}]+)\}").unwrap();
    let mut vars = Vec::new();

    for cap in re.captures_iter(template) {
        let content = &cap[1];

        // Skip literals ("text":color)
        if content.starts_with('"') {
            continue;
        }

        // Skip environment variables ($VAR)
        if content.starts_with('$') {
            continue;
        }

        // Extract variable name (before any : for colors)
        let var_name = content.split(':').next().unwrap();

        if !vars.contains(&var_name.to_string()) {
            vars.push(var_name.to_string());
        }
    }

    vars
}

/// Apply default configs for variables used in template but missing config sections
fn apply_implicit_sections(config: &mut Config, template: &str) {
    let registry = providers::ProviderRegistry::new();
    let vars = discover_variables(template);

    for var in vars {
        let prefix = var.split('_').next().unwrap_or(&var);

        if let Some(provider) = registry.get_by_section(prefix) {
            let defaults = provider.default_config();

            for (section_name, default_value) in defaults {
                if !config.has_section(&section_name) {
                    config.add_implicit_section(section_name, default_value);
                }
            }
        }
    }
}
