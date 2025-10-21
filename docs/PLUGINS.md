# Plugin Architecture Design

## Context

Currently, data providers (time, hostname, cwd) are hardcoded in main.rs. As we add more providers (git, ip, battery, etc.), this approach doesn't scale. Before adding git support, we should design a proper plugin architecture.

**Current state:**
- Time, hostname, cwd are hardcoded in main.rs
- Each provider requires modifying main.rs
- Config sections are manually checked
- Variable collection is hardcoded
- Not extensible for future providers

**Goal:**
- Plugin-based architecture
- Add providers without modifying main.rs
- Scalable for future additions (kubernetes, docker, AWS, etc.)
- Clear separation between core and plugins

## The Problem

Looking at the next breadth-first step (Git Provider), we noticed:

1. Adding git would require hardcoding more logic in main.rs
2. Every new provider (ip, battery, etc.) would repeat this pattern
3. The old prompt (reference/.prompt) has extensive git support - we need that flexibility
4. No clear boundary between core functionality and data providers

## Proposed Architecture

### Core vs Plugins

**Hardcoded Builtins (Keep Simple):**
- **Environment variables**: `{$VAR}` - Special syntax, always available
- **Time**: `{time}` - Simple, formatting only, no external state
- **Hostname**: `{hostname}` - Simple, cacheable by daemon
- **CWD**: `{cwd}` - Simple, always available from OS

**Plugins (Extensible Architecture):**
- **Git**: Branch, status, dirty indicators, ahead/behind
- **IP**: Network interfaces, IPv4/IPv6
- **Battery**: Percentage, status, time remaining
- **Future possibilities**:
  - Kubernetes context
  - Docker container info
  - AWS profile
  - Terraform workspace
  - Python virtualenv
  - Node.js version
  - Custom user plugins

### Provider Trait

```rust
// twig/src/providers/mod.rs

use serde_json::Value;
use std::collections::HashMap;

/// Trait for data providers that contribute variables to prompts
pub trait Provider {
    /// Provider name - used for registration and config section matching
    ///
    /// Example: "git", "ip", "battery"
    fn name(&self) -> &str;

    /// Config section name (usually same as name, but can override)
    ///
    /// Example: For "git" provider, config section is [git]
    fn config_section(&self) -> &str {
        self.name()
    }

    /// Collect variables from this provider
    ///
    /// # Arguments
    /// * `config` - Optional config section for this provider (parsed from TOML)
    ///
    /// # Returns
    /// HashMap of variable_name -> value pairs
    ///
    /// # Examples
    /// ```
    /// // Git provider might return:
    /// {
    ///     "git": "main",                    // Branch name
    ///     "git_dirty": "true",              // Has uncommitted changes
    ///     "git_ahead": "2",                 // Commits ahead of remote
    ///     "git_behind": "0",                // Commits behind remote
    /// }
    ///
    /// // Battery provider might return:
    /// {
    ///     "battery": "85%",                 // Formatted percentage
    ///     "battery_percent": "85",          // Raw number
    ///     "battery_status": "Discharging",  // Status string
    /// }
    /// ```
    fn collect(&self, config: Option<&Value>) -> HashMap<String, String>;

    /// Default config if section is missing but provider is used in template
    ///
    /// This enables implicit section creation like we do for time/hostname/cwd
    ///
    /// # Returns
    /// Optional default config as JSON Value
    fn default_config(&self) -> Option<Value> {
        None
    }

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
    providers: HashMap<String, Box<dyn Provider>>,
}

impl ProviderRegistry {
    /// Create new registry with built-in plugins registered
    pub fn new() -> Self {
        let mut registry = Self {
            providers: HashMap::new(),
        };

        // Register built-in plugins
        registry.register(Box::new(GitProvider::new()));
        registry.register(Box::new(IpProvider::new()));
        registry.register(Box::new(BatteryProvider::new()));

        registry
    }

    /// Register a new provider
    pub fn register(&mut self, provider: Box<dyn Provider>) {
        let name = provider.name().to_string();
        self.providers.insert(name, provider);
    }

    /// Get provider by name
    pub fn get(&self, name: &str) -> Option<&dyn Provider> {
        self.providers.get(name).map(|b| b.as_ref())
    }

    /// List all registered provider names
    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Collect variables from all providers
    ///
    /// Iterates through all registered providers and collects their variables
    /// based on the config sections available.
    ///
    /// # Arguments
    /// * `config` - The full config object
    ///
    /// # Returns
    /// HashMap of all variables from all providers
    pub fn collect_all(&self, config: &Config) -> HashMap<String, String> {
        let mut variables = HashMap::new();

        for provider in self.providers.values() {
            // Get config section for this provider (if it exists)
            let section = config.get_section(provider.config_section());

            // Collect variables from provider
            let provider_vars = provider.collect(section);

            // Merge into overall variables
            variables.extend(provider_vars);
        }

        variables
    }

    /// Collect variables from specific providers only
    ///
    /// Used when template parsing discovers only certain variables are needed.
    /// More efficient than collecting from all providers.
    ///
    /// # Arguments
    /// * `provider_names` - List of provider names to query
    /// * `config` - The full config object
    ///
    /// # Returns
    /// HashMap of variables from specified providers only
    pub fn collect_from(&self, provider_names: &[&str], config: &Config) -> HashMap<String, String> {
        let mut variables = HashMap::new();

        for name in provider_names {
            if let Some(provider) = self.get(name) {
                let section = config.get_section(provider.config_section());
                let provider_vars = provider.collect(section);
                variables.extend(provider_vars);
            }
        }

        variables
    }
}
```

### Example Provider: Git

```rust
// twig/src/providers/git.rs

use super::Provider;
use serde_json::Value;
use std::collections::HashMap;
use std::process::Command;

pub struct GitProvider;

impl GitProvider {
    pub fn new() -> Self {
        Self
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
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
}

impl Provider for GitProvider {
    fn name(&self) -> &str {
        "git"
    }

    fn collect(&self, config: Option<&Value>) -> HashMap<String, String> {
        let mut vars = HashMap::new();

        // If not in a git repo, return empty
        if !self.is_git_repo() {
            return vars;
        }

        // Get branch name
        if let Some(branch) = self.get_branch() {
            // Primary variable: {git} = branch name
            vars.insert("git".to_string(), branch);
        }

        // Get dirty status (optional, for depth later)
        // if config.get("show_dirty").is_some() {
        //     let dirty = self.is_dirty();
        //     vars.insert("git_dirty".to_string(), dirty.to_string());
        // }

        vars
    }

    fn default_config(&self) -> Option<Value> {
        // No special config needed for basic git
        None
    }

    fn cacheable(&self) -> bool {
        // Git status changes frequently, don't cache
        false
    }
}
```

### Example Provider: Battery

```rust
// twig/src/providers/battery.rs

use super::Provider;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;

pub struct BatteryProvider;

impl BatteryProvider {
    pub fn new() -> Self {
        Self
    }

    /// Get battery percentage from /sys/class/power_supply/BAT0/capacity (Linux)
    fn get_percentage(&self) -> Option<u8> {
        let capacity = fs::read_to_string("/sys/class/power_supply/BAT0/capacity").ok()?;
        capacity.trim().parse().ok()
    }

    /// Get battery status from /sys/class/power_supply/BAT0/status (Linux)
    fn get_status(&self) -> Option<String> {
        let status = fs::read_to_string("/sys/class/power_supply/BAT0/status").ok()?;
        Some(status.trim().to_string())
    }
}

impl Provider for BatteryProvider {
    fn name(&self) -> &str {
        "battery"
    }

    fn collect(&self, config: Option<&Value>) -> HashMap<String, String> {
        let mut vars = HashMap::new();

        // Try to get battery info (gracefully handle systems without battery)
        if let Some(percent) = self.get_percentage() {
            // Primary variable: {battery} = formatted percentage
            vars.insert("battery".to_string(), format!("{}%", percent));

            // Additional variable: {battery_percent} = raw number
            vars.insert("battery_percent".to_string(), percent.to_string());
        }

        if let Some(status) = self.get_status() {
            vars.insert("battery_status".to_string(), status);
        }

        vars
    }

    fn default_config(&self) -> Option<Value> {
        None
    }

    fn cacheable(&self) -> bool {
        // Battery changes slowly, can cache
        true
    }

    fn cache_duration(&self) -> u64 {
        10 // 10 seconds
    }
}
```

### Config Structure

```toml
# User's config.toml

# Builtins (still hardcoded in main.rs)
[time]
format = "%H:%M:%S"

[hostname]
# Implicit, no config needed

[cwd]
# Implicit, no config needed

# Plugin-based providers
[git]
# Plugin automatically discovers and provides variables:
# - {git} = branch name (or empty if not in repo)
# Future depth additions:
# - {git_dirty} = true/false
# - {git_ahead} = number
# - {git_behind} = number

[ip]
interface = "eth0"  # Optional: specify network interface
# Provides: {ip}

[battery]
# Provides: {battery}, {battery_percent}, {battery_status}

[prompt]
format = '{time:cyan} {git:yellow} {hostname:magenta} {cwd:green} {battery:red} {"$":bold}'
```

### File Structure

```
twig/
├── src/
│   ├── main.rs                 # Core logic, uses ProviderRegistry
│   ├── config.rs               # Config handling (TOML parsing)
│   ├── shell/                  # Shell formatters (existing)
│   │   ├── mod.rs
│   │   ├── raw.rs
│   │   ├── bash.rs
│   │   ├── zsh.rs
│   │   └── tcsh.rs
│   └── providers/              # Provider plugins (NEW)
│       ├── mod.rs              # Provider trait + ProviderRegistry
│       ├── builtin.rs          # BuiltinProvider (time, hostname, cwd) - OPTIONAL
│       ├── git.rs              # GitProvider
│       ├── ip.rs               # IpProvider
│       └── battery.rs          # BatteryProvider
```

### Main.rs Changes

**Before (current):**
```rust
fn main() {
    // ... config loading ...

    let mut variables = HashMap::new();

    // HARDCODED: Time
    if let Some(time_config) = &config.time {
        let time = Local::now().format(&time_config.format).to_string();
        variables.insert("time", time);
    }

    // HARDCODED: Hostname
    if let Some(hostname_config) = &config.hostname {
        let hostname = gethostname().to_string_lossy().to_string();
        variables.insert("hostname", hostname);
    }

    // HARDCODED: CWD
    if let Some(cwd_config) = &config.cwd {
        let cwd = std::env::current_dir()?.to_string_lossy().to_string();
        variables.insert("cwd", cwd);
    }

    // ... substitute_variables ...
}
```

**After (plugin-based):**
```rust
fn main() {
    // ... config loading ...

    // Create provider registry
    let registry = ProviderRegistry::new();

    // Collect variables from builtins (still hardcoded for simplicity)
    let mut variables = collect_builtins(&config);

    // Collect variables from all plugins
    let plugin_vars = registry.collect_all(&config);
    variables.extend(plugin_vars);

    // ... substitute_variables ...
}
```

Or, even better with implicit sections:

```rust
fn main() {
    // ... config loading ...

    // Create provider registry
    let registry = ProviderRegistry::new();

    // Discover which variables are used in template
    let needed_vars = discover_variables(&config.prompt.format);

    // Collect builtins
    let mut variables = collect_builtins(&config, &needed_vars);

    // Determine which providers are needed
    let needed_providers = determine_providers(&needed_vars, &registry);

    // Collect only from needed providers (optimization)
    let plugin_vars = registry.collect_from(&needed_providers, &config);
    variables.extend(plugin_vars);

    // ... substitute_variables ...
}
```

## Migration Strategy

### Phase 1: Create Provider Infrastructure

**Goal**: Set up the architecture without breaking existing functionality

**Steps**:
1. Create `twig/src/providers/` directory
2. Create `providers/mod.rs` with Provider trait and ProviderRegistry
3. Keep existing hardcoded logic in main.rs
4. Test that everything still works

**Files to create**:
- `twig/src/providers/mod.rs` (trait + registry)
- `twig/src/providers/git.rs` (stub, returns empty vars)
- `twig/src/providers/ip.rs` (stub)
- `twig/src/providers/battery.rs` (stub)

**Changes to main.rs**:
- Add `mod providers;`
- Create registry in main() (but don't use it yet)

**Test**:
- `cargo build` succeeds
- All existing functionality works unchanged
- No user-visible changes

### Phase 2: Refactor Builtins (Optional)

**Goal**: Extract time/hostname/cwd into a builtin provider for consistency

**Options**:
A. Keep builtins hardcoded in main.rs (simpler)
B. Create BuiltinProvider that handles all three (more consistent)

**Decision needed**: Is consistency worth the complexity?

**If we do refactor builtins**:
```rust
// providers/builtin.rs
pub struct BuiltinProvider;

impl Provider for BuiltinProvider {
    fn name(&self) -> &str {
        "builtin"
    }

    fn collect(&self, config: Option<&Value>) -> HashMap<String, String> {
        let mut vars = HashMap::new();

        // Time
        if let Some(time_config) = config.get("time") {
            let format = time_config["format"].as_str().unwrap_or("%H:%M:%S");
            vars.insert("time".to_string(), Local::now().format(format).to_string());
        }

        // Hostname
        if config.get("hostname").is_some() {
            vars.insert("hostname".to_string(), gethostname().to_string_lossy().to_string());
        }

        // CWD
        if config.get("cwd").is_some() {
            let cwd = std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("?"))
                .to_string_lossy()
                .to_string();
            vars.insert("cwd".to_string(), cwd);
        }

        vars
    }
}
```

### Phase 3: Implement Git Provider

**Goal**: Add git as the first real plugin

**Steps**:
1. Implement GitProvider::collect() fully
2. Register GitProvider in ProviderRegistry::new()
3. Add [git] section support to Config
4. Test with template: `{git:yellow}`

**Testing**:
```toml
[prompt]
format = '{time:cyan} {git:yellow} {hostname:magenta}'
```

In git repo:
- Should show branch name

Not in git repo:
- Should show nothing (empty string)

### Phase 4: Add IP and Battery

**Goal**: Prove the architecture scales

**Steps**:
1. Implement IpProvider
2. Implement BatteryProvider
3. Test with all providers active

### Phase 5: Optimization

**Goal**: Only collect from providers that are actually used

**Steps**:
1. Parse template to discover which variables are used
2. Map variables to providers
3. Only call collect() on needed providers

This is important for performance - don't query git if template doesn't use `{git}`.

## Open Questions

### 1. Should we refactor builtins into providers?

**Option A: Keep builtins hardcoded in main.rs**
- Pros: Simpler, less abstraction
- Cons: Inconsistent with plugins

**Option B: Create BuiltinProvider**
- Pros: Consistent architecture, everything is a provider
- Cons: More complex, another abstraction layer

**Recommendation**: Start with Option A (keep simple), refactor later if needed.

### 2. Multi-value providers

Git might want to return multiple variables:
- `{git}` - branch name
- `{git_dirty}` - true/false
- `{git_ahead}` - number of commits ahead
- `{git_behind}` - number of commits behind

**Options**:
A. Provider returns multiple variables (HashMap)
B. Provider returns single value, variables are split by config

**Recommendation**: Option A (HashMap). More flexible, provider decides what variables it provides.

### 3. Variable naming conventions

**Option A: Flat namespace**
```
{git}         = "main"
{git_dirty}   = "true"
{git_ahead}   = "2"
```

**Option B: Nested (would require template changes)**
```
{git.branch}  = "main"
{git.dirty}   = "true"
{git.ahead}   = "2"
```

**Recommendation**: Option A (flat namespace). Simpler template syntax, matches current design.

### 4. Primary variable name

When git provider is used, what does `{git}` return?

**Options**:
A. Just branch name: `{git}` = `"main"`
B. Both `{git}` and `{git_branch}` = `"main"`
C. Formatted string: `{git}` = `"main +2"` (branch with ahead count)

**Recommendation**: Option A (just branch name). Clean, intuitive. Users can customize in template.

### 5. Config section vs variable name

**Current behavior** (for builtins):
```toml
[cwd]
name = "dir"  # Use {dir} instead of {cwd}
```

**Should plugins support this?**
```toml
[git]
name = "branch"  # Use {branch} instead of {git}
```

**Recommendation**: Yes, support `name` field for consistency. Provider.collect() should check config for custom name.

### 6. Provider discovery

**When template uses `{git}`, how do we know to use GitProvider?**

**Option A: Hardcoded mapping**
```rust
fn determine_provider(var_name: &str) -> Option<&str> {
    match var_name {
        "git" | "git_dirty" | "git_ahead" => Some("git"),
        "ip" => Some("ip"),
        "battery" | "battery_percent" => Some("battery"),
        _ => None,
    }
}
```

**Option B: Provider registration includes variable names**
```rust
pub trait Provider {
    fn name(&self) -> &str;
    fn provides_variables(&self) -> Vec<&str>; // ["git", "git_dirty", ...]
}
```

**Option C: Variable prefix convention**
All variables starting with `git_` come from git provider, etc.

**Recommendation**: Option C (prefix convention). Simple, scalable, self-documenting.

### 7. Daemon integration

Some providers (hostname) can be cached by daemon.
Others (git) should run live every time.

**Should daemon support plugins?**

**Recommendation**: Phase 1 - don't worry about daemon. Phase 2 - add `cacheable()` method to trait.

### 8. Error handling

What happens if git command fails? Network is down for IP? Battery doesn't exist?

**Options**:
A. Return empty string (silent failure)
B. Return error string: `{git}` = `"<error>"`
C. Return None, variable is omitted from template

**Recommendation**: Option A (empty string). Silent, graceful degradation.

### 9. Implicit section creation

Currently, using `{time}` creates `[time]` section implicitly.

**Should plugins work the same way?**

Using `{git}` without `[git]` section:
- Should it auto-create `[git]` section with defaults?
- Or require explicit `[git]` section?

**Recommendation**: Yes, implicit creation. Consistent with current behavior. Provider.default_config() provides defaults.

## Next Steps

**Decision point**: Do we want to implement this architecture before adding git?

**Option A: Refactor first** (Recommended)
1. Implement provider architecture (Phase 1-2)
2. Add git as first plugin (Phase 3)
3. Add other plugins (Phase 4)

**Option B: Add git first, refactor later**
1. Hardcode git in main.rs (quick)
2. Refactor to plugins later (when we add more providers)

**Recommendation**: Option A. The architecture is solid, and it prevents accumulating more technical debt.

**Immediate tasks if we proceed**:
1. Answer the open questions above
2. Create providers/mod.rs with trait definition
3. Create stub providers (git, ip, battery)
4. Test that architecture works
5. Implement GitProvider.collect()
6. Test git in prompt template

## Benefits of Plugin Architecture

1. **Scalability**: Add unlimited providers without touching core
2. **Testability**: Each provider is isolated and testable
3. **Maintainability**: Clear boundaries, single responsibility
4. **Extensibility**: Third-party plugins possible in future
5. **Performance**: Only query providers actually used in template
6. **Consistency**: All providers follow same pattern
7. **Discovery**: Easy to list available providers
8. **Documentation**: Each provider documents its variables

## Risks and Mitigation

**Risk 1: Over-engineering**
- Mitigation: Keep trait simple, don't add features we don't need yet

**Risk 2: Performance overhead**
- Mitigation: Only call providers that are used (lazy evaluation)

**Risk 3: Breaking changes**
- Mitigation: Migrate incrementally, keep existing code working

**Risk 4: Complexity for users**
- Mitigation: Implicit sections, sensible defaults, clear docs

## Example User Flow

**User wants git in prompt:**

1. Edit config:
```toml
[prompt]
format = '{time:cyan} {git:yellow} {hostname:magenta}'
```

2. No `[git]` section needed - it's implicit!

3. Run twig:
```bash
twig --mode tcsh
```

4. In git repo: Shows branch name in yellow
5. Not in git repo: Shows nothing (empty)
6. Works immediately, no explicit config needed

**User wants to customize git:**

1. Add config section:
```toml
[git]
name = "branch"  # Use {branch} instead of {git}
```

2. Update template:
```toml
[prompt]
format = '{time:cyan} {branch:yellow} {hostname:magenta}'
```

**User wants advanced git info (future depth):**

```toml
[git]
show_dirty = true
show_ahead = true

[prompt]
format = '{time} {git:yellow} {git_dirty:red} +{git_ahead:green}'
```

## Conclusion

The plugin architecture provides a clean, scalable foundation for adding providers. It's the right approach before adding git support.

**Decision needed**: Should we implement this architecture now, or hardcode git first?

**Recommended**: Implement architecture first. It's well-designed and prevents future refactoring pain.
