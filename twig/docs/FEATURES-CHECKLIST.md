# Twig Features Checklist

This document tracks the implementation status of all features in the twig prompt generator.

## Core Architecture

- [x] **Config System** - TOML-based configuration with implicit section support
- [x] **Provider System** - Pluggable provider architecture for data sources
- [x] **Provider Registry** - Automatic provider registration and routing
- [x] **Template Engine** - Variable substitution with styling support
- [x] **Shell Formatters** - Shell-specific output formatting

## Template Features

- [x] **Variable Substitution** - `{variable_name}` syntax
- [x] **Color Styling** - `{variable:color}` syntax
  - [x] Basic colors (black, red, green, yellow, blue, magenta, cyan, white)
  - [x] Bright colors (bright_black, bright_red, etc.)
- [x] **Text Styling** - `{variable:color,style}` syntax
  - [x] Bold styling
  - [x] Italic styling
  - [x] Underline styling
- [x] **Literal Text** - `{"text":color,style}` for static text with styling
- [x] **Environment Variables** - `{$VAR_NAME}` expansion
- [x] **Conditional Spacing** - `~` character for spaces that collapse when next variable is empty
  - [x] Basic conditional spacing
  - [x] Multiple conditional spaces
  - [x] Conditional spacing with colors
  - [x] Escaped tilde (`\~`) for literal tilde

## Providers

### Builtin Provider
- [x] **Time** - `{time}` with customizable format (strftime)
- [x] **Hostname** - `{hostname}` for system hostname
- [x] **Current Directory** - `{cwd}` for working directory path

### Git Provider
- [x] **Branch Name** - `{git_branch}` for current git branch
- [x] **Tracking Status** - `{git_tracking}` for ahead/behind status
  - [x] Ahead commits display (e.g., "ahead.2")
  - [x] Behind commits display (e.g., "behind.3")
  - [x] Both ahead and behind (e.g., "ahead.2.behind.1")
- [x] **Status Indicators**
  - [x] `{git_status_clean}` - Clean repo indicator (`:✔`)
  - [x] `{git_status_staged}` - Staged files count (`:+N`)
  - [x] `{git_status_unstaged}` - Modified/untracked files count (`:+N`)
- [x] **Elapsed Time** - `{git_elapsed}` time since last commit
  - [x] Seconds (`:Xs`)
  - [x] Minutes (`:Xm`)
  - [x] Hours (`:Xh`)
  - [x] Days (`:Xd`)

### IP Provider
- [x] **IP Address** - `{ip_address}` for network IP
  - [x] IPv4 support
  - [x] IPv6 support
  - [x] Auto-detection of primary interface
  - [x] Loopback filtering
- [x] **Interface Name** - `{ip_interface}` (e.g., "eth0", "wlan0")
- [x] **IP Version** - `{ip_version}` ("4" or "6")
- [x] **Configuration Options**
  - [x] Custom interface selection
  - [x] IPv6 preference toggle

### Battery Provider
- [x] **Battery Percentage** - `{battery_percentage}` (e.g., "85%")
- [x] **Battery Status** - `{battery_status}` (Charging, Discharging, Full, Empty)
- [x] **Power Draw** - Power consumption/charging in watts
  - [x] `{battery_power}` - Generic power (positive=charging, negative=discharging)
  - [x] `{battery_power_charging}` - Only set when charging (for conditional coloring)
  - [x] `{battery_power_discharging}` - Only set when discharging (for conditional coloring)
- [x] **Graceful Degradation** - Empty values on desktops without batteries
- [x] **Caching** - 30-second cache duration

## Shell Support

- [x] **Bash** - ANSI escape code wrapping with `\[...\]`
- [x] **Zsh** - ANSI escape code wrapping with `%{...%}`
- [x] **Tcsh** - ANSI escape code wrapping with `%{...%}`
  - [x] Newline handling (`\n` → `\\n`)
  - [x] Exclamation mark escaping (`!` → `\!` for history expansion)
  - [x] Percent sign escaping (`%` → `%%` for prompt substitutions)
  - [x] Edge case handling (space after `%}` before `\n`)
- [x] **Raw** - No wrapping (for testing/debugging)
- [x] **Auto Mode** - Shell detection via `--mode` flag

## CLI Features

- [x] **Config File Loading**
  - [x] Default location (`~/.config/twig/config.toml`)
  - [x] Custom config via `--config` flag
  - [x] Example config in repo (`config.toml`)
- [x] **Shell Mode Selection** - `--mode <shell>` flag
- [x] **Version Display** - `--version` flag
- [x] **Validation Mode** - `--validate` flag
  - [x] Template syntax validation
  - [x] Color/style validation
  - [x] Time format validation
  - [x] Provider error checking
- [x] **Timing Information** - Performance metrics display
  - [x] Per-provider timing
  - [x] Total render time
  - [x] Config load time

## Configuration

- [x] **Implicit Sections** - Auto-create config sections from template variables
- [x] **Default Configurations** - Provider-supplied defaults
- [x] **Custom Variable Names** - Override default names (e.g., `name = "mydir"`)
- [x] **TOML Format** - Standard TOML configuration syntax
- [x] **Comprehensive Documentation** - Inline config comments and examples

## Testing

- [x] **Unit Tests** - 49 passing tests
  - [x] Provider tests (battery, git, ip, builtin)
  - [x] Shell formatter tests (bash, tcsh, zsh, raw)
  - [x] Template parsing tests
  - [x] Conditional spacing tests
  - [x] Validation tests
  - [x] Special character escaping tests
- [x] **Integration Tests**
  - [x] End-to-end template rendering
  - [x] Tcsh exclamation mark escaping
  - [x] Tcsh prompt with conditional spacing

## Documentation

- [x] **Config File Documentation** - Comprehensive inline comments in `config.toml`
- [x] **Variable Reference** - All variables documented
- [x] **Usage Examples** - Example prompts for various use cases
- [x] **Feature Checklist** - This document

## Future Enhancements (Not Yet Implemented)

- [ ] **Daemon Mode** - Background process for caching slow providers
- [ ] **Custom Providers** - User-defined provider plugins
- [ ] **Conditional Rendering** - If/else logic in templates
- [ ] **Performance Optimization** - Selective provider execution
- [ ] **Additional Providers**
  - [ ] Kubernetes context
  - [ ] Docker status
  - [ ] Python virtualenv
  - [ ] Node.js version
  - [ ] Ruby version
- [ ] **Color Themes** - Predefined color schemes
- [ ] **Prompt Presets** - Common prompt configurations
- [ ] **Plugin System** - External provider loading

---

**Last Updated**: 2025-10-24
**Total Features Implemented**: 70+
**Test Coverage**: 49 passing tests
