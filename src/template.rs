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
        Some("debug_format") => Ok(format!(
            "{{{}}}{}{{{}}}",
            color,
            text,
            format!("/{}", color)
        )),
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

    // Process colored variables first
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

            if show_warnings {
                eprintln!("Warning: undefined variable '{}'", var_name);
            }

            // Instead of replacing with empty string, just advance the position
            pos = end + 1;
        } else {
            pos = start + 1;
        }
    }

    Ok(result)
}

pub fn format_template(
    template: &str,
    vars: &[(&str, &str)],
    validate: bool,
    mode: Option<&str>,
) -> Result<String, Box<dyn Error>> {
    if mode == Some("tcsh_debug") {
        let colored_output = format_template(template, vars, validate, None)?;
        let debug_output = process_variables(template, vars, validate, Some("debug_format"))?;
        return Ok(format!("{}\nDEBUG: \n{}", colored_output, debug_output));
    }

    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            let mut var_spec = String::new();
            while let Some(&next_char) = chars.peek() {
                if next_char == '}' {
                    chars.next();
                    if !var_spec.is_empty() {
                        let (var_name, color) = parse_template_var(&var_spec);

                        // Find the variable value
                        if let Some((_, value)) = vars.iter().find(|(name, _)| *name == var_name) {
                            let formatted = match color {
                                Some(color_name) => apply_color(value, &color_name, validate, mode),
                                None => Ok(value.to_string()),
                            }?;
                            result.push_str(&formatted);
                        } else if validate {
                            eprintln!("Warning: variable '{}' not found", var_name);
                        }
                    }
                    break;
                }
                var_spec.push(chars.next().unwrap());
            }
        } else {
            result.push(c);
        }
    }

    // Validate variables first
    validate_variables(template, vars)?;

    if mode == Some("tcsh") {
        // Process variables
        let result = process_variables(template, vars, validate, mode)?;

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
            let processed = process_variables(line, vars, validate, mode)?;
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

// Fix the split_var_and_color function
pub fn split_var_and_color(var_spec: &str) -> (&str, Option<&str>) {
    if let Some(colon_pos) = var_spec.find(':') {
        let (var, color) = var_spec.split_at(colon_pos);
        // Skip the colon
        (var, Some(&color[1..]))
    } else {
        (var_spec, None)
    }
}

pub fn parse_template_var(var: &str) -> (String, Option<String>) {
    if var.starts_with('$') {
        // For environment variables
        let (name, color) = split_var_and_color(&var[1..]); // Skip the $ prefix
        (format!("${}", name), color.map(String::from))
    } else {
        // For regular variables
        let (name, color) = split_var_and_color(var);
        (name.to_string(), color.map(String::from))
    }
}
