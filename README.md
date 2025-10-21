# twig

Fast shell prompt generator with daemon caching and multi-shell support.

## Features

- ANSI color and style support
- Template-based configuration (TOML)
- Daemon caching for expensive operations
- Multi-shell output (TCSH, Bash, Zsh)
- Implicit section creation
- Environment variable expansion

## Installation

```bash
cargo build --release
cp target/release/twig ~/.local/bin/
cp target/release/twigd ~/.local/bin/
```

## Usage

### Development Mode
```bash
twig                    # Boxed output with timing
```

### Shell Integration
```bash
twig --prompt           # Raw ANSI codes
twig --mode tcsh        # TCSH-specific format
twig --mode bash        # Bash-specific format
twig --mode zsh         # Zsh-specific format
```

### Debugging
```bash
twig --mode tcsh --debug    # Show timing and config paths
TWIG_DEBUG=1 twig --mode tcsh  # Or use environment variable
```

## Configuration

Default location: `~/.config/twig/config.toml`

```toml
[prompt]
format = '''{time:cyan} {hostname:magenta} {cwd:green}
{"$":white,bold} '''

[time]
format = "%H:%M:%S"
```

### Template Syntax

```
{variable}              # Plain variable
{variable:color}        # Colored variable
{variable:color,bold}   # Colored and styled
{"text":color}          # Literal colored text
{$ENV_VAR}              # Environment variable
```

### Implicit Sections

Sections are created automatically based on template variables.

```toml
# Using {time} creates [time] section implicitly
# Using {hostname} creates [hostname] section implicitly
# Using {cwd} creates [cwd] section implicitly
```

Override variable names:

```toml
[cwd]
name = "dir"  # Use {dir} instead of {cwd}
```

## Shell Setup

### TCSH
```tcsh
# In ~/.tcshrc
set prompt="`twig --mode tcsh`"
```

### Bash
```bash
# In ~/.bashrc
PS1=$(twig --mode bash)
```

### Zsh
```zsh
# In ~/.zshrc
PROMPT=$(twig --mode zsh)
```

## Daemon

Start the caching daemon for faster prompts:

```bash
twigd &  # Runs in background, caches hostname
```

Cache location: `~/.local/share/twig/data.json`

## Colors

Basic: black, red, green, yellow, blue, magenta, cyan, white
Bright: bright_red, bright_green, bright_blue, etc.
Styles: bold, italic, underline

## Architecture

```
twig/       CLI binary - generates prompts
twigd/      Daemon binary - caches expensive operations
docs/       Technical documentation
reference/  Example configurations
```

## Documentation

- `docs/FEATURES.md` - Complete feature list
- `docs/FEATURES-CHECKLIST.md` - Development roadmap
- `docs/TCSH.md` - TCSH implementation plan
- `docs/REFERENCE-mode-tcsh.md` - TCSH technical reference

## License

MIT
