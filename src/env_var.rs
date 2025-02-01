use crate::variable::{ConfigWithName, VariableProvider};
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

impl VariableProvider for EnvProvider {
    type Error = EnvError;
    type Config = Config;

    fn get_value(config: &Self::Config) -> Result<String, Self::Error> {
        let var_name = config.name.strip_prefix('$').unwrap_or(&config.name);
        env::var(var_name).map_err(|_| EnvError::NotFound(var_name.to_string()))
    }

    fn section_name() -> &'static str {
        "env"
    }
} 
