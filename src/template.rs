use colored::*;
use std::error::Error;

#[derive(Debug)]
pub enum TemplateError {
    MissingVariable(String),
    InvalidSyntax(String),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::MissingVariable(var) => write!(f, "Missing variable: {}", var),
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

pub fn format_template(
    template: &str,
    variables: &[(&str, &str)],
    show_warnings: bool,
    mode: Option<&str>,
) -> Result<String, TemplateError> {
    // For tcsh mode, process the entire template as one string
    if mode == Some("tcsh") {
        let mut result = template.to_string();

        // First, validate all variables in the template
        let mut pos = 0;
        while let Some(start) = result[pos..].find('{') {
            let start = start + pos;
            if let Some(end) = result[start..].find('}') {
                let end = end + start;
                let var_spec = &result[start + 1..end];
                let var_name = if let Some(colon) = var_spec.find(':') {
                    &var_spec[..colon]
                } else {
                    var_spec
                };

                if !variables.iter().any(|(name, _)| *name == var_name) {
                    return Err(TemplateError::MissingVariable(var_name.to_string()));
                }
                pos = end + 1;
            } else {
                return Err(TemplateError::InvalidSyntax("Unclosed variable".into()));
            }
        }

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
                    let colored_value = apply_color(value, color, show_warnings, Some("tcsh"))?;
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

        // Replace actual newlines with literal "\n"
        let mut final_result = String::new();
        let mut chars = result.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\n' {
                final_result.push_str("\\n");
            } else {
                final_result.push(ch);
            }
        }

        Ok(final_result)
    } else {
        // Original code for non-tcsh mode
        let lines: Vec<&str> = template.lines().collect();
        let mut result_lines = Vec::with_capacity(lines.len());

        for line in lines {
            let mut result = line.to_string();

            // First, validate that all variables in the template are provided
            let mut pos = 0;
            while let Some(start) = result[pos..].find('{') {
                let start = start + pos;
                if let Some(end) = result[start..].find('}') {
                    let end = end + start;
                    let var_spec = &result[start + 1..end];
                    let var_name = if let Some(colon) = var_spec.find(':') {
                        &var_spec[..colon]
                    } else {
                        var_spec
                    };

                    if !variables.iter().any(|(name, _)| *name == var_name) {
                        return Err(TemplateError::MissingVariable(var_name.to_string()));
                    }
                    pos = end + 1;
                } else {
                    return Err(TemplateError::InvalidSyntax("Unclosed variable".into()));
                }
            }

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

            result_lines.push(result);
        }

        // For tcsh mode, preserve literal newlines
        match mode {
            Some("tcsh") => Ok(result_lines.join("\n")),
            _ => {
                // For other modes, filter out empty lines and join
                Ok(result_lines
                    .into_iter()
                    .filter(|line| !line.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        }
    }
}
