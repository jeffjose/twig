// twig/src/providers/mod.rs

pub mod battery;
pub mod builtin;
pub mod git;
pub mod ip;

use crate::config::Config;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug)]
#[allow(dead_code)]
pub enum ProviderError {
    CommandNotFound(String),
    /// Future: will be used for command execution failures
    #[allow(dead_code)]
    ExecutionFailed(String),
    /// Future: will be used for missing resources (e.g., battery not found)
    #[allow(dead_code)]
    ResourceNotAvailable(String),
    /// Future: will be used for parsing failures
    #[allow(dead_code)]
    ParseError(String),
}

pub type ProviderResult<T> = Result<T, ProviderError>;

/// Timing information for provider execution
#[derive(Debug, Clone)]
pub struct ProviderTiming {
    pub name: String,
    pub duration: Duration,
}

/// Result of collecting variables from all providers
pub struct CollectResult {
    pub variables: HashMap<String, String>,
    pub timings: Vec<ProviderTiming>,
}

/// Trait for data providers that contribute variables to prompts
pub trait Provider {
    /// Provider name - used for registration
    ///
    /// Example: "git", "builtin", "battery"
    fn name(&self) -> &str;

    /// Config sections this provider handles
    ///
    /// A single provider can handle multiple config sections.
    ///
    /// Examples:
    /// - GitProvider: vec!["git"]
    /// - BuiltinProvider: vec!["time", "hostname", "cwd"]
    /// - BatteryProvider: vec!["battery"]
    ///
    /// The registry uses this to route config sections to providers.
    fn sections(&self) -> Vec<&str>;

    /// Collect variables from this provider
    ///
    /// # Arguments
    /// * `config` - Full config object (provider reads its own sections)
    /// * `validate` - If true, return errors instead of empty strings
    ///
    /// # Returns
    /// HashMap of variable_name -> value pairs
    ///
    /// # Error Handling
    /// - If validate=false: Return empty vars on error (graceful degradation)
    /// - If validate=true: Return Err(ProviderError) for debugging
    ///
    /// # Examples
    /// ```
    /// // Git provider might return:
    /// {
    ///     "git": "main",           // Branch name
    ///     "git_dirty": "true",     // Has uncommitted changes
    ///     "git_ahead": "2",        // Commits ahead of remote
    ///     "git_behind": "0",       // Commits behind remote
    /// }
    /// ```
    fn collect(&self, config: &Config, validate: bool) -> ProviderResult<HashMap<String, String>>;

    /// Default config if section is missing but variables are used in template
    ///
    /// REQUIRED for implicit section support. Every provider must implement this.
    ///
    /// # Returns
    /// HashMap of section_name -> default config
    ///
    /// # Examples
    /// ```
    /// // GitProvider returns:
    /// {
    ///     "git": {} // No config needed, just enable it
    /// }
    ///
    /// // BuiltinProvider returns:
    /// {
    ///     "time": { "format": "%H:%M:%S" },
    ///     "hostname": {},
    ///     "cwd": {}
    /// }
    /// ```
    fn default_config(&self) -> HashMap<String, Value>;

    /// Whether this provider can be cached by the daemon
    ///
    /// Some providers change rarely (hostname) and can be cached.
    /// Others change frequently (git branch) and should be queried live.
    ///
    /// Default: false (query live)
    /// Future: will be used when daemon caching is implemented
    #[allow(dead_code)]
    fn cacheable(&self) -> bool {
        false
    }

    /// How long cached data is valid (in seconds)
    ///
    /// Only used if cacheable() returns true.
    ///
    /// Default: 5 seconds
    /// Future: will be used when daemon caching is implemented
    #[allow(dead_code)]
    fn cache_duration(&self) -> u64 {
        5
    }
}

/// Registry of all available providers
pub struct ProviderRegistry {
    // Provider name -> Provider
    providers: HashMap<String, Box<dyn Provider>>,
    // Section name -> Provider name (one section maps to one provider)
    section_map: HashMap<String, String>,
}

impl ProviderRegistry {
    /// Create new registry with built-in plugins registered
    pub fn new() -> Self {
        let mut registry = Self {
            providers: HashMap::new(),
            section_map: HashMap::new(),
        };

        // Register built-in plugins
        registry.register(Box::new(builtin::BuiltinProvider::new()));
        registry.register(Box::new(git::GitProvider::new()));
        registry.register(Box::new(ip::IpProvider::new()));
        registry.register(Box::new(battery::BatteryProvider::new()));

        registry
    }

    /// Register a new provider
    ///
    /// This updates both the provider map and section map.
    /// One provider can handle multiple sections.
    pub fn register(&mut self, provider: Box<dyn Provider>) {
        let name = provider.name().to_string();

        // Map each section to this provider
        for section in provider.sections() {
            self.section_map.insert(section.to_string(), name.clone());
        }

        self.providers.insert(name, provider);
    }

    /// Get provider by name
    pub fn get(&self, name: &str) -> Option<&dyn Provider> {
        self.providers.get(name).map(|b| b.as_ref())
    }

    /// Get provider that handles a specific section
    pub fn get_by_section(&self, section: &str) -> Option<&dyn Provider> {
        self.section_map.get(section)
            .and_then(|name| self.get(name))
    }

    /// List all registered provider names
    /// Future: will be used for diagnostic/debugging commands
    #[allow(dead_code)]
    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Collect variables from all providers
    ///
    /// # Arguments
    /// * `config` - The full config object
    /// * `validate` - If true, providers return errors instead of empty values
    ///
    /// # Returns
    /// Result with CollectResult containing variables and timing data, or first error encountered
    pub fn collect_all(&self, config: &Config, validate: bool) -> ProviderResult<CollectResult> {
        let mut variables = HashMap::new();
        let mut timings = Vec::new();

        for provider in self.providers.values() {
            let start = Instant::now();
            match provider.collect(config, validate) {
                Ok(vars) => {
                    let duration = start.elapsed();
                    timings.push(ProviderTiming {
                        name: provider.name().to_string(),
                        duration,
                    });
                    variables.extend(vars);
                }
                Err(e) if validate => return Err(e),
                Err(_) => {} // Silent failure in non-validate mode
            }
        }

        // Sort timings by provider name for consistent output
        timings.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(CollectResult { variables, timings })
    }

    /// Collect variables from specific providers only
    ///
    /// Used when template parsing discovers only certain variables are needed.
    /// More efficient than collecting from all providers.
    ///
    /// # Arguments
    /// * `provider_names` - List of provider names to query
    /// * `config` - The full config object
    /// * `validate` - If true, providers return errors
    ///
    /// # Returns
    /// Result with CollectResult containing variables and timing data from specified providers
    pub fn collect_from(
        &self,
        provider_names: &[&str],
        config: &Config,
        validate: bool,
    ) -> ProviderResult<CollectResult> {
        let mut variables = HashMap::new();
        let mut timings = Vec::new();

        for name in provider_names {
            if let Some(provider) = self.get(name) {
                let start = Instant::now();
                match provider.collect(config, validate) {
                    Ok(vars) => {
                        let duration = start.elapsed();
                        timings.push(ProviderTiming {
                            name: provider.name().to_string(),
                            duration,
                        });
                        variables.extend(vars);
                    }
                    Err(e) if validate => return Err(e),
                    Err(_) => {} // Silent failure in non-validate mode
                }
            }
        }

        // Sort timings by provider name for consistent output
        timings.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(CollectResult { variables, timings })
    }

    /// Determine which providers are needed based on variables in template
    ///
    /// Uses prefix convention: {git_dirty} -> "git" provider
    ///
    /// # Arguments
    /// * `variables` - List of variable names found in template
    ///
    /// # Returns
    /// List of provider names needed
    pub fn determine_providers(&self, variables: &[&str]) -> Vec<&str> {
        let mut needed = std::collections::HashSet::new();

        for var in variables {
            // Extract prefix (before first underscore, or whole name)
            let prefix = var.split('_').next().unwrap_or(var);

            // Check if any section matches this prefix
            if let Some(provider_name) = self.section_map.get(prefix) {
                needed.insert(provider_name.as_str());
            }
        }

        needed.into_iter().collect()
    }
}
