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

// Helper function to process a section's variables
pub async fn process_section<P: VariableProvider>(
    configs: &[P::Config],
    format: &str,
    validate: bool,
) -> ProcessingResult {
    use std::time::Instant;
    let start = Instant::now();
    let mut variables = Vec::new();

    for (i, config) in configs.iter().enumerate() {
        let var_name = get_var_name(config, P::section_name(), i);
        debug_variable_usage(format, P::section_name(), &var_name, validate);

        if format_uses_variable(format, &var_name) {
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

pub fn format_uses_variable(format: &str, var_name: &str) -> bool {
    format.contains(&format!("{{{}}}", var_name)) || format.contains(&format!("{{{}:", var_name))
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
