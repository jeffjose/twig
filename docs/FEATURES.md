# Twig - Deep Analysis & Feature Documentation

## What Twig Is

**Twig** is a shell prompt engine - a Rust utility that generates dynamic, colorized terminal prompts with real-time system information. Think of it as a modern, highly configurable alternative to tools like `powerline` or `starship`, but with a unique daemon-based caching architecture.

## Core Concept & Architecture

Twig uses a **daemon + client architecture**:
1. A background daemon (`twigd`) continuously fetches expensive system data (hostname, IP, battery) and caches it to disk as JSON
2. The interactive client (`twig`) reads this cache for instant prompts, falling back to live data only when needed

This architecture solves the "expensive prompt" problem where fetching battery/network info on every shell prompt would slow down the user experience.

---

## Key Features (Every Nuance)

### 1. Data Providers - Six Core Modules

Each module can be configured multiple times with different settings:

#### Time
- Custom strftime formatting (`%H:%M:%S`, `%Y-%m-%d`, etc.)
- Full chrono library support
- Validation of format strings
- Multiple time displays with different formats

#### Hostname
- System hostname retrieval
- Can be deferred (don't fetch unless explicitly requested)
- Unicode pathname support

#### IP Address
- IPv4 and IPv6 support
- Optional interface filtering (`eth0`, `wlan0`, etc.)
- List available network interfaces
- Comprehensive error handling

#### CWD (Current Working Directory)
- Full path or basename-only display
- `shorten` option to show only directory name
- Unicode-aware path handling

#### Power/Battery
Incredibly detailed battery information:
- **Status**: Percentage, charging state (Charging/Discharging/Full)
- **Time**: Time to full/empty
- **Power**: Current power draw (positive = charging, negative = discharging)
- **Energy**: Current energy, full capacity
- **Hardware**: Voltage, temperature, capacity, cycle count
- **Metadata**: Technology, manufacturer, model, serial number
- Graceful handling of systems without batteries
- Customizable output format with variable substitution

#### Environment Variables
- Direct access to any environment variable
- Syntax: `{$USER}`, `{$PWD}`, `{$HOME}`, etc.

---

### 2. Template System - The UX Power

The format string syntax supports:

#### Variable Substitution
```
{datetime} {host} {dir}
```

#### Inline Coloring
```
{datetime:cyan} {host:magenta,bold} {dir:green}
```

#### Available Colors
- **Basic**: red, green, yellow, blue, magenta, cyan, white
- **Bright variants**: bright_red, bright_green, bright_blue, etc.
- **Styles**: bold, italic, normal
- **Combined**: `{var:cyan,bold}` or `{var:bright_red,italic}`

#### Literal Text with Colors
```
{"→":cyan} {"$":yellow,bold} {" | ":white}
```

#### Environment Variables
```
{$USER:yellow}@{host} {$PWD:blue}
```

#### Shell-Specific Output Modes
- **Default**: Standard ANSI escape codes for bash
- **TCSH mode** (`--mode tcsh`): Wraps codes in `%{...%}` for tcsh compatibility
- **Other shells**: Extensible for future shell-specific formatting

#### Multiline Support
- Proper handling of newlines in format strings
- ANSI code wrapping per line

---

### 3. Configuration System (config.toml)

#### Multiple Instances of Same Type
```toml
[[time]]
name = "time1"
format = "%H:%M"

[[time]]
name = "time2"
format = "%Y-%m-%d"
```

**Note**: If you only have one instance of a type, `name` is optional - it defaults to the section type name.

#### Deferred Execution
```toml
[[hostname]]
name = "host"
deferred = true  # Don't fetch unless explicitly requested
```

This is a performance optimization - some data isn't always needed, so you can skip fetching it unless the format string requests it.

#### Daemon Configuration
```toml
[daemon]
frequency = 1      # Update cache every N seconds
stale_after = 5    # Consider data stale after N seconds
data_file = "/custom/path/data.json"  # Optional custom cache file path
```

Controls cache freshness - a balance between system load and data accuracy.

#### Power/Battery Format Examples
```toml
[[power]]
name = "bat"
format = "{percentage}% ({power_now}W)"

# Available variables:
# {percentage}, {status}, {time_left}, {power_now}, {energy_now},
# {energy_full}, {voltage}, {temperature}, {capacity}, {cycle_count},
# {technology}, {manufacturer}, {model}, {serial}
```

#### IP Interface Selection
```toml
[[ip]]
name = "ip"
interface = "eth0"  # Optional: specific network interface
```

#### CWD Shortening
```toml
[[cwd]]
name = "dir"
shorten = false  # If true, shows only directory basename
```

#### Full Example Configuration
```toml
[[time]]
name = "datetime"
format = "%H:%M:%S"

[[hostname]]
name = "host"

[[ip]]
name = "ip"

[[cwd]]
name = "dir"
shorten = false

[[power]]
name = "bat"
format = "{percentage}% ({power_now}W)"

[prompt]
format = "({datetime:cyan} {host:magenta} {ip:magenta} {dir:green}) $ "

[daemon]
frequency = 1
stale_after = 5
```

---

### 4. Performance Analysis (`--timing` flag)

Sophisticated performance monitoring showing:
- **Per-module fetch time**: How long each data provider took
- **Cache hits vs. live fetches**: What came from cache vs. fresh data
- **Deferred section stats**: How many sections were skipped
- **Configuration load time**: Time to parse config.toml
- **Total execution breakdown**: Complete timing of all operations
- **Tree visualization**: Slowest operations shown first

Output goes to stderr so it doesn't pollute the prompt output.

Example output:
```
Configuration loaded in 2.1ms
Data providers:
  ├─ time: 0.3ms (live)
  ├─ hostname: 0.1ms (cached)
  ├─ ip: 15.2ms (live)
  ├─ cwd: 0.2ms (live)
  ├─ power: 12.5ms (cached)
  └─ env: 0.1ms (live)
Template processing: 1.2ms
Total: 31.7ms
```

---

### 5. Daemon System - Background Intelligence

#### Key Behaviors
- **Auto-forking**: Automatically backgrounds itself (unless `--fg` flag used)
- **File locking**: Prevents multiple daemon instances from running
- **Graceful shutdown**: Handles Ctrl+C and cleanup
- **Configurable frequency**: Updates cache at user-defined intervals
- **Deferred section support**: Request files allow on-demand data fetching

#### Cache Strategy
- **Daemon writes**: `~/.local/share/twig/data.json` (or custom path)
- **Client reads**: With staleness check based on `stale_after` config
- **Fallback**: If cache is stale or missing, falls back to live data
- **Request mechanism**: Deferred sections can be requested via request file

#### Running the Daemon
```bash
# Background mode (default)
twig daemon

# Foreground mode for debugging
twig daemon --fg
```

---

### 6. UX Polish Features

#### Auto-Config Creation
- First run auto-creates `~/.config/twig/config.toml` with sensible defaults
- No manual setup required - works out of the box

#### Color Preview (`--colors`)
- Shows all available colors and style combinations
- Visual reference to help users design their prompts
- Displays actual ANSI codes in terminal

#### Validation (`--validate`)
- Shows configuration errors and warnings
- Helps debug format string issues
- Validates time format strings, variable names, etc.

#### Parallel Data Fetching
- All 6 data providers run concurrently via tokio async runtime
- Much faster than serial execution
- Non-blocking architecture

#### Custom Config Path
```bash
twig --config /path/to/custom/config.toml
```

---

## Technical Implementation Details

### Language & Build System
- **Language**: Rust (for performance, safety, and cross-platform support)
- **Build**: Cargo package manager
- **Async Runtime**: Tokio for concurrent operations

### Key Dependencies
- `chrono` - Time/date handling with timezone support
- `toml` - TOML configuration parsing
- `serde` / `serde_json` - Serialization for config and cache
- `clap` - Command-line argument parsing (derive-based)
- `tokio` - Async runtime
- `battery` - Cross-platform battery information
- `local-ip-address` - IP address detection
- `hostname` - System hostname retrieval
- `colored` - Terminal color output
- `directories` - Platform-specific user directories
- `fs2` - File locking for daemon
- `libc` - Unix system calls (for forking)
- `regex` - Regular expressions for template parsing

### File Locations (Linux/Unix)
- **Config**: `~/.config/twig/config.toml`
- **Cache**: `~/.local/share/twig/data.json`
- **Lock file**: `~/.local/share/twig/daemon.lock`
- **Request file**: `~/.local/share/twig/request` (for deferred sections)

### Error Handling
- Graceful degradation - missing data doesn't crash
- All errors caught and optionally reported with `--validate` or `--timing`
- Battery module returns None on systems without batteries
- Network errors fall back to empty IP

---

## Usage Examples

### Basic Usage
```bash
# Generate prompt with default config
twig

# Use custom config
twig --config ~/.twig.toml

# Show timing information
twig --timing

# Validate configuration
twig --validate

# Display available colors
twig --colors

# Set output mode for specific shell
twig --mode tcsh
```

### In Shell Configuration

**Bash** (~/.bashrc):
```bash
# Start daemon on shell startup (if not running)
twig daemon 2>/dev/null &

# Use in PS1
PS1='$(twig) '
```

**TCSH** (~/.tcshrc):
```tcsh
# Start daemon
twig daemon >& /dev/null &

# Use in prompt
set prompt=`twig --mode tcsh`
```

---

## Architecture Strengths

### What Works Well
1. **Daemon caching** - Elegant solution to expensive prompt generation
2. **Async/parallel execution** - All data providers fetch concurrently
3. **Deferred sections** - Smart optimization for rarely-needed data
4. **Rich templating** - Powerful color and style system
5. **Cross-platform** - Works on Linux, macOS, BSD
6. **Performance monitoring** - `--timing` flag for optimization
7. **Graceful degradation** - Missing features don't crash

### Design Philosophy
- **Fast by default**: Daemon caching means prompts appear instantly
- **Configurable everything**: Colors, formats, frequencies, paths
- **Smart defaults**: Works out of the box with sensible configuration
- **Non-invasive**: Daemon runs quietly in background
- **Shell-agnostic**: Works with bash, tcsh, zsh, fish, etc.

---

## Future Considerations

### Potential Enhancements
- Git repository information (branch, dirty status, ahead/behind)
- Kubernetes context display
- Docker container status
- Custom command output (run arbitrary commands)
- Conditional formatting (show battery only when low)
- Themes/presets for common configurations
- Plugin system for custom data providers

### Cross-Platform Improvements
- Windows support (currently Unix-focused)
- Better macOS battery handling
- Android/Termux support

### Performance Optimizations
- Binary cache format instead of JSON
- Memory-mapped files for IPC
- Unix sockets instead of file-based communication
- Compiled template format

---

## Testing

### Current Test Coverage
- Unit tests for all modules (time, hostname, ip, cwd, power)
- Parameterized testing with `rstest`
- Edge case handling (Unicode, missing batteries, invalid configs)
- Temporary file handling for test isolation
- Performance benchmarks with timing

### Areas for Expansion
- Integration tests (full prompt generation flow)
- Daemon lifecycle tests
- Cache invalidation tests
- Shell-specific output validation
- Load testing with multiple concurrent clients

---

## Summary

Twig is a **production-grade shell prompt utility** that combines:
- Real-time system information display
- Intelligent background caching
- Asynchronous data fetching
- Sophisticated templating with colors and styles
- High performance and low latency
- Excellent user experience with sensible defaults

The daemon-based architecture is the key innovation, allowing expensive operations (battery queries, network detection) to run in the background while keeping the interactive prompt instantaneous.
