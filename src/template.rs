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

fn apply_color(text: &str, color: &str, show_warnings: bool) -> Result<String, TemplateError> {
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

pub fn format_template(template: &str, variables: &[(&str, &str)], show_warnings: bool) -> Result<String, TemplateError> {
    let mut result = template.to_string();
    
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
                let colored_value = apply_color(value, color, show_warnings)?;
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
    
    Ok(result)
} 
