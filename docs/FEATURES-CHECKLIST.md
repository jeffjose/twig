# Twig Features Checklist - Phased Development

**Strategy**: Build incrementally from simple to complex. Each phase adds value and can be tested/used independently.

---

## Phase 1: Core Foundation (Hardcoded, No Config, No Colors)

**Goal**: Get basic data providers working with simple template substitution

### Basic Data Providers
- [x] Get current working directory
- [ ] Get current time (basic - just `%H:%M:%S`)
- [ ] Get hostname
- [ ] Multiple variables in one format string (hardcoded: `{time} {host} {dir} $ `)

### Simple Template System
- [x] Basic variable replacement (e.g., `{dir}`)
- [ ] Multiple variables in one format string
- [ ] Handle missing variables gracefully

### Output
- [x] Print to stdout without newline (for shell prompts)

**Deliverable**: `twig` works with hardcoded format showing time, hostname, and directory

---

## Phase 2: Template System (Colors & Styles)

**Goal**: Add visual formatting to make prompts beautiful

### Color Support
- [ ] Basic colors: red, green, yellow, blue, magenta, cyan, white
- [ ] Bright color variants: bright_red, bright_green, bright_blue, etc.
- [ ] Color syntax: `{variable:color}`

### Text Styles
- [ ] Bold style
- [ ] Italic style
- [ ] Normal style
- [ ] Combined color + style: `{variable:cyan,bold}`

### Literal Text
- [ ] Literal text with colors: `{"text":color}`
- [ ] Literal text with styles: `{"→":cyan,bold}`
- [ ] Plain literal text without formatting

### ANSI Output
- [ ] Standard ANSI escape codes (bash, zsh, fish)
- [ ] Proper code generation for colors
- [ ] Proper code generation for styles

**Deliverable**: `twig` outputs colorized prompts with hardcoded format like `{time:cyan} {host:magenta} {dir:green} $ `

---

## Phase 3: Configuration System

**Goal**: Read settings from TOML file instead of hardcoding

### Config File Basics
- [ ] TOML-based configuration
- [ ] Default location: `~/.config/twig/config.toml`
- [ ] Custom config path via `--config` flag
- [ ] Auto-create default config on first run
- [ ] Auto-create config directory if needed

### Section Configuration
- [ ] `[[time]]` section with `format` field
- [ ] `[[hostname]]` section with `name` field
- [ ] `[[cwd]]` section with `name` field
- [ ] `[prompt]` section for format string

### Single Instance Per Type
- [ ] Parse one `[[time]]` section
- [ ] Parse one `[[hostname]]` section
- [ ] Parse one `[[cwd]]` section
- [ ] Use `[prompt]` format string

### Error Handling
- [ ] Handle missing config file (use defaults)
- [ ] Handle malformed TOML
- [ ] Show clear error messages

**Deliverable**: `twig` reads from `~/.config/twig/config.toml` and uses configured format and time format

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
- [ ] Access environment variables in format strings
- [ ] Syntax: `{$USER}`, `{$PWD}`, `{$HOME}`, etc.
- [ ] Support any environment variable
- [ ] Gracefully handle missing env vars

### Hostname Enhancements
- [ ] Handle Unicode pathnames

**Deliverable**: More flexible prompts with env vars and configurable time/cwd formatting

---

## Phase 5: IP Address Provider

**Goal**: Add network information

### IP Address Module
- [ ] Get IPv4 addresses
- [ ] Get IPv6 addresses
- [ ] Support specific interface selection (e.g., `eth0`, `wlan0`)
- [ ] Handle missing/down interfaces gracefully
- [ ] `[[ip]]` config section with optional `interface` field

**Deliverable**: Prompts can show IP address like `{ip:blue}`

---

## Phase 6: CLI Interface & UX Polish

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

## Phase 7: Advanced Configuration

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

## Phase 8: Shell-Specific Output

**Goal**: Work perfectly with different shells

### Shell Modes
- [ ] Standard ANSI escape codes (bash, zsh, fish)
- [ ] TCSH-specific formatting with `%{...%}` wrapping
- [ ] Mode selection via `--mode` flag
- [ ] Proper ANSI code wrapping per line

### Shell Integration
- [ ] Works with bash
- [ ] Works with zsh
- [ ] Works with fish
- [ ] Works with tcsh

**Deliverable**: `twig --mode tcsh` works correctly in tcsh prompts

---

## Phase 9: Battery/Power Provider

**Goal**: Show battery information

### Basic Battery Info
- [ ] Display battery percentage
- [ ] Display charging status (Charging/Discharging/Full)
- [ ] Gracefully handle systems without batteries
- [ ] `[[power]]` config section

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

## Phase 10: Performance & Async Execution

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

## Phase 11: Daemon System - Background Process

**Goal**: Start the daemon infrastructure

### Background Operation
- [ ] `twig daemon` subcommand
- [ ] Auto-fork to background process
- [ ] Foreground mode for debugging (`--fg`)
- [ ] File-based locking to prevent multiple instances
- [ ] PID file management
- [ ] Graceful shutdown handling (Ctrl+C)

### Daemon Configuration
- [ ] `[daemon]` section in config
- [ ] `frequency` setting (update interval in seconds)
- [ ] `stale_after` setting (cache staleness threshold)
- [ ] `data_file` setting (custom cache file path)

**Deliverable**: `twig daemon` runs in background, `twig daemon --fg` runs in foreground

---

## Phase 12: Daemon System - Caching

**Goal**: Make daemon actually cache data

### Caching Strategy
- [ ] Periodic data updates at configured frequency
- [ ] Write cache to JSON file
- [ ] Default cache location: `~/.local/share/twig/data.json`
- [ ] Custom cache file path support
- [ ] Staleness checking based on `stale_after` config

### Cache Integration
- [ ] Client reads from cache if fresh
- [ ] Fall back to live data if stale
- [ ] Fall back to live data if cache missing
- [ ] Fast cache reads (JSON deserialization)

### Daemon Data Providers
- [ ] Cache hostname data
- [ ] Cache IP address data
- [ ] Cache battery/power data
- [ ] Update at configured frequency
- [ ] Handle errors gracefully

### Timing Updates
- [ ] Show cache vs. live data stats in `--timing`
- [ ] Show daemon detection status

**Deliverable**: Daemon caches expensive data, client uses it for instant prompts

---

## Phase 13: Deferred Sections (Advanced Caching)

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

## Phase 14: Platform Support & Polish

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

## Phase 15: Documentation

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

- **Total Features**: ~150+
- **Completed**: 2 ✅
- **Current Phase**: Phase 1 (3/4 features remaining)

---

## Current Status

### ✅ Phase 1 Progress (2/4)
- [x] Get current working directory
- [x] Basic variable replacement
- [ ] Get current time
- [ ] Get hostname
- [ ] Multiple variables in one format string

### 🎯 Immediate Next Steps
1. Add time provider (current time with hardcoded format)
2. Add hostname provider
3. Support multiple variables in template (`{time} {host} {dir}`)
4. **Phase 1 Complete** ✅

Then move to Phase 2 for colors!
