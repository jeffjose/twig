// twig/src/providers/builtin.rs

use super::{Provider, ProviderResult};
use crate::config::Config;
use chrono::Local;
use gethostname::gethostname;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;

pub struct BuiltinProvider;

impl BuiltinProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Provider for BuiltinProvider {
    fn name(&self) -> &str {
        "builtin"
    }

    fn sections(&self) -> Vec<&str> {
        // One provider handles THREE sections
        vec!["time", "hostname", "cwd"]
    }

    fn collect(&self, config: &Config, _validate: bool) -> ProviderResult<HashMap<String, String>> {
        let mut vars = HashMap::new();

        // Handle [time] section
        if let Some(time_config) = &config.time {
            let time = Local::now()
                .format(&time_config.format)
                .to_string();
            let var_name = time_config.name.as_deref().unwrap_or("time");
            vars.insert(var_name.to_string(), time);
        }

        // Handle [hostname] section
        if let Some(hostname_config) = &config.hostname {
            let hostname = gethostname()
                .to_string_lossy()
                .to_string();
            // Use short hostname (before first dot) instead of FQDN
            let short_hostname = hostname.split('.').next().unwrap_or(&hostname).to_string();
            let var_name = hostname_config.name.as_deref().unwrap_or("hostname");
            vars.insert(var_name.to_string(), short_hostname);
        }

        // Handle [cwd] section
        if let Some(cwd_config) = &config.cwd {
            let cwd = env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "?".to_string());
            let var_name = cwd_config.name.as_deref().unwrap_or("cwd");
            vars.insert(var_name.to_string(), cwd);
        }

        Ok(vars)
    }

    fn default_config(&self) -> HashMap<String, Value> {
        let mut defaults = HashMap::new();
        defaults.insert("time".to_string(), json!({ "format": "%H:%M:%S" }));
        defaults.insert("hostname".to_string(), json!({}));
        defaults.insert("cwd".to_string(), json!({}));
        defaults
    }

    fn cacheable(&self) -> bool {
        // Time changes constantly, so don't cache
        false
    }
}
