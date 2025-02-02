use crate::variable::{ConfigWithName, VariableProvider, LazyVariables};
use std::env;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum EnvError {
    NotFound(String),
}

impl fmt::Display for EnvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvError::NotFound(var) => write!(f, "Environment variable not found: {}", var),
        }
    }
}

impl Error for EnvError {}

#[derive(Default)]
pub struct Config {
    pub name: String,
    pub error: String,
}

impl ConfigWithName for Config {
    fn name(&self) -> Option<&str> {
        Some(&self.name)
    }
    fn error(&self) -> &str {
        &self.error
    }
}

pub struct EnvProvider;

impl LazyVariables for EnvProvider {
    type Error = EnvError;
    
    fn get_variable(name: &str) -> Result<String, Self::Error> {
        let var_name = if name.starts_with('$') {
            &name[1..]
        } else {
            name
        };
        env::var(var_name).map_err(|_| EnvError::NotFound(var_name.to_string()))
    }
    
    fn variable_names() -> &'static [&'static str] {
        &[] // Dynamic variables from environment
    }
}

impl VariableProvider for EnvProvider {
    type Error = EnvError;
    type Config = Config;

    fn get_value(config: &Self::Config) -> Result<String, Self::Error> {
        let var_name = config.name.strip_prefix('$').unwrap_or(&config.name);
        Self::get_variable(var_name)
    }

    fn section_name() -> &'static str {
        "env"
    }
} 
