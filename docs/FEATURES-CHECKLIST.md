# Twig Features Checklist - Phased Development

**Strategy**: Build incrementally from simple to complex. Each phase adds value and can be tested/used independently.

---

## Phase 1: Core Foundation (Hardcoded, No Config, No Colors)

**Goal**: Get basic data providers working with simple template substitution

### Basic Data Providers
- [x] Get current working directory
- [x] Get current time (basic - just `%H:%M:%S`)
- [x] Get hostname
- [x] Multiple variables in one format string (hardcoded: `{time} {host} {dir} $ `)

### Simple Template System
- [x] Basic variable replacement (e.g., `{dir}`)
- [x] Multiple variables in one format string
- [x] Handle missing variables gracefully

### Output
- [x] Print to stdout without newline (for shell prompts)

**Deliverable**: ✅ `twig` works with hardcoded format showing time, hostname, and directory

---

## Phase 2: Template System (Colors & Styles)

**Goal**: Add visual formatting to make prompts beautiful

### Color Support
- [x] Basic colors: red, green, yellow, blue, magenta, cyan, white
- [x] Bright color variants: bright_red, bright_green, bright_blue, etc.
- [x] Color syntax: `{variable:color}`

### Text Styles
- [x] Bold style
- [x] Italic style
- [x] Normal style
- [x] Combined color + style: `{variable:cyan,bold}`

### Literal Text
- [x] Literal text with colors: `{"text":color}`
- [x] Literal text with styles: `{"→":cyan,bold}`
- [x] Plain literal text without formatting

### ANSI Output
- [x] Standard ANSI escape codes (bash, zsh, fish)
- [x] Proper code generation for colors
- [x] Proper code generation for styles

**Deliverable**: ✅ `twig` outputs colorized prompts with hardcoded format like `{time:cyan} {host:magenta} {dir:green} $ `

---

## Phase 3: Configuration System

**Goal**: Read settings from TOML file instead of hardcoding

### Config File Basics
- [x] TOML-based configuration
- [x] Default location: `~/.config/twig/config.toml`
- [x] Custom config path via `--config` flag
- [x] Auto-create default config on first run
- [x] Auto-create config directory if needed

### Section Configuration
- [x] `[time]` section with `format` field
- [x] `[hostname]` section
- [x] `[cwd]` section
- [x] `[prompt]` section for format string

### Single Instance Per Type
- [x] Parse one `[time]` section
- [x] Parse one `[hostname]` section
- [x] Parse one `[cwd]` section
- [x] Use `[prompt]` format string

### Implicit Sections & Custom Names
- [x] Implicit section creation from template variables
- [x] Section name = variable name by default
- [x] Optional `name` field to override variable name
- [x] Template parsing to discover needed variables

### Error Handling
- [x] Handle missing config file (auto-creates with defaults)
- [ ] Handle malformed TOML (panics currently)
- [ ] Show clear error messages

**Deliverable**: ✅ `twig` reads from `~/.config/twig/config.toml` and uses configured format and time format

---

## Phase 4: Enhanced Data Providers

**Goal**: Add more data sources

### CWD Enhancements
- [ ] Full path display mode
- [ ] Shortened display mode (basename only via `shorten` option)
- [ ] Unicode-aware path handling

### Time Enhancements
- [ ] Support all strftime format specifiers
- [ ] Validate time format strings
- [ ] Timezone support

### Environment Variables
- [x] Access environment variables in format strings
- [x] Syntax: `{$USER}`, `{$PWD}`, `{$HOME}`, etc.
- [x] Support any environment variable
- [x] Gracefully handle missing env vars

### Hostname Enhancements
- [ ] Handle Unicode pathnames

**Deliverable**: More flexible prompts with env vars and configurable time/cwd formatting

---

## Phase 5: Git Provider

**Goal**: Add git repository information (essential for modern prompts)

### Basic Git Info
- [ ] Detect if current directory is in a git repo
- [ ] Get current branch name
- [ ] Show when not in a git repo (empty string)
- [ ] `[git]` config section

### Git Status Indicators
- [ ] Detect dirty/clean status (uncommitted changes)
- [ ] Show ahead/behind remote status
- [ ] Detect staged vs unstaged changes
- [ ] Handle detached HEAD state

**Deliverable**: Prompts can show git branch like `{git:yellow}` or `{git:red}` when dirty

---

## Phase 6: IP Address Provider

**Goal**: Add network information

### IP Address Module
- [ ] Get IPv4 addresses
- [ ] Get IPv6 addresses
- [ ] Support specific interface selection (e.g., `eth0`, `wlan0`)
- [ ] Handle missing/down interfaces gracefully
- [ ] `[ip]` config section with optional `interface` field

**Deliverable**: Prompts can show IP address like `{ip:blue}`

---

## Phase 7: CLI Interface & UX Polish

**Goal**: Better command-line experience

### Basic CLI
- [x] `twig` - Generate prompt
- [ ] `twig --help` - Show help with usage examples
- [ ] `twig --version` - Show version info

### CLI Options
- [ ] `--config <PATH>` - Use custom config file
- [ ] `--validate` - Show config validation errors
- [ ] `--colors` - Display available colors and styles

### Config Validation
- [ ] Validate config on load
- [ ] Show errors with `--validate` flag
- [ ] Show warnings for deprecated/unknown fields
- [ ] Validate time format strings
- [ ] Validate variable names in format strings

### Color Preview
- [ ] `--colors` flag displays all colors
- [ ] Show basic colors
- [ ] Show bright variants
- [ ] Show text styles (bold, italic)
- [ ] Show combined color+style examples
- [ ] Render in actual terminal colors

### Error Handling
- [ ] Graceful degradation (missing data doesn't crash)
- [ ] Clear error messages
- [ ] Warnings for config issues

**Deliverable**: Professional CLI with help, validation, and color preview

---

## Phase 8: Battery/Power Provider

**Goal**: Add battery information

### Basic Battery Info
- [ ] Display battery percentage
- [ ] Display charging status (Charging/Discharging/Full)
- [ ] Gracefully handle systems without batteries
- [ ] `[power]` config section

**Deliverable**: Prompts can show battery like `{power:yellow}`

---

## Phase 9: Advanced Configuration

**Goal**: Support complex configurations

### Multiple Instances
- [ ] Support multiple instances of same type (e.g., two `[[time]]` sections)
- [ ] Require `name` field when multiple instances exist
- [ ] Auto-name single instances (defaults to section type)

### Per-Provider Options
- [ ] `name` field for all providers
- [ ] `format` field for time provider
- [ ] `shorten` field for cwd provider
- [ ] `interface` field for ip provider

### Advanced Template Features
- [ ] Multiline format support
- [ ] Proper newline handling
- [ ] Escaped characters in format strings
- [ ] Nested variable support

**Deliverable**: Complex prompts with multiple time zones, multiple IPs, etc.

---

## Phase 10: Shell-Specific Output

**Goal**: Work perfectly with different shells

### Shell Modes
- [x] Standard ANSI escape codes (raw mode, default)
- [x] TCSH-specific formatting with `%{...%}` wrapping
- [x] Bash-specific formatting with `\[...\]` wrapping
- [x] Zsh-specific formatting with `%{...%}` wrapping
- [x] Mode selection via `--mode` flag
- [x] Proper ANSI code wrapping per line
- [x] Extensible architecture for future shell support

### Shell Integration
- [x] Works with bash (use `--mode bash`)
- [x] Works with zsh (use `--mode zsh`)
- [x] Works with fish (use `--prompt` for raw ANSI)
- [x] Works with tcsh (use `--mode tcsh`)

### Architecture
- [x] Shell output formatter abstraction (ShellFormatter trait)
- [x] Separate module per shell (shell/raw.rs, bash.rs, zsh.rs, tcsh.rs)
- [x] Factory pattern for shell mode selection (get_formatter)
- [x] `--mode` flag controls shell-specific output format
- [x] Flag behavior: twig (boxed), --prompt (raw), --mode <shell> (shell-specific)

**Deliverable**: ✅ `twig --mode tcsh` outputs tcsh-compatible prompt with `%{...%}` wrapped ANSI codes

---

## Phase 11: Performance & Async Execution

**Goal**: Make it fast with parallel data fetching

### Async Runtime
- [ ] Add tokio dependency
- [ ] Convert to async/await architecture
- [ ] Async/concurrent data fetching
- [ ] All providers run in parallel
- [ ] Non-blocking architecture

### Timing Analysis
- [ ] `--timing` flag for performance breakdown
- [ ] Show per-module fetch times
- [ ] Show configuration load time
- [ ] Show template processing time
- [ ] Show total execution time breakdown
- [ ] Tree-like visual output
- [ ] Sort by slowest operations first
- [ ] Output to stderr (don't pollute prompt)

### Performance Goals
- [ ] Fast execution (< 50ms typical)

**Deliverable**: Blazing fast prompt generation with timing diagnostics

---

## Phase 12: Advanced Battery Info

**Goal**: Detailed battery information

### Advanced Battery Info
- [ ] Display time to full
- [ ] Display time to empty
- [ ] Display power draw (watts, positive=charging, negative=discharging)
- [ ] Display current energy level
- [ ] Display full energy capacity
- [ ] Display voltage
- [ ] Display temperature
- [ ] Display capacity percentage
- [ ] Display cycle count
- [ ] Display battery technology
- [ ] Display manufacturer
- [ ] Display model name
- [ ] Display serial number

### Power Format String
- [ ] Customizable format string with variable substitution
- [ ] `{percentage}` variable
- [ ] `{status}` variable
- [ ] `{time_left}` variable
- [ ] `{power_now}` variable
- [ ] `{energy_now}` variable
- [ ] `{energy_full}` variable
- [ ] `{voltage}` variable
- [ ] `{temperature}` variable
- [ ] `{capacity}` variable
- [ ] `{cycle_count}` variable
- [ ] `{technology}` variable
- [ ] `{manufacturer}` variable
- [ ] `{model}` variable
- [ ] `{serial}` variable

**Deliverable**: Prompts can show detailed battery info like `{bat:yellow}` where bat format is `{percentage}%`

---

## Phase 13: Daemon System - Background Process

**Goal**: Start the daemon infrastructure

### Background Operation
- [ ] `twig daemon` subcommand (currently separate `twigd` binary)
- [ ] Auto-fork to background process
- [ ] Foreground mode for debugging (`--fg`)
- [ ] File-based locking to prevent multiple instances
- [ ] PID file management
- [x] Graceful shutdown handling (Ctrl+C)

### Daemon Configuration
- [ ] `[daemon]` section in config
- [x] Hardcoded 1 second update interval (config support pending)
- [x] Hardcoded 5 second staleness threshold (config support pending)
- [x] Default cache location: `~/.local/share/twig/data.json`

**Deliverable**: ✅ Basic `twigd` daemon runs (foreground only for now)

---

## Phase 14: Daemon System - Caching

**Goal**: Make daemon actually cache data

### Caching Strategy
- [x] Periodic data updates at configured frequency
- [x] Write cache to JSON file
- [x] Default cache location: `~/.local/share/twig/data.json`
- [ ] Custom cache file path support
- [x] Staleness checking based on `stale_after` config

### Cache Integration
- [x] Client reads from cache if fresh
- [x] Fall back to live data if stale
- [x] Fall back to live data if cache missing
- [x] Fast cache reads (JSON deserialization)

### Daemon Data Providers
- [x] Cache hostname data
- [ ] Cache IP address data
- [ ] Cache battery/power data
- [x] Update at configured frequency (hardcoded 1s)
- [x] Handle errors gracefully

### Timing Updates
- [x] Show cache status in output (cached/live)
- [x] Show daemon detection status

**Deliverable**: ✅ Daemon caches hostname, client uses it for instant prompts

---

## Phase 15: Deferred Sections (Advanced Caching)

**Goal**: Skip expensive operations unless explicitly needed

### Deferred Configuration
- [ ] `deferred = true` option for any provider
- [ ] Skip deferred sections unless format string uses them
- [ ] Mark hostname as deferrable
- [ ] Mark IP as deferrable
- [ ] Mark battery as deferrable

### Request Mechanism
- [ ] Request file mechanism for on-demand data
- [ ] Request file location: `~/.local/share/twig/request`
- [ ] Daemon watches request file
- [ ] Client can request deferred data

### Timing Updates
- [ ] Show deferred section counts in `--timing`

**Deliverable**: Even faster prompts by skipping unused expensive operations

---

## Phase 16: Platform Support & Polish

**Goal**: Cross-platform compatibility

### Platform Support
- [ ] Linux support (already works)
- [ ] macOS support
- [ ] BSD support
- [ ] Cross-platform battery detection
- [ ] Cross-platform network interfaces
- [ ] Cross-platform hostname

### Final Polish
- [ ] Sensible default values
- [ ] Works out of the box with no configuration
- [ ] Network errors return empty string
- [ ] File permission errors handled gracefully

**Deliverable**: Works on Linux, macOS, and BSD

---

## Phase 17: Documentation

**Goal**: Help users get started and troubleshoot

### User Documentation
- [ ] README with quick start
- [ ] Configuration examples
- [ ] Shell integration examples (bash, zsh, tcsh, fish)
- [ ] Troubleshooting guide
- [ ] Performance tuning guide

### Help Output
- [ ] `--help` shows usage
- [ ] Subcommand help (`twig daemon --help`)
- [ ] Examples in help text
- [ ] Config file location in help

**Deliverable**: Complete documentation for users

---

## Summary Stats

- **Total Phases**: 17
- **Completed Phases**: 4 (Phases 1-3, 10 complete!)
- **Partial Progress**: Phase 4 (environment variables done), Phase 13-14 (basic daemon done)
- **Total Features**: ~160+
- **Completed Features**: ~65 ✅

---

## Current Status

### ✅ Phase 1 Complete!
- [x] Get current working directory
- [x] Get current time
- [x] Get hostname
- [x] Multiple variables in one format string
- [x] Basic variable replacement
- [x] Handle missing variables gracefully

### ✅ Phase 2 Complete!
- [x] Basic colors (red, green, yellow, blue, magenta, cyan, white)
- [x] Bright color variants
- [x] Color syntax: `{variable:color}`
- [x] Text styles (bold, italic, normal)
- [x] Combined color + style
- [x] Literal text with colors
- [x] ANSI escape codes

### ✅ Phase 3 Complete!
- [x] TOML-based configuration
- [x] Default location: `~/.config/twig/config.toml`
- [x] Custom config path via `--config` flag
- [x] Auto-create default config
- [x] Config sections: [time], [hostname], [cwd], [prompt]
- [x] Parse and apply config
- [x] Implicit section creation from template
- [x] Section name = variable name (no magic)
- [x] Optional name field to override variable names

### 🔨 Phase 4 In Progress
- [x] Environment variables (`{$USER}`, `{$HOME}`, etc.)
- [ ] CWD enhancements (shorten, etc.)
- [ ] Time format validation
- [ ] Timezone support

### ✅ Phase 10 Complete! (Shell-Specific Output)
- [x] TCSH formatter with `%{...%}` wrapping
- [x] Bash formatter with `\[...\]` wrapping
- [x] Zsh formatter with `%{...%}` wrapping
- [x] Raw formatter for `--prompt` flag
- [x] `--mode` flag (tcsh, bash, zsh)
- [x] Extensible shell formatter architecture
- [x] Separate modules (shell/raw.rs, bash.rs, zsh.rs, tcsh.rs)

### ✅ Phase 10 Complete!
- [x] Shell-specific ANSI wrapping (TCSH, Bash, Zsh, Raw)
- [x] `--mode` flag for shell selection
- [x] Extensible ShellFormatter architecture

### 🎯 Next Steps (Breadth-First Approach)

**Missing Moving Pieces** (core architectural components):
1. **Git Provider** (Phase 5) - Branch name, dirty status
2. **IP Provider** (Phase 6) - Basic IPv4/IPv6
3. **CLI Interface** (Phase 7) - --help, --version, --colors
4. **Battery Provider** (Phase 8) - Basic percentage and status

**Current Focus**: Git provider would be the most valuable next addition for shell prompts.
