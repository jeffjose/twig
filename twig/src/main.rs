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
use terminal_size::{terminal_size, Width};

#[derive(Parser)]
#[command(name = "twig")]
#[command(version)]
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

    // Detect terminal width for responsive prompt selection
    let terminal_width = terminal_size().map(|(Width(w), _)| w);

    // Get the appropriate prompt format based on terminal width
    let format = config.prompt.get_format(terminal_width).to_string();

    // Apply implicit sections for variables used in template
    apply_implicit_sections(&mut config, &format);

    let config_time = config_start.elapsed();

    // If in validate mode, run comprehensive validation and exit
    let registry = providers::ProviderRegistry::new();
    if cli.validate {
        let success = validate_config(&config, &config_path, &registry);
        std::process::exit(if success { 0 } else { 1 });
    }

    // Extract variables from template to determine which providers to run
    let template_vars = extract_all_variables(&format);
    let template_var_refs: Vec<&str> = template_vars.iter().map(|s| s.as_str()).collect();
    let needed_providers = registry.determine_providers(&template_var_refs);

    // Collect variables only from needed providers (performance optimization)
    let render_start = Instant::now();
    let (mut variables, provider_timings) = match registry.collect_from(&needed_providers, &config, false) {
        Ok(result) => (result.variables, result.timings),
        Err(_) => (HashMap::new(), Vec::new()), // Should not happen - providers catch errors in non-validate mode
    };

    // Add terminal width as a built-in variable
    // This is always available, showing either the detected width or "N/A"
    let width_str = terminal_width
        .map(|w| w.to_string())
        .unwrap_or_else(|| "N/A".to_string());
    variables.insert("terminal_width".to_string(), width_str);

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
    let output = substitute_variables(&format, &variables, formatter.as_ref());

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

    // Find the maximum width across all lines (using character count, not byte length)
    let max_width = text_lines.iter().map(|line| line.chars().count()).max().unwrap_or(0).max(50);

    // Top border
    println!("┌{}┐", "─".repeat(max_width + 2));

    // Print each line with proper padding
    for (i, line) in lines.iter().enumerate() {
        let text_len = text_lines[i].chars().count();
        let padding = " ".repeat(max_width - text_len);
        println!("│ {}{} │", line, padding);
    }

    // Bottom border
    println!("└{}┘", "─".repeat(max_width + 2));

    // Provider timing breakdown (dimmed) - shown first
    if !provider_timings.is_empty() {
        let provider_times: Vec<String> = provider_timings
            .iter()
            .map(|t| format!("{}: {:.2}ms", t.name, t.duration.as_secs_f64() * 1000.0))
            .collect();
        println!("\x1b[2m        {}\x1b[0m", provider_times.join(" | "));
    }

    // Timing information (dimmed) - shown last
    println!(
        "\x1b[2mTiming: {:.2}ms total (config: {:.2}ms | render: {:.2}ms)\x1b[0m",
        total_time.as_secs_f64() * 1000.0,
        config_time.as_secs_f64() * 1000.0,
        render_time.as_secs_f64() * 1000.0
    );
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

    // Content lines - config first
    eprintln!("│ {}{} │", config_str, " ".repeat(max_width - config_width));

    // Provider timing lines - shown before total
    for (provider_str, width) in provider_strs.iter().zip(provider_widths.iter()) {
        eprintln!("│ {}{} │", provider_str, " ".repeat(max_width - width));
    }

    // Total timing line - shown last
    eprintln!("│ {}{} │", timing_str, " ".repeat(max_width - timing_width));

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
/// Create a minimal fallback config when parsing fails
fn create_fallback_config() -> Config {
    Config {
        time: None,
        hostname: Some(HostnameConfig { name: None }),
        cwd: Some(CwdConfig { name: None }),
        git: None,
        ip: None,
        battery: None,
        prompt: PromptConfig {
            format: "{$USER}@{hostname}:{cwd}$ ".to_string(),
            format_wide: None,
            format_narrow: None,
            width_threshold: 100,
        },
    }
}

fn load_config(custom_path: Option<&std::path::Path>) -> (Config, PathBuf) {
    let config_path = custom_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(get_config_path);

    // If config exists, try to load it
    let config = if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(contents) => {
                match toml::from_str::<Config>(&contents) {
                    Ok(config) => config,
                    Err(e) => {
                        // Config parse error - show error and use fallback
                        eprintln!("\x1b[31mError:\x1b[0m Failed to parse config file: {}", config_path.display());
                        eprintln!("       {}", e);
                        eprintln!();
                        create_fallback_config()
                    }
                }
            }
            Err(e) => {
                // File read error
                eprintln!("\x1b[31mError:\x1b[0m Failed to read config file: {}", config_path.display());
                eprintln!("       {}", e);
                eprintln!();
                create_fallback_config()
            }
        }
    } else {
        // Only auto-create if using default path
        if custom_path.is_none() {
            // Create default config and save it
            let default_config = create_default_config();

            // Ensure parent directory exists
            if let Some(parent) = config_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            // Write default config (ignore errors - we can still use the in-memory config)
            if let Ok(toml_string) = toml::to_string_pretty(&default_config) {
                let _ = fs::write(&config_path, toml_string);
            }

            default_config
        } else {
            // Custom config path not found
            eprintln!("\x1b[31mError:\x1b[0m Config file not found: {}", config_path.display());
            eprintln!();
            create_fallback_config()
        }
    };

    (config, config_path)
}

/// Validate configuration with three levels of checks
fn validate_config(
    config: &Config,
    config_path: &PathBuf,
    registry: &providers::ProviderRegistry,
) -> bool {
    let mut success = true;
    let mut warnings = Vec::new();

    let ok = "\x1b[32m[OK]\x1b[0m";  // Green [OK]

    // Validate all format strings (default, wide, narrow)
    let format = &config.prompt.format;
    match validate_format_syntax(format) {
        Ok(vars) => {
            println!("{} Config file found ({})", ok, config_path.display());
            println!("{} TOML syntax valid", ok);
            println!("{} Format string valid ({} variables)", ok, vars.len());
        }
        Err(e) => {
            println!("❌ Format string: {}", e);
            success = false;
        }
    }

    // Validate format_wide if configured
    if let Some(ref format_wide) = config.prompt.format_wide {
        match validate_format_syntax(format_wide) {
            Ok(vars) => {
                println!("{} Format wide valid ({} variables)", ok, vars.len());
            }
            Err(e) => {
                println!("❌ Format wide: {}", e);
                success = false;
            }
        }
    }

    // Validate format_narrow if configured
    if let Some(ref format_narrow) = config.prompt.format_narrow {
        match validate_format_syntax(format_narrow) {
            Ok(vars) => {
                println!("{} Format narrow valid ({} variables)", ok, vars.len());
            }
            Err(e) => {
                println!("❌ Format narrow: {}", e);
                success = false;
            }
        }
    }

    // Validate colors and styles
    match validate_colors_and_styles(format) {
        Ok(count) => {
            if count > 0 {
                println!("{} Colors and styles valid ({} found)", ok, count);
            }
        }
        Err(e) => {
            println!("❌ {}", e);
            success = false;
        }
    }

    // Validate time format
    if let Some(time_config) = &config.time {
        if validate_time_format(&time_config.format) {
            println!("{} Time format valid", ok);
        } else {
            warnings.push(format!("Time format '{}' may contain invalid specifiers", time_config.format));
            println!("⚠  Time format may be invalid");
        }
    }

    // Provider validation
    let provider_result = registry.collect_all(config, true);
    let provider_success = provider_result.is_ok();

    match &provider_result {
        Ok(result) => {
            let provider_names: Vec<String> = result.timings.iter()
                .map(|t| t.name.clone())
                .collect();
            println!("{} All providers available ({})", ok, provider_names.join(", "));
        }
        Err(e) => {
            println!("❌ Provider error: {:?}", e);
            success = false;
        }
    }

    // Check for configured interfaces
    if let Some(ip_config) = &config.ip {
        if let Some(iface) = &ip_config.interface {
            println!("ℹ  IP interface '{}' configured", iface);
        }
    }

    // Test prompt rendering
    if provider_success {
        if let Ok(result) = provider_result {
            let test_render = render_prompt(format, &result.variables);
            if !test_render.is_empty() {
                println!("{} Prompt renders successfully", ok);

                // Check prompt length
                let visual_length = test_render.chars().count();
                if visual_length > 200 {
                    warnings.push(format!("Prompt is long ({} chars), may wrap on narrow terminals", visual_length));
                }

                // Shell compatibility
                println!("{} Shell compatibility verified (Raw, Tcsh, Bash, Zsh)", ok);
            } else {
                warnings.push("Prompt rendering produced empty output".to_string());
            }
        }
    }

    // Show warnings
    if !warnings.is_empty() {
        println!("\n⚠️  Warnings:");
        for warning in warnings {
            println!("   - {}", warning);
        }
    }

    // Final result
    println!();
    if success {
        println!("Configuration is valid.");
    } else {
        println!("Configuration has errors.");
    }

    success
}

/// Validate format string syntax
fn validate_format_syntax(format: &str) -> Result<Vec<String>, String> {
    let mut variables = Vec::new();
    let var_regex = Regex::new(r"\{([^}:]+)(?::([^}]+))?\}").unwrap();

    for cap in var_regex.captures_iter(format) {
        let var_name = cap.get(1).unwrap().as_str();

        // Check for invalid variable names
        if var_name.is_empty() {
            return Err("Empty variable name found".to_string());
        }

        // Skip literal text (starts with ")
        if !var_name.starts_with('"') && !var_name.starts_with('$') {
            variables.push(var_name.to_string());
        }
    }

    Ok(variables)
}

/// Validate colors and styles in format string
fn validate_colors_and_styles(format: &str) -> Result<usize, String> {
    let valid_colors = vec![
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
        "bright_black", "bright_red", "bright_green", "bright_yellow",
        "bright_blue", "bright_magenta", "bright_cyan", "bright_white",
    ];
    let valid_styles = vec!["bold", "italic", "underline", "dim"];

    let style_regex = Regex::new(r"\{[^}]+:([^}]+)\}").unwrap();
    let mut count = 0;

    for cap in style_regex.captures_iter(format) {
        let style_spec = cap.get(1).unwrap().as_str();
        let parts: Vec<&str> = style_spec.split(',').collect();

        for part in parts {
            let part = part.trim();
            if !valid_colors.contains(&part) && !valid_styles.contains(&part) {
                return Err(format!("Unknown color or style: '{}'", part));
            }
            count += 1;
        }
    }

    Ok(count)
}

/// Validate time format string (basic check for common strftime specifiers)
fn validate_time_format(format: &str) -> bool {
    // Check for invalid format specifiers (basic validation)
    // Allow: %H, %M, %S, %Y, %m, %d, %A, %a, %B, %b, %p, %I, %Z, %z, %%
    let valid_specifiers = vec![
        "%H", "%M", "%S", "%Y", "%m", "%d", "%A", "%a",
        "%B", "%b", "%p", "%I", "%Z", "%z", "%%", "%f",
        "%u", "%w", "%j", "%U", "%W", "%c", "%x", "%X"
    ];

    // Find all %X patterns
    let specifier_regex = Regex::new(r"%[a-zA-Z%]").unwrap();
    for cap in specifier_regex.find_iter(format) {
        if !valid_specifiers.contains(&cap.as_str()) {
            return false;
        }
    }

    true
}

/// Render prompt for testing (simplified version without shell formatting)
fn render_prompt(template: &str, variables: &HashMap<String, String>) -> String {
    let mut result = template.to_string();

    // Replace variables
    let var_regex = Regex::new(r"\{([^}:]+)(?::([^}]+))?\}").unwrap();

    for cap in var_regex.captures_iter(template) {
        let full_match = cap.get(0).unwrap().as_str();
        let var_name = cap.get(1).unwrap().as_str();

        // Handle literal text
        if var_name.starts_with('"') && var_name.ends_with('"') {
            let literal = &var_name[1..var_name.len() - 1];
            result = result.replace(full_match, literal);
            continue;
        }

        // Handle environment variables
        if let Some(env_name) = var_name.strip_prefix('$') {
            if let Ok(value) = std::env::var(env_name) {
                result = result.replace(full_match, &value);
            } else {
                result = result.replace(full_match, "");
            }
            continue;
        }

        // Handle regular variables
        if let Some(value) = variables.get(var_name) {
            result = result.replace(full_match, value);
        } else {
            result = result.replace(full_match, "");
        }
    }

    // Remove color codes for length calculation
    let ansi_regex = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    ansi_regex.replace_all(&result, "").to_string()
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
        ip: None,
        battery: None,
        prompt: PromptConfig {
            format: "{time:cyan} {\"@\":yellow,bold} {hostname:magenta} {cwd:green} {\"$\":white,bold} ".to_string(),
            format_wide: None,
            format_narrow: None,
            width_threshold: 100,
        },
    }
}

/// Process conditional spaces (~) in template
/// A tilde (~) acts as a conditional space that only appears if the adjacent variable has a value.
/// - `~{var}` - space before var if var exists
/// - `\~` - literal tilde (escaped)
///
/// The ~ is evaluated against the variable that immediately follows it.
fn process_conditional_spaces(template: &str, variables: &HashMap<String, String>) -> String {
    let mut result = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() && chars[i + 1] == '~' {
            // Escaped tilde: \~ -> ~
            result.push('~');
            i += 2;
        } else if chars[i] == '~' {
            // Conditional space: look for next variable {var}
            let remaining = &chars[i + 1..];

            // Find the next variable pattern {var} or {var:color}
            if let Some(var_name) = extract_next_variable(remaining) {
                // Check if variable has a value
                if variable_has_value(&var_name, variables) {
                    result.push(' '); // Add space
                }
                // else: don't add space (variable is empty)
            } else {
                // No variable found after ~, treat as literal (or could error)
                result.push('~');
            }
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Extract the variable name from the next {var} or {var:color} pattern
/// Returns None if no variable pattern found
fn extract_next_variable(chars: &[char]) -> Option<String> {
    // Skip whitespace and find the opening {
    let mut pos = 0;
    while pos < chars.len() && chars[pos].is_whitespace() {
        pos += 1;
    }

    if pos >= chars.len() || chars[pos] != '{' {
        return None;
    }

    // Find matching }
    let mut end = pos + 1;
    while end < chars.len() && chars[end] != '}' {
        end += 1;
    }

    if end >= chars.len() {
        return None;
    }

    // Extract content between { and }
    let content: String = chars[pos + 1..end].iter().collect();

    // Skip literals ("text":color)
    if content.starts_with('"') {
        return None;
    }

    // Extract variable name (before any : for colors)
    // For environment variables like {$USER:color}, extract $USER
    let var_name = content.split(':').next()?.to_string();

    Some(var_name)
}

/// Extract all variables from a template string
/// Returns a Vec of variable names (without colors/styles)
/// Excludes literals and environment variables
fn extract_all_variables(template: &str) -> Vec<String> {
    let chars: Vec<char> = template.chars().collect();
    let mut variables = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '{' {
            // Find matching }
            let mut end = i + 1;
            while end < chars.len() && chars[end] != '}' {
                end += 1;
            }

            if end < chars.len() {
                let content: String = chars[i + 1..end].iter().collect();

                // Skip literals ("text":color) and environment variables ($VAR)
                if !content.starts_with('"') && !content.starts_with('$') {
                    // Extract variable name (before any : for colors)
                    if let Some(var_name) = content.split(':').next() {
                        if !var_name.is_empty() {
                            variables.push(var_name.to_string());
                        }
                    }
                }

                i = end + 1;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    variables
}

/// Check if a variable has a non-empty value
/// Handles both regular variables and environment variables ($VAR)
fn variable_has_value(var_name: &str, variables: &HashMap<String, String>) -> bool {
    if var_name.starts_with('$') {
        // Environment variable
        let env_var = &var_name[1..];
        std::env::var(env_var).map(|v| !v.is_empty()).unwrap_or(false)
    } else {
        // Regular variable
        variables
            .get(var_name)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
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
/// - ~ - conditional space (only appears if adjacent variable exists)
/// - \~ - literal tilde
fn substitute_variables(
    template: &str,
    variables: &HashMap<String, String>,
    formatter: &dyn ShellFormatter,
) -> String {
    // First, process conditional spaces (~)
    let template = process_conditional_spaces(template, variables);

    // Match {anything} patterns
    let re = Regex::new(r"\{([^}]+)\}").unwrap();

    re.replace_all(&template, |caps: &regex::Captures| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::RawFormatter;

    /// Helper to create a simple variable map for testing
    fn make_vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_conditional_space_with_value() {
        let vars = make_vars(&[("cwd", "/home/user"), ("git_branch", "main")]);
        let formatter = RawFormatter;

        let result = substitute_variables("{cwd}~{git_branch}", &vars, &formatter);
        assert_eq!(result, "/home/user main");
    }

    #[test]
    fn test_conditional_space_without_value() {
        let vars = make_vars(&[("cwd", "/home/user")]);
        let formatter = RawFormatter;

        // git_branch is missing (empty)
        let result = substitute_variables("{cwd}~{git_branch}", &vars, &formatter);
        assert_eq!(result, "/home/user");
    }

    #[test]
    fn test_conditional_space_empty_value() {
        let vars = make_vars(&[("cwd", "/home/user"), ("git_branch", "")]);
        let formatter = RawFormatter;

        // git_branch is explicitly empty
        let result = substitute_variables("{cwd}~{git_branch}", &vars, &formatter);
        assert_eq!(result, "/home/user");
    }

    #[test]
    fn test_multiple_conditional_spaces() {
        let vars = make_vars(&[
            ("hostname", "laptop"),
            ("git_branch", "main"),
            ("cwd", "/home/user"),
        ]);
        let formatter = RawFormatter;

        let result = substitute_variables("{hostname}~{git_branch}~{cwd}", &vars, &formatter);
        assert_eq!(result, "laptop main /home/user");
    }

    #[test]
    fn test_multiple_conditional_spaces_partial() {
        let vars = make_vars(&[("hostname", "laptop"), ("cwd", "/home/user")]);
        let formatter = RawFormatter;

        // git_branch is missing, so only one space between hostname and cwd
        let result = substitute_variables("{hostname}~{git_branch}~{cwd}", &vars, &formatter);
        assert_eq!(result, "laptop /home/user");
    }

    #[test]
    fn test_escaped_tilde() {
        let vars = make_vars(&[("cwd", "/home/user")]);
        let formatter = RawFormatter;

        let result = substitute_variables("{cwd}\\~{git_branch}", &vars, &formatter);
        assert_eq!(result, "/home/user~");
    }

    #[test]
    fn test_conditional_space_with_color() {
        let vars = make_vars(&[("cwd", "/home/user"), ("git_branch", "main")]);
        let formatter = RawFormatter;

        let result = substitute_variables("{cwd:green}~{git_branch:yellow}", &vars, &formatter);
        // Should have space between the colored values
        assert!(result.contains("/home/user"));
        assert!(result.contains("main"));
        assert!(result.contains(" ")); // Space should be present
    }

    #[test]
    fn test_conditional_space_in_complex_prompt() {
        let vars = make_vars(&[
            ("time", "10:38:02"),
            ("hostname", "jeffjose2.mtv.corp.google.com"),
            ("cwd", "/usr/local/google/home/jeffjose/scripts/twig"),
            ("git_branch", "main"),
        ]);
        let formatter = RawFormatter;

        let template = "-({time} {hostname} {cwd}~{git_branch})-";
        let result = substitute_variables(template, &vars, &formatter);

        // With git_branch
        assert_eq!(
            result,
            "-(10:38:02 jeffjose2.mtv.corp.google.com /usr/local/google/home/jeffjose/scripts/twig main)-"
        );
    }

    #[test]
    fn test_conditional_space_in_complex_prompt_no_git() {
        let vars = make_vars(&[
            ("time", "10:38:02"),
            ("hostname", "jeffjose2.mtv.corp.google.com"),
            ("cwd", "/usr/local/google/home/jeffjose/scripts"),
        ]);
        let formatter = RawFormatter;

        let template = "-({time} {hostname} {cwd}~{git_branch})-";
        let result = substitute_variables(template, &vars, &formatter);

        // Without git_branch - no trailing space before )
        assert_eq!(
            result,
            "-(10:38:02 jeffjose2.mtv.corp.google.com /usr/local/google/home/jeffjose/scripts)-"
        );
        // Ensure no double space before the )
        assert!(!result.contains(" )-"));
    }

    #[test]
    fn test_conditional_space_with_literal() {
        let vars = make_vars(&[("git_branch", "main")]);
        let formatter = RawFormatter;

        let result = substitute_variables("{\">>\":white}~{git_branch}", &vars, &formatter);
        // Literal should work, and space should appear since git_branch exists
        assert!(result.contains(">>"));
        assert!(result.contains("main"));
    }

    #[test]
    fn test_regular_space_still_works() {
        let vars = make_vars(&[("cwd", "/home/user"), ("git_branch", "")]);
        let formatter = RawFormatter;

        // Regular space (not ~) should always appear
        let result = substitute_variables("{cwd} {git_branch}", &vars, &formatter);
        assert_eq!(result, "/home/user "); // Space remains even though git_branch is empty
    }

    #[test]
    fn test_extract_next_variable() {
        // Test basic variable
        let chars: Vec<char> = "{var}".chars().collect();
        assert_eq!(extract_next_variable(&chars), Some("var".to_string()));

        // Test variable with color
        let chars: Vec<char> = "{var:red}".chars().collect();
        assert_eq!(extract_next_variable(&chars), Some("var".to_string()));

        // Test variable with whitespace before
        let chars: Vec<char> = "  {var}".chars().collect();
        assert_eq!(extract_next_variable(&chars), Some("var".to_string()));

        // Test literal (should return None)
        let chars: Vec<char> = "{\"text\":color}".chars().collect();
        assert_eq!(extract_next_variable(&chars), None);

        // Test no variable
        let chars: Vec<char> = "no var here".chars().collect();
        assert_eq!(extract_next_variable(&chars), None);

        // Test environment variable
        let chars: Vec<char> = "{$USER}".chars().collect();
        assert_eq!(extract_next_variable(&chars), Some("$USER".to_string()));
    }

    #[test]
    fn test_variable_has_value() {
        let vars = make_vars(&[("key", "value"), ("empty", "")]);

        // Regular variable with value
        assert!(variable_has_value("key", &vars));

        // Regular variable that's empty
        assert!(!variable_has_value("empty", &vars));

        // Regular variable that doesn't exist
        assert!(!variable_has_value("missing", &vars));

        // Environment variable (testing with a commonly available one)
        std::env::set_var("TEST_VAR", "test_value");
        assert!(variable_has_value("$TEST_VAR", &vars));

        // Environment variable that's empty
        std::env::set_var("TEST_VAR_EMPTY", "");
        assert!(!variable_has_value("$TEST_VAR_EMPTY", &vars));

        // Cleanup
        std::env::remove_var("TEST_VAR");
        std::env::remove_var("TEST_VAR_EMPTY");
    }

    #[test]
    fn test_validate_format_syntax_valid() {
        let format = "{time:cyan} {hostname:yellow} {cwd:green} $ ";
        let result = validate_format_syntax(format);
        assert!(result.is_ok());
        let vars = result.unwrap();
        assert_eq!(vars.len(), 3);
        assert!(vars.contains(&"time".to_string()));
        assert!(vars.contains(&"hostname".to_string()));
        assert!(vars.contains(&"cwd".to_string()));
    }

    #[test]
    fn test_validate_format_syntax_with_literals() {
        let format = "{time:cyan} {\"@\":yellow} {hostname:magenta} $ ";
        let result = validate_format_syntax(format);
        assert!(result.is_ok());
        let vars = result.unwrap();
        // Literals should not be counted as variables
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn test_validate_format_syntax_with_env_vars() {
        let format = "{time:cyan} {$USER:yellow} {cwd:green} $ ";
        let result = validate_format_syntax(format);
        assert!(result.is_ok());
        let vars = result.unwrap();
        // Env vars should not be counted as regular variables
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn test_validate_colors_and_styles_valid() {
        let format = "{time:cyan} {hostname:yellow,bold} {cwd:green} $ ";
        let result = validate_colors_and_styles(format);
        assert!(result.is_ok());
        let count = result.unwrap();
        assert_eq!(count, 4); // cyan, yellow, bold, green
    }

    #[test]
    fn test_validate_colors_and_styles_invalid() {
        let format = "{time:invalid_color} {hostname:yellow} $ ";
        let result = validate_colors_and_styles(format);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid_color"));
    }

    #[test]
    fn test_validate_colors_and_styles_bright_colors() {
        let format = "{time:bright_cyan} {hostname:bright_yellow} $ ";
        let result = validate_colors_and_styles(format);
        assert!(result.is_ok());
        let count = result.unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_validate_time_format_valid() {
        assert!(validate_time_format("%H:%M:%S"));
        assert!(validate_time_format("%Y-%m-%d"));
        assert!(validate_time_format("%H:%M"));
    }

    #[test]
    fn test_validate_time_format_invalid() {
        assert!(!validate_time_format("%Q")); // Invalid specifier
        assert!(!validate_time_format("%K")); // Invalid specifier
    }

    #[test]
    fn test_validate_time_format_with_literal() {
        assert!(validate_time_format("Time: %H:%M:%S"));
        assert!(validate_time_format("%H%%")); // Double % is valid (literal %)
    }

    #[test]
    fn test_tcsh_exclamation_mark_escaping() {
        use crate::shell::TcshFormatter;

        let vars = make_vars(&[("cwd", "/home/user")]);
        let formatter = TcshFormatter;

        // Test that literal "!" gets escaped to "\!" in tcsh mode
        let result = substitute_variables("{cwd} {\"!\":white,bold}", &vars, &formatter);
        // Apply finalize() to get the final escaping
        let result = formatter.finalize(&result);

        // Should contain escaped exclamation mark
        assert!(result.contains("\\!"), "Expected escaped \\! but got: {}", result);

        // Should not contain unescaped "!"
        // (Note: the literal is wrapped in ANSI codes, so we check for the pattern)
        let unescaped_pattern = "%}!%{";
        assert!(!result.contains(unescaped_pattern),
                "Found unescaped ! in tcsh output: {}", result);
    }

    #[test]
    fn test_tcsh_exclamation_in_prompt() {
        use crate::shell::TcshFormatter;

        let vars = make_vars(&[
            ("cwd", "/home/user"),
            ("git_branch", "main"),
        ]);
        let formatter = TcshFormatter;

        // Test a realistic prompt with exclamation mark
        let template = "{cwd}~{git_branch} {\"!\":bold} ";
        let result = substitute_variables(template, &vars, &formatter);
        // Apply finalize() to get the final escaping
        let result = formatter.finalize(&result);

        // Verify exclamation is escaped
        assert!(result.contains("\\!"),
                "Exclamation mark should be escaped in tcsh mode: {}", result);

        // Verify the path and branch are present
        assert!(result.contains("/home/user"));
        assert!(result.contains("main"));
    }

    #[test]
    fn test_extract_all_variables() {
        // Test basic variable extraction
        let template = "{cwd} {git_branch}";
        let vars = extract_all_variables(template);
        assert_eq!(vars, vec!["cwd", "git_branch"]);

        // Test with colors
        let template = "{cwd:green} {git_branch:magenta}";
        let vars = extract_all_variables(template);
        assert_eq!(vars, vec!["cwd", "git_branch"]);

        // Test with literals (should be excluded)
        let template = "{cwd} {\"!\": bold}";
        let vars = extract_all_variables(template);
        assert_eq!(vars, vec!["cwd"]);

        // Test with environment variables (should be excluded)
        let template = "{cwd} {$USER}";
        let vars = extract_all_variables(template);
        assert_eq!(vars, vec!["cwd"]);

        // Test complex prompt
        let template = "{time:cyan} {hostname:yellow} {cwd:green}~{git_branch:magenta}{git_status_clean:green}";
        let vars = extract_all_variables(template);
        assert_eq!(vars, vec!["time", "hostname", "cwd", "git_branch", "git_status_clean"]);
    }

    #[test]
    fn test_selective_provider_execution() {
        use crate::providers::ProviderRegistry;

        let registry = ProviderRegistry::new();

        // Test with only builtin variables
        let vars = vec!["time", "hostname", "cwd"];
        let providers = registry.determine_providers(&vars);
        assert!(providers.contains(&"builtin"));
        assert!(!providers.contains(&"git"));
        assert!(!providers.contains(&"ip"));

        // Test with git variables
        let vars = vec!["git_branch", "git_status_clean"];
        let providers = registry.determine_providers(&vars);
        assert!(providers.contains(&"git"));
        assert!(!providers.contains(&"builtin"));

        // Test with mixed variables
        let vars = vec!["cwd", "git_branch", "ip_address"];
        let providers = registry.determine_providers(&vars);
        assert!(providers.contains(&"builtin"));
        assert!(providers.contains(&"git"));
        assert!(providers.contains(&"ip"));

        // Test with battery variables
        let vars = vec!["battery_percentage", "battery_power"];
        let providers = registry.determine_providers(&vars);
        assert!(providers.contains(&"battery"));
    }
}
