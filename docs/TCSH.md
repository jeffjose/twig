# TCSH Shell Support - Implementation Plan

## Overview

TCSH (and CSH) require special handling of ANSI escape codes for prompts to work correctly. Without proper wrapping, the shell miscalculates prompt width, causing line editing issues.

## The Problem

**Current output** (standard ANSI codes):
```
\x1b[36m22:42:12\x1b[0m @ \x1b[35mnomad\x1b[0m
```

**TCSH needs** (wrapped in `%{...%}`):
```
%{\x1b[36m%}22:42:12%{\x1b[0m%} @ %{\x1b[35m%}nomad%{\x1b[0m%}
```

The `%{...%}` wrapper tells TCSH that the enclosed characters don't consume visible space.

**Bash needs** (wrapped in `\[...\]`):
```
\[\x1b[36m\]22:42:12\[\x1b[0m\] @ \[\x1b[35m\]nomad\[\x1b[0m\]
```

The `\[...\]` wrapper tells Bash that the enclosed characters are non-printing.

**Zsh needs** (wrapped in `%{...%}`):
```
%{\x1b[36m%}22:42:12%{\x1b[0m%} @ %{\x1b[35m%}nomad%{\x1b[0m%}
```

Zsh uses the same syntax as TCSH.

## Requirements

### Current Behavior (Already Implemented)
1. `twig` - Boxed output with timing (for debugging/testing)
2. `twig --prompt` - Output prompt only with standard ANSI codes (for shell integration)

### New Behavior (To Implement)
3. `twig --mode tcsh` - Output prompt in TCSH format with `%{...%}` wrapping
4. `twig --mode bash` - Output prompt in Bash format with `\[...\]` wrapping
5. `twig --mode zsh` - Output prompt in Zsh format with `%{...%}` wrapping

### Flag Behavior
- `twig` (no flags) - Shows boxed output (debugging)
- `twig --prompt` - Outputs raw ANSI codes (no wrapping, no box)
- `twig --mode <shell>` - Outputs shell-specific format (no box)
- The `--mode` and `--prompt` flags are mutually exclusive (user picks one)

## Architecture

### File Structure
```
twig/src/
├── main.rs              # CLI parsing, main logic
├── shell/               # Shell output formatters (new directory)
│   ├── mod.rs           # Module definition and ShellFormatter trait
│   ├── raw.rs           # Raw ANSI codes (for --prompt, no wrapping)
│   ├── bash.rs          # Bash formatter with \[...\] wrapping
│   ├── zsh.rs           # Zsh formatter with %{...%} wrapping
│   └── tcsh.rs          # TCSH formatter with %{...%} wrapping
```

### Core Abstraction

```rust
// shell/mod.rs
pub trait ShellFormatter {
    /// Wrap ANSI escape codes for the specific shell
    fn format_ansi(&self, ansi_code: &str, text: &str, reset_code: &str) -> String;
}

pub enum ShellMode {
    Raw,    // Raw ANSI codes (for --prompt)
    Bash,   // Bash with \[...\] wrapping
    Zsh,    // Zsh with %{...%} wrapping
    Tcsh,   // TCSH with %{...%} wrapping
}

pub fn get_formatter(mode: ShellMode) -> Box<dyn ShellFormatter> {
    match mode {
        ShellMode::Raw => Box::new(RawFormatter),
        ShellMode::Bash => Box::new(BashFormatter),
        ShellMode::Zsh => Box::new(ZshFormatter),
        ShellMode::Tcsh => Box::new(TcshFormatter),
    }
}
```

### Raw Formatter (for --prompt)

```rust
// shell/raw.rs
pub struct RawFormatter;

impl ShellFormatter for RawFormatter {
    fn format_ansi(&self, ansi_code: &str, text: &str, reset_code: &str) -> String {
        // No wrapping, just raw ANSI codes
        format!("{}{}{}", ansi_code, text, reset_code)
    }
}
```

### Bash Formatter

```rust
// shell/bash.rs
pub struct BashFormatter;

impl ShellFormatter for BashFormatter {
    fn format_ansi(&self, ansi_code: &str, text: &str, reset_code: &str) -> String {
        // Wrap ANSI codes in \[...\]
        format!("\\[{}\\]{}\\[{}\\]", ansi_code, text, reset_code)
    }
}
```

### Zsh Formatter

```rust
// shell/zsh.rs
pub struct ZshFormatter;

impl ShellFormatter for ZshFormatter {
    fn format_ansi(&self, ansi_code: &str, text: &str, reset_code: &str) -> String {
        // Wrap ANSI codes in %{...%}
        format!("%{{{}%}}{}%{{{}%}}", ansi_code, text, reset_code)
    }
}
```

### TCSH Formatter

```rust
// shell/tcsh.rs
pub struct TcshFormatter;

impl ShellFormatter for TcshFormatter {
    fn format_ansi(&self, ansi_code: &str, text: &str, reset_code: &str) -> String {
        // Wrap ANSI codes in %{...%}
        format!("%{{{}%}}{}%{{{}%}}", ansi_code, text, reset_code)
    }
}
```

Note: Zsh and TCSH use identical wrapping syntax, but we keep them separate for clarity and potential future differences.

## Implementation Steps

### 1. CLI Flag Addition
```rust
#[derive(Parser)]
struct Cli {
    /// Output only the prompt (for shell integration)
    #[arg(long)]
    prompt: bool,

    /// Path to config file (default: ~/.config/twig/config.toml)
    #[arg(long)]
    config: Option<PathBuf>,

    /// Shell output mode (tcsh, bash, zsh)
    #[arg(long, value_name = "SHELL")]
    mode: Option<String>,
}
```

### 2. Shell Module Creation

Create `twig/src/shell/` directory with:
- `mod.rs` - Trait definition and factory
- `ansi.rs` - Default formatter
- `tcsh.rs` - TCSH formatter

### 3. Modify `colorize()` Function

Current signature:
```rust
fn colorize(text: &str, style_spec: &str) -> String
```

New signature:
```rust
fn colorize(text: &str, style_spec: &str, formatter: &dyn ShellFormatter) -> String
```

Modify to use `formatter.format_ansi()` instead of hardcoded format.

### 4. Update Call Chain

Pass formatter through:
1. `main()` creates formatter based on CLI flag
2. `substitute_variables()` accepts formatter parameter
3. `handle_literal()` and `handle_variable()` accept formatter
4. `colorize()` uses formatter

### 5. Output Logic

```rust
fn main() {
    let cli = Cli::parse();

    // ... config loading, variable collection ...

    // Determine shell mode and output format
    let (shell_mode, show_box) = if let Some(mode) = &cli.mode {
        // --mode flag: use specified shell formatter, no box
        let mode = match mode.as_str() {
            "tcsh" => ShellMode::Tcsh,
            "bash" => ShellMode::Bash,
            "zsh" => ShellMode::Zsh,
            other => {
                eprintln!("Unknown shell mode: {}. Valid options: tcsh, bash, zsh", other);
                std::process::exit(1);
            }
        };
        (mode, false)
    } else if cli.prompt {
        // --prompt flag: raw ANSI codes, no box
        (ShellMode::Raw, false)
    } else {
        // Default: raw ANSI codes, show box
        (ShellMode::Raw, true)
    };

    // Create formatter
    let formatter = get_formatter(shell_mode);

    // Perform variable substitution with formatter
    let output = substitute_variables(&config.prompt.format, &variables, formatter.as_ref());

    // Output based on show_box flag
    if show_box {
        // Default: boxed output with timing
        print_boxed(&output, &config_path, &cache_status, config_time, render_time, total_time);
    } else {
        // --prompt or --mode: just the prompt
        print!("{}", output);
    }
}
```

## Testing Strategy

### Manual Testing

1. **Default mode (no flags)**
   ```bash
   ./target/debug/twig
   # Should show boxed output
   ```

2. **Prompt mode**
   ```bash
   ./target/debug/twig --prompt
   # Should output: \x1b[36m22:42:12\x1b[0m ...
   ```

3. **TCSH mode**
   ```bash
   ./target/debug/twig --mode tcsh
   # Should output: %{\x1b[36m%}22:42:12%{\x1b[0m%} ...
   ```

4. **Bash mode (explicit)**
   ```bash
   ./target/debug/twig --mode bash
   # Should output: \x1b[36m22:42:12\x1b[0m ...
   ```

### Integration Testing

Create test configs and verify output format:
```bash
# Test TCSH with colors
./target/debug/twig --mode tcsh --config test-tcsh.toml
```

### TCSH Shell Testing

Set up in `~/.tcshrc`:
```tcsh
# In your ~/.tcshrc
set prompt="`/path/to/twig --mode tcsh`"
```

Test line editing:
- Long commands that wrap
- History navigation (up/down arrows)
- Tab completion
- Ctrl+A (beginning of line)
- Ctrl+E (end of line)

## Edge Cases

### Multiline Prompts
TCSH handles multiline prompts differently. For Phase 1, we'll focus on single-line prompts only.

### No Colors
If no colors are used, both formatters should produce identical output (just the text).

### Literal Text
Literal colored text like `{"@":yellow}` must also be wrapped correctly in TCSH.

## Future Enhancements

### Phase 2: Additional Shell Support
- **Fish** - May need special handling (likely same as bash)
- **PowerShell** - Different escape sequences
- **Windows CMD** - Different color system

### Phase 3: Auto-Detection
```rust
// Detect from $SHELL environment variable
let shell_mode = if cli.mode.is_some() {
    // Explicit mode takes priority
    parse_mode(cli.mode.unwrap())
} else if let Ok(shell) = std::env::var("SHELL") {
    // Auto-detect from $SHELL
    if shell.contains("tcsh") || shell.contains("csh") {
        ShellMode::Tcsh
    } else {
        ShellMode::Ansi
    }
} else {
    ShellMode::Ansi
};
```

### Phase 4: Multiline Support
Handle TCSH multiline prompts with proper wrapping on each line.

## Success Criteria

1. ✅ `twig` produces boxed output (unchanged)
2. ✅ `twig --prompt` produces bash-compatible prompt (unchanged)
3. ✅ `twig --mode tcsh` produces TCSH-compatible prompt with `%{...%}` wrapping
4. ✅ TCSH line editing works correctly with colored prompts
5. ✅ Architecture allows easy addition of new shell formatters
6. ✅ Code is clean and maintainable with separate modules per shell

## Implementation Checklist

- [ ] Create `twig/src/shell/` directory
- [ ] Create `shell/mod.rs` with trait and factory
- [ ] Create `shell/ansi.rs` with AnsiFormatter
- [ ] Create `shell/tcsh.rs` with TcshFormatter
- [ ] Add `--mode` flag to CLI struct
- [ ] Modify `colorize()` to accept formatter parameter
- [ ] Update call chain to pass formatter through
- [ ] Update main() output logic for mode handling
- [ ] Test with bash/zsh (should be unchanged)
- [ ] Test with tcsh (verify %{...%} wrapping)
- [ ] Test line editing in actual tcsh shell
- [ ] Update example config with tcsh usage
- [ ] Document in README or shell integration guide
