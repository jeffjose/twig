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

impl std::error::Error for TemplateError {}

pub fn format_template(template: &str, variables: &[(&str, &str)]) -> Result<String, TemplateError> {
    let mut result = template.to_string();
    
    for (name, value) in variables {
        let var_pattern = format!("{{{}}}", name);
        if template.contains(&var_pattern) {
            result = result.replace(&var_pattern, value);
        }
    }
    
    // Check if there are any remaining {...} patterns
    if let Some(start) = result.find('{') {
        if let Some(end) = result[start..].find('}') {
            let var_name = &result[start + 1..start + end];
            return Err(TemplateError::MissingVariable(var_name.to_string()));
        }
    }
    
    Ok(result)
} 
