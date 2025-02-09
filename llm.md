# Twig Codebase Architecture Summary

## Overall Architecture
- Command-line tool for displaying customizable system information (time, hostname, IP, git status, etc.)
- Built in Rust using a modular, trait-based architecture
- Uses async/await for parallel processing of different information providers
- Configuration driven using TOML files
- Supports colored output and different output modes (e.g., tcsh)

## Core Design Patterns
1. Provider Pattern
   - Each information type implements the `VariableProvider` trait (src/variable.rs:2)
   - Key providers:
     * `TimeProvider` (src/time.rs:24)
     * `GitProvider` (src/git.rs:52)
     * `HostnameProvider` (src/hostname.rs:46)
     * `IpProvider` (src/ip.rs:46)
     * `CwdProvider` (src/cwd.rs:41)
     * `EnvProvider` (src/env_var.rs:35)

2. Trait-based Architecture
   - Core traits:
     * `VariableProvider` (src/variable.rs:4-9): Base trait for all providers
     * `ConfigWithName` (src/variable.rs:11-14): Configuration trait
     * `LazyVariables` (src/variable.rs:16-45): Lazy evaluation trait

3. Error Handling
   - Custom error types per module:
     * `GitError` (src/git.rs:7-11)
     * `CwdError` (src/cwd.rs:8-12)
     * `IpConfigError` (src/ip.rs:6-10)
     * `TimeError` (src/time.rs:28-32)
     * `EnvError` (src/env_var.rs:6-10)

4. Configuration Management
   - Config structs:
     * `Config` (src/main.rs:114-126): Main configuration
     * `TimeConfig` (src/time.rs:5-11)
     * `GitConfig` (src/git.rs:23-29)
     * `CwdConfig` (src/cwd.rs:24-30)
     * `IpConfig` (src/ip.rs:19-25)

5. Template System
   - Core template functionality in `template.rs`:
     * `format_template()` (src/template.rs:243-297): Main formatting function
     * `apply_color()` (src/template.rs:18-82): Color handling
     * `process_variables()` (src/template.rs:99-152): Variable processing

## Module Structure

1. Main Module (src/main.rs)
   - `Cli` struct (lines 38-49): Command line interface
   - `Config` struct (lines 114-126): Application configuration
   - `main()` function (lines 256-442): Entry point

2. Variable Module (src/variable.rs)
   - Core traits (lines 4-45)
   - Processing functions (lines 92-152)
   - Helper functions (lines 154-186)

3. Provider Modules
   a. Time (src/time.rs)
      - `TimeProvider` struct and impl (lines 24-65)
      - `TimeError` handling (lines 28-41)
      - Time formatting functions (lines 89-96)

   b. Git (src/git.rs)
      - `GitProvider` struct and impl (lines 52-89)
      - Git command execution (lines 115-137)
      - Status functions (lines 139-258)

   c. Hostname (src/hostname.rs)
      - `HostnameProvider` struct and impl (lines 46-82)
      - FQDN handling (lines 84-96)

   d. IP (src/ip.rs)
      - `IpProvider` struct and impl (lines 46-82)
      - IP address detection (lines 84-96)

   e. CWD (src/cwd.rs)
      - `CwdProvider` struct and impl (lines 41-77)
      - Path manipulation functions (lines 100-116)

   f. Environment Variables (src/env_var.rs)
      - `EnvProvider` struct and impl (lines 35-63)
      - Variable access functions (lines 65-82)

4. Template Module (src/template.rs)
   - Template processing (lines 243-297)
   - Color handling (lines 18-82)
   - Variable parsing (lines 299-312)

## Testing Structure
- Comprehensive unit tests for each module
- Test files parallel main implementation files
- Mock implementations for system calls
- Error case testing
- Format validation testing

## Key Features

1. Configuration
   - TOML-based configuration
   - Multiple sections per provider
   - Default values
   - Error handling options

2. Variable Processing
   - Lazy evaluation
   - Parallel processing
   - Color support
   - Format string parsing

3. Git Integration
   - Branch detection
   - Status indicators
   - Stash counting
   - Remote tracking

4. Path Handling
   - Home directory expansion
   - Path shortening
   - Parent directory access

5. Performance Features
   - Async processing
   - Lazy evaluation
   - Parallel execution
   - Performance timing

## Error Handling Strategy
- Custom error types per module
- Error propagation
- User-friendly error messages
- Optional validation mode
- Warning system for non-critical issues

## Performance Optimizations
- Parallel processing of providers
- Lazy variable evaluation
- String capacity pre-allocation
- Efficient template processing
- Skip unused variables

## Testing Strategy
- Unit tests per module
- Integration tests
- Error case coverage
- Format validation
- System interaction testing

## Code Organization
- Modular file structure
- Consistent naming conventions
- Clear separation of concerns
- Parallel test files
- Common traits and interfaces

## Future Extensibility
- Trait-based design allows easy addition of new providers
- Template system supports custom formatting
- Configurable error handling
- Modular architecture for new features
- Output mode extensibility

## Dependencies
- clap: Command line argument parsing
- serde: Serialization/deserialization
- tokio: Async runtime
- colored: Terminal color support
- chrono: Time handling
- local-ip-address: Network information
- directories: Config file locations

## Build and Development
- Cargo-based build system
- Development-time validation mode
- Performance timing options
- Warning system for debugging
- Comprehensive test suite

This architecture provides a robust, maintainable, and extensible system for displaying system information in a customizable format. 
