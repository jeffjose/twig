use crate::template::format_template;
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

// Common trait for all variable providers
pub trait VariableProvider {
    type Error: Error;
    type Config: ConfigWithName;

    fn get_value(config: &Self::Config) -> Result<String, Self::Error>;
    fn section_name() -> &'static str;
}

// Common trait for configs that have a name and error field
pub trait ConfigWithName {
    fn name(&self) -> Option<&str>;
    fn error(&self) -> &str;
}

// Common struct for variable processing results
pub struct ProcessingResult {
    pub variables: Vec<(String, String)>,
    pub duration: Duration,
}

// Add this new trait for lazy variable evaluation
pub trait LazyVariables {
    type Error;

    // Get a specific variable's value
    fn get_variable(name: &str) -> Result<String, Self::Error>;

    // List available variable names
    fn variable_names() -> &'static [&'static str];

    // Default implementation for getting only needed variables
    fn get_needed_variables(format: &str) -> Result<HashMap<String, String>, Self::Error> {
        let mut vars = HashMap::new();

        // Check each variable name
        for &var_name in Self::variable_names() {
            // Look for both colored and uncolored variants
            let plain_pattern = format!("{{{}}}", var_name);
            let colored_pattern = format!("{{{}:", var_name);

            if format.contains(&plain_pattern) || format.contains(&colored_pattern) {
                vars.insert(var_name.to_string(), Self::get_variable(var_name)?);
            }
        }

        Ok(vars)
    }
}

// Update helper function for variable replacement to handle colors
pub fn replace_variables(format: &str, vars: &HashMap<String, String>) -> String {
    let mut var_specs = Vec::new(); // Store the variable specifications and values
    let mut chars = format.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            let mut var_spec = String::new();
            let mut found_closing = false;

            while let Some(&next_char) = chars.peek() {
                if next_char == '}' {
                    found_closing = true;
                    chars.next();
                    break;
                }
                var_spec.push(chars.next().unwrap());
            }

            if found_closing && !var_spec.is_empty() {
                // Split variable name and color
                let var_name = if let Some(colon_pos) = var_spec.find(':') {
                    var_spec[..colon_pos].to_string()
                } else {
                    var_spec.clone()
                };

                // If we have a value for this variable, store both name and value
                if let Some(value) = vars.get(&var_name) {
                    var_specs.push((var_name, value.clone()));
                }
            }
        }
    }

    // Create a vector of references with the correct types for format_template
    let var_refs: Vec<(&str, &str)> = var_specs
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();

    // Use template formatter to handle both variable replacement and colors
    format_template(&format, &var_refs, false, None).unwrap_or_else(|_| format.to_string())
}

// Helper function to process a section's variables
pub async fn process_section<P: VariableProvider>(
    configs: &[P::Config],
    format: &str,
    validate: bool,
) -> ProcessingResult {
    use std::time::Instant;
    let start = Instant::now();
    let mut variables = Vec::new();

    // Skip entire section if format doesn't contain any variables
    if !format.contains('{') {
        return ProcessingResult {
            variables,
            duration: start.elapsed(),
        };
    }

    for (i, config) in configs.iter().enumerate() {
        let var_name = get_var_name(config, P::section_name(), i);
        debug_variable_usage(format, P::section_name(), &var_name, validate);

        // Check for both colored and uncolored variants
        let plain_pattern = format!("{{{}}}", var_name);
        let colored_pattern = format!("{{{}:", var_name);

        if format.contains(&plain_pattern) || format.contains(&colored_pattern) {
            let value = match P::get_value(config) {
                Ok(val) => val,
                Err(e) => {
                    if validate {
                        eprintln!("Warning: couldn't get {}: {}", P::section_name(), e);
                    }
                    config.error().to_string()
                }
            };
            variables.push((var_name, value));
        }
    }

    ProcessingResult {
        variables,
        duration: start.elapsed(),
    }
}

// Update format_uses_variable to check for both variants
pub fn format_uses_variable(format: &str, var_name: &str) -> bool {
    let plain_pattern = format!("{{{}}}", var_name);
    let colored_pattern = format!("{{{}:", var_name);
    format.contains(&plain_pattern) || format.contains(&colored_pattern)
}

pub fn debug_variable_usage(format: &str, section: &str, var_name: &str, validate: bool) {
    if validate {
        if format_uses_variable(format, var_name) {
            eprintln!(
                "Debug: Will process {} section for variable '{}'",
                section, var_name
            );
        } else {
            eprintln!(
                "Debug: Skipping {} section - variable '{}' not used",
                section, var_name
            );
        }
    }
}

pub fn get_var_name<T: ConfigWithName>(config: &T, section_name: &str, index: usize) -> String {
    config.name().map(String::from).unwrap_or_else(|| {
        if index == 0 {
            section_name.to_string()
        } else {
            format!("{}_{}", section_name, index + 1)
        }
    })
}
