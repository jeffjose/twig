# Plugin Architecture - Implementation Guide

## Context

Currently, data providers (time, hostname, cwd) are hardcoded in main.rs. Before adding git support, we need a plugin architecture that scales.

**Current state:**
- Time, hostname, cwd are hardcoded in main.rs
- Each provider requires modifying main.rs
- Config sections are manually checked
- Variable collection is hardcoded
- Not extensible for future providers

**Goal:**
- Plugin-based architecture where one plugin can handle multiple sections
- Add providers without modifying main.rs
- Scalable for future additions (kubernetes, docker, AWS, etc.)
- Clear separation between core and plugins

**Implementation Focus:**
- **Primary target**: Git provider (fully detailed)
- **Future providers**: Battery, IP, Network (mentioned for architecture validation)

## Key Design Decisions

### 1. Multi-Section Plugins
**One plugin can register multiple config sections**

Example: `BuiltinProvider` handles `[time]`, `[hostname]`, and `[cwd]` sections together
```rust
impl Provider for BuiltinProvider {
    fn sections(&self) -> Vec<&str> {
        vec!["time", "hostname", "cwd"]
    }
}
```

### 2. Prefix Convention for Variable Discovery
**How do we know which provider handles `{git_dirty}`?**

Variables use a **prefix convention**:
- `{git}`, `{git_dirty}`, `{git_ahead}` → all handled by GitProvider
- `{battery}`, `{battery_percent}`, `{battery_status}` → all handled by BatteryProvider

The prefix (before underscore or standalone) maps to the provider name. Simple, scalable, self-documenting.

### 3. Error Handling: Silent Graceful Degradation + Validation Mode
**Normal mode**: Return empty strings on errors (graceful UX)
- Git not installed? `{git}` shows empty string, prompt still works
- Not in a git repo? `{git}` shows empty string, no errors

**Validation mode**: `twig --validate` shows detailed errors for debugging
```bash
$ twig --validate
✓ time provider: OK
✓ hostname provider: OK
✗ git provider: git command not found
✗ battery provider: /sys/class/power_supply/BAT0 not found
```

### 4. Implicit Sections with Defaults
**You can use `{git}` without a `[git]` section in config**

```toml
# This works immediately - no [git] section needed!
[prompt]
format = '{time:cyan} {git:yellow} {cwd:green}'
```

When twig sees `{git}` in template:
1. Checks if `[git]` section exists
2. If not, calls `GitProvider::default_config()` to get implicit defaults
3. Provider runs with defaults
4. User only adds `[git]` section when they want to customize

Every provider MUST implement `default_config()` to support this.

## Proposed Architecture

### Core vs Plugins

**Builtins (Refactored into BuiltinProvider):**
- **Environment variables**: `{$VAR}` - Special syntax, always available
- **Time**: `{time}` - Simple formatting
- **Hostname**: `{hostname}` - Cacheable by daemon
- **CWD**: `{cwd}` - Always available from OS

All three handled by single `BuiltinProvider` that manages multiple sections.

**Plugins (Extensible Architecture):**
- **Git**: Branch, status, dirty indicators, ahead/behind (PRIMARY FOCUS)
- Future: Battery, IP, Kubernetes context, Docker, AWS profile, etc.

### Provider Trait

```rust
// twig/src/providers/mod.rs

use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug)]
pub enum ProviderError {
    CommandNotFound(String),
    ExecutionFailed(String),
    ResourceNotAvailable(String),
    ParseError(String),
}

pub type ProviderResult<T> = Result<T, ProviderError>;

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
    fn cacheable(&self) -> bool {
        false
    }

    /// How long cached data is valid (in seconds)
    ///
    /// Only used if cacheable() returns true.
    ///
    /// Default: 5 seconds
    fn cache_duration(&self) -> u64 {
        5
    }
}
```

### Provider Registry

```rust
// twig/src/providers/mod.rs

use std::collections::HashMap;

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
        registry.register(Box::new(BuiltinProvider::new()));
        registry.register(Box::new(GitProvider::new()));

        // Future providers (stubs for now):
        // registry.register(Box::new(BatteryProvider::new()));

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
    /// Result with HashMap of all variables or first error encountered
    pub fn collect_all(&self, config: &Config, validate: bool) -> ProviderResult<HashMap<String, String>> {
        let mut variables = HashMap::new();

        for provider in self.providers.values() {
            match provider.collect(config, validate) {
                Ok(vars) => variables.extend(vars),
                Err(e) if validate => return Err(e),
                Err(_) => {} // Silent failure in non-validate mode
            }
        }

        Ok(variables)
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
    /// Result with HashMap of variables from specified providers
    pub fn collect_from(
        &self,
        provider_names: &[&str],
        config: &Config,
        validate: bool,
    ) -> ProviderResult<HashMap<String, String>> {
        let mut variables = HashMap::new();

        for name in provider_names {
            if let Some(provider) = self.get(name) {
                match provider.collect(config, validate) {
                    Ok(vars) => variables.extend(vars),
                    Err(e) if validate => return Err(e),
                    Err(_) => {}
                }
            }
        }

        Ok(variables)
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
```

### Git Provider (DETAILED IMPLEMENTATION)

```rust
// twig/src/providers/git.rs

use super::{Provider, ProviderError, ProviderResult};
use crate::config::Config;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Command;

pub struct GitProvider;

impl GitProvider {
    pub fn new() -> Self {
        Self
    }

    /// Check if git command is available
    fn git_available(&self) -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Check if current directory is in a git repo
    fn is_git_repo(&self) -> bool {
        Command::new("git")
            .args(&["rev-parse", "--git-dir"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Get current branch name
    fn get_branch(&self) -> Option<String> {
        let output = Command::new("git")
            .args(&["branch", "--show-current"])
            .output()
            .ok()?;

        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if branch.is_empty() {
                None // Detached HEAD state
            } else {
                Some(branch)
            }
        } else {
            None
        }
    }

    /// Check if working directory is dirty (has uncommitted changes)
    fn is_dirty(&self) -> bool {
        Command::new("git")
            .args(&["status", "--porcelain"])
            .output()
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false)
    }

    /// Get commits ahead/behind remote
    fn get_ahead_behind(&self) -> Option<(u32, u32)> {
        let output = Command::new("git")
            .args(&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
            .output()
            .ok()?;

        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            let parts: Vec<&str> = text.trim().split_whitespace().collect();
            if parts.len() == 2 {
                let ahead = parts[0].parse().ok()?;
                let behind = parts[1].parse().ok()?;
                return Some((ahead, behind));
            }
        }
        None
    }
}

impl Provider for GitProvider {
    fn name(&self) -> &str {
        "git"
    }

    fn sections(&self) -> Vec<&str> {
        vec!["git"]
    }

    fn collect(&self, config: &Config, validate: bool) -> ProviderResult<HashMap<String, String>> {
        let mut vars = HashMap::new();

        // Check if git is available
        if !self.git_available() {
            return if validate {
                Err(ProviderError::CommandNotFound(
                    "git command not found".to_string()
                ))
            } else {
                Ok(vars) // Silent failure - return empty vars
            };
        }

        // If not in a git repo, return empty (not an error)
        if !self.is_git_repo() {
            return Ok(vars);
        }

        // Get branch name
        if let Some(branch) = self.get_branch() {
            // Primary variable: {git} = branch name
            vars.insert("git".to_string(), branch);
        } else {
            // Detached HEAD or error
            vars.insert("git".to_string(), "HEAD".to_string());
        }

        // Check config for advanced features (future depth)
        if let Some(git_config) = config.git.as_ref() {
            // Future: Support for showing dirty status
            // if git_config.show_dirty == Some(true) {
            //     let dirty = self.is_dirty();
            //     vars.insert("git_dirty".to_string(), dirty.to_string());
            // }

            // Future: Support for ahead/behind counts
            // if git_config.show_ahead_behind == Some(true) {
            //     if let Some((ahead, behind)) = self.get_ahead_behind() {
            //         vars.insert("git_ahead".to_string(), ahead.to_string());
            //         vars.insert("git_behind".to_string(), behind.to_string());
            //     }
            // }
        }

        Ok(vars)
    }

    fn default_config(&self) -> HashMap<String, Value> {
        let mut defaults = HashMap::new();
        // Git section enabled with no special config
        defaults.insert("git".to_string(), json!({}));
        defaults
    }

    fn cacheable(&self) -> bool {
        // Git status changes frequently, don't cache
        false
    }
}
```

**Future enhancements** (for later depth additions):
```toml
[git]
show_dirty = true       # Add {git_dirty} variable
show_ahead_behind = true  # Add {git_ahead} and {git_behind} variables
```

Then template could be:
```toml
[prompt]
format = '{git:yellow}{git_dirty:red}+{git_ahead:green}'
# Example output: "main*+2" (main branch, dirty, 2 commits ahead)
```

### Builtin Provider (Multi-Section Example)

This shows how **one provider handles multiple config sections**:

```rust
// twig/src/providers/builtin.rs

use super::{Provider, ProviderError, ProviderResult};
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
            vars.insert("time".to_string(), time);
        }

        // Handle [hostname] section
        if config.hostname.is_some() {
            let hostname = gethostname()
                .to_string_lossy()
                .to_string();
            vars.insert("hostname".to_string(), hostname);
        }

        // Handle [cwd] section
        if let Some(cwd_config) = &config.cwd {
            let cwd = env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "?".to_string());

            // Apply custom name if configured
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
```

### Future Providers (Brief Examples)

These demonstrate the architecture scales:

**Battery Provider** (future):
```rust
impl Provider for BatteryProvider {
    fn sections(&self) -> Vec<&str> { vec!["battery"] }
    // Provides: {battery}, {battery_percent}, {battery_status}
    // Cacheable: true (battery changes slowly)
}
```

**Network Provider** (future):
```rust
impl Provider for NetworkProvider {
    fn sections(&self) -> Vec<&str> { vec!["ip", "wifi"] }
    // One provider handles both IP and WiFi
    // Provides: {ip}, {ip_v6}, {wifi_ssid}, {wifi_strength}
}
```

### Config Structure

```toml
# User's config.toml

# Builtins (handled by BuiltinProvider)
[time]
format = "%H:%M:%S"  # Can be implicit - this is the default

[hostname]
# Completely implicit - no config needed

[cwd]
# Implicit, but can customize:
# name = "dir"  # Use {dir} instead of {cwd}

# Git (handled by GitProvider) - MAIN FOCUS
[git]
# Completely implicit! No config needed to use {git}
# Just add {git:yellow} to your prompt and it works

# Future depth additions:
# show_dirty = true         # Enable {git_dirty}
# show_ahead_behind = true  # Enable {git_ahead} and {git_behind}

[prompt]
# Basic usage (all implicit):
format = '{time:cyan} {git:yellow} {hostname:magenta} {cwd:green} '

# Advanced git usage (future):
# format = '{git:yellow}{git_dirty:red} +{git_ahead:green} {cwd} '
```

### File Structure

```
twig/
├── src/
│   ├── main.rs                 # Core logic, uses ProviderRegistry
│   ├── config.rs               # Config handling (TOML parsing) - NEEDS UPDATES
│   ├── shell/                  # Shell formatters (existing)
│   │   ├── mod.rs
│   │   ├── raw.rs
│   │   ├── bash.rs
│   │   ├── zsh.rs
│   │   └── tcsh.rs
│   └── providers/              # Provider plugins (NEW DIRECTORY)
│       ├── mod.rs              # Provider trait + ProviderRegistry + ProviderError
│       ├── builtin.rs          # BuiltinProvider (time, hostname, cwd)
│       └── git.rs              # GitProvider (PRIMARY FOCUS)
│       # Future:
│       # ├── battery.rs
│       # └── network.rs
```

### Main.rs Refactoring

**Current (hardcoded):**
```rust
fn main() {
    let config = load_config();
    let mut variables = HashMap::new();

    // HARDCODED: Time
    if let Some(time_config) = &config.time {
        variables.insert("time", format_time(&time_config.format));
    }
    // HARDCODED: Hostname
    // HARDCODED: CWD
    // etc...

    let output = substitute_variables(&config.prompt.format, &variables);
    println!("{}", output);
}
```

**After refactoring:**
```rust
mod providers;

use providers::ProviderRegistry;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let mut config = load_config();

    // Apply implicit sections for variables used in template
    apply_implicit_sections(&mut config, &config.prompt.format);

    // Create provider registry (registers all providers)
    let registry = ProviderRegistry::new();

    // Collect variables from all providers
    let validate = args.validate;
    let variables = match registry.collect_all(&config, validate) {
        Ok(vars) => vars,
        Err(e) if validate => {
            eprintln!("Provider error: {:?}", e);
            std::process::exit(1);
        }
        Err(_) => HashMap::new(), // Should not happen - providers catch errors in non-validate mode
    };

    if validate {
        println!("✓ All providers validated successfully");
        return Ok(());
    }

    // Substitute and format
    let output = substitute_variables(&config.prompt.format, &variables);
    let formatted = format_for_shell(&output, args.mode);
    println!("{}", formatted);

    Ok(())
}

/// Apply default configs for variables used in template but missing config sections
fn apply_implicit_sections(config: &mut Config, template: &str) {
    let registry = ProviderRegistry::new();
    let vars = discover_variables(template);

    for var in vars {
        let prefix = var.split('_').next().unwrap_or(&var);

        if let Some(provider) = registry.get_by_section(prefix) {
            let defaults = provider.default_config();

            for (section_name, default_value) in defaults {
                if !config.has_section(&section_name) {
                    config.add_implicit_section(section_name, default_value);
                }
            }
        }
    }
}
```

## Implementation Plan

### Phase 1: Provider Infrastructure (Foundation)

**Goal**: Create the provider system without breaking anything

**1. Create directory and module structure**
```bash
mkdir src/providers
touch src/providers/mod.rs
touch src/providers/builtin.rs
touch src/providers/git.rs
```

**2. Implement `src/providers/mod.rs`**:
- Copy `Provider` trait from this doc
- Copy `ProviderError` and `ProviderResult` types
- Copy `ProviderRegistry` struct
- Add: `pub mod builtin;`
- Add: `pub mod git;`

**3. Implement stub providers**:
- `builtin.rs`: Copy BuiltinProvider from this doc (full implementation)
- `git.rs`: Copy GitProvider from this doc (full implementation)

**4. Update `src/main.rs`**:
- Add: `mod providers;` at top
- Don't integrate yet - just ensure it compiles

**5. Test**:
```bash
cargo build
```
Should compile with no errors. No functionality changes.

---

### Phase 2: Config Updates

**Goal**: Add support for Git config section and implicit sections

**1. Update `src/config.rs`**:

Add Git config struct:
```rust
#[derive(Debug, Deserialize)]
pub struct GitConfig {
    pub name: Option<String>,  // Custom variable name
    // Future: show_dirty, show_ahead_behind
}
```

Add to Config struct:
```rust
pub struct Config {
    pub git: Option<GitConfig>,
    // ... existing fields ...
}
```

Add helper methods:
```rust
impl Config {
    pub fn has_section(&self, section: &str) -> bool {
        match section {
            "time" => self.time.is_some(),
            "hostname" => self.hostname.is_some(),
            "cwd" => self.cwd.is_some(),
            "git" => self.git.is_some(),
            _ => false,
        }
    }

    pub fn add_implicit_section(&mut self, section: String, _value: Value) {
        match section.as_str() {
            "time" => self.time = Some(TimeConfig { format: "%H:%M:%S".to_string() }),
            "hostname" => self.hostname = Some(HostnameConfig {}),
            "cwd" => self.cwd = Some(CwdConfig { name: None }),
            "git" => self.git = Some(GitConfig { name: None }),
            _ => {}
        }
    }
}
```

**2. Test**:
```bash
cargo build
```

---

### Phase 3: Integrate Providers into Main

**Goal**: Replace hardcoded logic with provider system

**1. Add CLI argument for --validate**:
```rust
// In main.rs or cli parsing
#[derive(Parser)]
struct Args {
    #[arg(long)]
    validate: bool,
    // ... existing args ...
}
```

**2. Replace hardcoded variable collection in main.rs**:

Remove old hardcoded blocks for time/hostname/cwd.

Add:
```rust
mod providers;
use providers::ProviderRegistry;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let mut config = load_config()?;

    // Apply implicit sections
    apply_implicit_sections(&mut config, &config.prompt.format);

    // Collect from providers
    let registry = ProviderRegistry::new();
    let variables = registry.collect_all(&config, args.validate)
        .unwrap_or_else(|e| {
            if args.validate {
                eprintln!("Error: {:?}", e);
                std::process::exit(1);
            }
            HashMap::new()
        });

    if args.validate {
        println!("✓ All providers OK");
        return Ok(());
    }

    // Continue with existing logic
    let output = substitute_variables(&config.prompt.format, &variables);
    // ... format and print ...

    Ok(())
}

// Add helper function (from this doc)
fn apply_implicit_sections(config: &mut Config, template: &str) {
    // Implementation from this doc
}

fn discover_variables(template: &str) -> Vec<String> {
    // Parse template and extract {variable} names
    // Reuse existing parser logic
}
```

**3. Test**:
```bash
# Build
cargo build

# Test existing functionality still works
./twig --mode bash

# Test in git repo
cd /path/to/git/repo
./twig --mode bash  # Should show time/hostname/cwd (no change yet)

# Test validation
./twig --validate
```

---

### Phase 4: Enable Git in Prompt

**Goal**: Users can now use `{git}` in their prompts!

**1. Update your config.toml**:
```toml
[prompt]
format = '{time:cyan} {git:yellow} {hostname:magenta} {cwd:green} '
```

**2. Test**:
```bash
cd /path/to/git/repo
./twig --mode bash

# Should see branch name in prompt!
```

**3. Test edge cases**:
```bash
# Not in git repo
cd ~
./twig --mode bash
# Should show empty string for git (graceful)

# Validate mode
./twig --validate
# Should show ✓ for all providers

# Git not installed (simulate)
alias git="false"
./twig --mode bash
# Should work, git just empty

./twig --validate
# Should show error about git
```

---

### Phase 5: Testing & Refinement

**Test matrix**:
- ✓ In git repo: shows branch
- ✓ Not in git repo: shows empty
- ✓ Git not installed: shows empty (validates shows error)
- ✓ Detached HEAD: shows "HEAD"
- ✓ Existing time/hostname/cwd still work
- ✓ --validate flag works

**Integration tests**:
Create `tests/integration_test.rs`:
```rust
#[test]
fn test_git_provider_in_repo() {
    // Test git provider returns branch name
}

#[test]
fn test_git_provider_outside_repo() {
    // Test git provider returns empty
}
```

---

### Phase 6: Documentation

**Update README.md** with:
```markdown
## Git Support

Twig now supports showing the current git branch in your prompt!

### Usage

Add `{git}` to your prompt:

```toml
[prompt]
format = '{time} {git:yellow} {cwd} '
```

No additional configuration needed - it just works!

### Troubleshooting

Run validation mode to check if git is working:
```bash
twig --validate
```
```

---

### Optional: Future Enhancements (Later)

These are NOT part of initial implementation:

- **Git dirty status**: Add `show_dirty` config option
- **Ahead/behind**: Add `show_ahead_behind` config option
- **Battery provider**: New provider for battery status
- **Performance optimization**: Only query needed providers (skip providers not in template)

## Design Decisions Summary

All key decisions have been made (see "Key Design Decisions" section above):

✓ **Multi-section plugins**: One provider can handle multiple sections (e.g., BuiltinProvider handles time/hostname/cwd)
✓ **Prefix convention**: Variables use prefix to map to providers (`git_*` → GitProvider)
✓ **Error handling**: Silent graceful degradation in normal mode, errors in `--validate` mode
✓ **Implicit sections**: Variables work without config sections using `default_config()`
✓ **Multi-value providers**: Providers return `HashMap<String, String>` with multiple variables
✓ **Variable naming**: Flat namespace (`git_dirty` not `git.dirty`)
✓ **Builtins refactoring**: YES - create BuiltinProvider for consistency
✓ **Daemon integration**: Use `cacheable()` method in trait (future optimization)

## Next Steps

**Start with Phase 1** of the implementation plan above:

1. Create `src/providers/` directory
2. Implement Provider trait and registry
3. Implement BuiltinProvider
4. Implement GitProvider
5. Update Config for git section
6. Integrate into main.rs
7. Test thoroughly
8. Document

Follow the detailed phase-by-phase plan in the "Implementation Plan" section.

## Why This Architecture?

**Benefits**:
- **Scalability**: Add new providers without modifying core code
- **Testability**: Each provider is isolated and independently testable
- **Maintainability**: Clear separation of concerns, single responsibility
- **User-friendly**: Implicit sections mean it "just works" out of the box
- **Performance**: Can optimize to only query needed providers (future)
- **Consistency**: All providers follow the same pattern

**Risks mitigated**:
- **Over-engineering**: Kept trait simple, only features we need now
- **Breaking changes**: Incremental migration, everything stays working
- **User complexity**: Implicit sections + sensible defaults = zero config needed

## Example Usage

**Basic (works immediately, no config needed)**:
```toml
[prompt]
format = '{time:cyan} {git:yellow} {cwd:green} '
```

**In git repo**: Shows `14:30:15 main ~/code/twig`
**Not in git repo**: Shows `14:30:15 ~/home`

**With validation**:
```bash
$ twig --validate
✓ builtin provider: OK (time, hostname, cwd)
✓ git provider: OK
```

**Future advanced usage** (after adding depth):
```toml
[git]
show_dirty = true
show_ahead_behind = true

[prompt]
format = '{git:yellow}{git_dirty:red}+{git_ahead:green} {cwd} '
```
Output: `main*+2 ~/code/twig` (main branch, dirty, 2 commits ahead)
