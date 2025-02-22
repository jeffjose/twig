use colored::*;
use std::error::Error;

#[derive(Debug)]
pub enum TemplateError {
    InvalidSyntax(String),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::InvalidSyntax(msg) => write!(f, "Invalid syntax: {}", msg),
        }
    }
}

impl Error for TemplateError {}

fn apply_color(
    text: &str,
    color: &str,
    show_warnings: bool,
    mode: Option<&str>,
) -> Result<String, TemplateError> {
    match mode {
        Some("tcsh") => {
            let color_code = match color {
                "red" => "31",
                "green" => "32",
                "yellow" => "33",
                "blue" => "34",
                "magenta" => "35",
                "cyan" => "36",
                "white" => "37",
                "bright_red" => "1;31",
                "bright_green" => "1;32",
                "bright_yellow" => "1;33",
                "bright_blue" => "1;34",
                "bright_magenta" => "1;35",
                "bright_cyan" => "1;36",
                "bright_white" => "1;37",
                unknown => {
                    if show_warnings {
                        eprintln!("Warning: unknown color '{}', using white instead", unknown);
                    }
                    "37"
                }
            };
            Ok(format!("%{{\x1b[{}m%}}{}%{{\x1b[0m%}}", color_code, text))
        }
        None => {
            let result = match color {
                "red" => text.red().to_string(),
                "green" => text.green().to_string(),
                "yellow" => text.yellow().to_string(),
                "blue" => text.blue().to_string(),
                "magenta" => text.magenta().to_string(),
                "cyan" => text.cyan().to_string(),
                "white" => text.white().to_string(),
                "bright_red" => text.bright_red().to_string(),
                "bright_green" => text.bright_green().to_string(),
                "bright_yellow" => text.bright_yellow().to_string(),
                "bright_blue" => text.bright_blue().to_string(),
                "bright_magenta" => text.bright_magenta().to_string(),
                "bright_cyan" => text.bright_cyan().to_string(),
                "bright_white" => text.bright_white().to_string(),
                unknown => {
                    if show_warnings {
                        eprintln!("Warning: unknown color '{}', using white instead", unknown);
                    }
                    text.white().to_string()
                }
            };
            Ok(result)
        }
        Some(unknown_mode) => {
            if show_warnings {
                eprintln!(
                    "Warning: unknown mode '{}', using default colors",
                    unknown_mode
                );
            }
            apply_color(text, color, show_warnings, None)
        }
    }
}

fn validate_variables(template: &str, _variables: &[(&str, &str)]) -> Result<(), TemplateError> {
    // Only validate syntax (unclosed braces)
    let mut pos = 0;
    while let Some(start) = template[pos..].find('{') {
        let start = start + pos;
        if let Some(end) = template[start..].find('}') {
            pos = start + end + 1;
        } else {
            return Err(TemplateError::InvalidSyntax("Unclosed variable".into()));
        }
    }
    Ok(())
}

fn process_variables(
    template: &str,
    variables: &[(&str, &str)],
    show_warnings: bool,
    mode: Option<&str>,
) -> Result<String, TemplateError> {
    let mut result = template.to_string();

    // Process colored variables first (including quoted text)
    for (name, value) in variables {
        let pattern = format!("{{{}:", name);
        let mut position = 0;
        while let Some(start) = result[position..].find(&pattern) {
            let start = start + position;
            let after_var = start + pattern.len();

            if let Some(end) = result[after_var..].find('}') {
                let end = end + after_var;
                let color = &result[after_var..end];
                let colored_value = apply_color(value, color, show_warnings, mode)?;
                result.replace_range(start..end + 1, &colored_value);
                position = start + colored_value.len();
            }
        }
    }

    // Process quoted text (both colored and uncolored)
    let mut position = 0;
    let mut replacements = Vec::new();

    while let Some(start) = result[position..].find("{\"") {
        let start = start + position;
        if let Some(quote_end) = result[start + 2..].find('\"') {
            let quote_end = start + 2 + quote_end;
            let text = &result[start + 2..quote_end];
            
            // Check if there's a color specification
            if result[quote_end + 1..].starts_with(':') {
                if let Some(end) = result[quote_end + 1..].find('}') {
                    let end = quote_end + 1 + end;
                    let color = &result[quote_end + 2..end];
                    let colored_text = apply_color(text, color, show_warnings, mode)?;
                    replacements.push((start..end + 1, colored_text));
                    position = end + 1;
                    continue;
                }
            } else if result[quote_end + 1..].starts_with('}') {
                // No color specification, just replace the quoted text
                replacements.push((start..quote_end + 2, text.to_string()));
                position = quote_end + 2;
                continue;
            }
        }
        position = start + 1;
    }

    // Apply replacements in reverse order to maintain correct indices
    for (range, replacement) in replacements.into_iter().rev() {
        result.replace_range(range, &replacement);
    }

    // Then process non-colored variables
    for (name, value) in variables {
        let pattern = format!("{{{}}}", name);
        while result.contains(&pattern) {
            result = result.replace(&pattern, value);
        }
    }

    // Handle any remaining unmatched variables by keeping them as-is
    let mut pos = 0;
    while let Some(start) = result[pos..].find('{') {
        let start = start + pos;
        if let Some(end) = result[start..].find('}') {
            let end = end + start;
            let var_spec = &result[start..=end];
            let var_name = var_spec[1..end - start].split(':').next().unwrap_or("");

            if show_warnings && !var_name.starts_with('\"') {
                eprintln!("Warning: undefined variable '{}'", var_name);
            }

            pos = end + 1;
        } else {
            pos = start + 1;
        }
    }

    Ok(result)
}

pub fn format_template(
    template: &str,
    variables: &[(&str, &str)],
    show_warnings: bool,
    mode: Option<&str>,
) -> Result<String, TemplateError> {
    // Validate variables first
    validate_variables(template, variables)?;

    if mode == Some("tcsh") {
        // Process variables
        let result = process_variables(template, variables, show_warnings, mode)?;

        // Convert newlines to literal "\n" for tcsh mode
        let mut final_result = String::new();
        for ch in result.chars() {
            if ch == '\n' {
                final_result.push_str("\\n");
            } else {
                final_result.push(ch);
            }
        }
        Ok(final_result)
    } else {
        // For non-tcsh mode, process line by line
        let lines: Vec<&str> = template.lines().collect();
        let mut result_lines = Vec::with_capacity(lines.len());

        for line in lines {
            let processed = process_variables(line, variables, show_warnings, mode)?;
            result_lines.push(processed);
        }

        // Filter empty lines in non-tcsh mode
        Ok(result_lines
            .into_iter()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"))
    }
}
