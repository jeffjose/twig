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

fn apply_color(text: &str, color: &str) -> Result<String, TemplateError> {
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
            eprintln!("Warning: unknown color '{}', using white instead", unknown);
            text.white().to_string()
        }
    };
    Ok(result)
}

pub fn format_template(template: &str, variables: &[(&str, &str)]) -> Result<String, TemplateError> {
    let mut result = template.to_string();
    
    for (name, value) in variables {
        // Look for both colored and non-colored variables
        let var_patterns = [
            format!("{{{}}}", name),  // Simple {var}
            format!("{{{}:", name),   // Start of {var:color}
        ];

        for pattern in var_patterns {
            while let Some(start) = result.find(&pattern) {
                let after_var = start + pattern.len();
                
                // Check if this is a color variant
                if pattern.ends_with(':') {
                    // Find the end of the color specification
                    if let Some(end) = result[after_var..].find('}') {
                        let color = &result[after_var..after_var + end];
                        let colored_value = apply_color(value, color)?;
                        result.replace_range(start..after_var + end + 1, &colored_value);
                    } else {
                        return Err(TemplateError::InvalidSyntax("Unclosed color specification".into()));
                    }
                } else {
                    // Simple replacement
                    result.replace_range(start..after_var + 1, value);
                }
            }
        }
    }

    // Check for any remaining {...} patterns
    if let Some(start) = result.find('{') {
        if let Some(end) = result[start..].find('}') {
            let var_spec = &result[start + 1..start + end];
            if let Some(colon) = var_spec.find(':') {
                return Err(TemplateError::MissingVariable(var_spec[..colon].to_string()));
            }
            return Err(TemplateError::MissingVariable(var_spec.to_string()));
        }
    }
    
    Ok(result)
} 
