# Conditional Styling and Display - Design Document

## Overview

This document explores design options for implementing conditional logic in twig prompts, enabling users to dynamically change colors, styles, and visibility based on variable values and state.

## Use Cases

### 1. Conditional Styling
Change color/style based on variable value:
- Different hostname colors per machine
- Git branch color based on name (main=green, dev=yellow)
- Directory color based on path depth or location

### 2. Conditional Display
Show/hide elements based on conditions:
- Only show git dirty count if > 0
- Only show battery if < 20%
- Show warning symbols for certain states

### 3. State-Based Formatting
Display different content based on provider state:
- Git: red count for unstaged, green for staged
- Exit code: red for errors, green for success
- Network: different icons for wifi/ethernet/offline

## Design Goals

1. **Easy to Learn**: Intuitive syntax that doesn't require programming knowledge
2. **Easy to Read**: Configuration should be self-documenting
3. **Scalable**: Support simple and complex use cases
4. **Maintainable**: Easy to modify and debug
5. **Grokkable**: Users can understand what's happening at a glance
6. **Performant**: Minimal overhead for evaluation

## Option 1: Value Mapping in Config

### Syntax
```toml
[hostname]
colors = { "skyfall" = "magenta", "localhost" = "green", "default" = "white" }

[git]
branch_colors = { "main" = "green", "master" = "green", "dev" = "yellow", "default" = "cyan" }
```

### Pros
✅ Very simple and readable
✅ No complex syntax to learn
✅ Easy to validate
✅ Self-documenting

### Cons
❌ Limited to exact value matching
❌ Can't handle numeric comparisons or ranges
❌ Verbose for many values
❌ Doesn't support conditional display (show/hide)

### Example Usage
```toml
[hostname]
colors = { "skyfall" = "magenta", "laptop" = "blue", "default" = "white" }

[prompt]
format = '{hostname} {cwd:green}'
# hostname automatically uses color from mapping
```

---

## Option 2: Inline Conditionals

### Syntax
```toml
[prompt]
format = '{hostname:magenta if $hostname=="skyfall" else green}'
format = '{git_dirty:red if $git_dirty>0 else hide}'
```

### Pros
✅ Powerful and flexible
✅ Everything in one place
✅ Supports comparisons and logic

### Cons
❌ Complex to parse and validate
❌ Can become unreadable for complex logic
❌ Security concerns with expression evaluation
❌ Hard to debug when errors occur

### Example Usage
```toml
[prompt]
format = '''
{hostname:magenta if $hostname=="skyfall" else green}
{git:yellow} {git_dirty:red if $git_dirty>0}
{battery:red if $battery<20 else green if $battery<50 else white}
'''
```

---

## Option 3: Template Functions

### Syntax
```toml
[prompt]
format = '{hostname:color_map($hostname)} {git_dirty:show_if($git_dirty>0,red)}'

[functions]
color_map = { "skyfall" = "magenta", "default" = "white" }
```

### Pros
✅ Clean separation of logic and template
✅ Reusable functions
✅ Can be extended over time

### Cons
❌ Adds complexity with function concept
❌ Requires learning function syntax
❌ May be overkill for simple cases

---

## Option 4: Provider Multi-Variables + Simple Display Rules

### Concept
Providers return multiple granular variables, config uses simple display rules.

### Syntax
```toml
[git]
# Provider returns: git_branch, git_dirty_count, git_staged_count, git_clean

[prompt]
format = '{git_branch:yellow} {git_dirty_count:red?} {git_staged_count:green?}'
# The ? suffix means "only show if > 0"
```

### Provider Changes
```rust
// GitProvider returns multiple variables
vars.insert("git_branch", "main");
vars.insert("git_dirty_count", "3");    // Number of dirty files
vars.insert("git_staged_count", "2");   // Number of staged files
vars.insert("git_clean", "true");       // true if clean, false otherwise
```

### Display Rules
- `?` suffix: only show if truthy (non-zero, non-empty)
- `!` suffix: only show if falsy (zero, empty)
- Multiple variables let users compose their own logic

### Pros
✅ Simple and readable
✅ No complex conditional syntax needed
✅ Providers handle the complexity
✅ Easy to understand at a glance
✅ Highly composable

### Cons
❌ Limited to simple show/hide logic
❌ Can't do complex color switching inline
❌ Providers need to be designed carefully

### Example Usage
```toml
[prompt]
# Only show counts if non-zero
format = '{git_branch:yellow} {git_dirty_count:red?}✗ {git_staged_count:green?}✓'

# Result when dirty_count=3, staged_count=0:
# main 3✗

# Result when both zero:
# main
```

---

## Option 5: Hybrid Approach (RECOMMENDED)

Combine simple value mapping with provider multi-variables and display rules.

### Phase 1: Simple Value Mapping
For basic styling without conditions:

```toml
[hostname]
style = { "skyfall" = "magenta", "laptop" = "blue", "default" = "white" }

[git.branch_style]
"main" = "green,bold"
"master" = "green,bold"
"dev" = "yellow"
default = "cyan"

[prompt]
format = '{hostname} {git_branch} {cwd:green}'
# hostname and git_branch automatically use their style mappings
```

### Phase 2: Multi-Variables + Display Rules
For conditional display:

```toml
[git]
# Provider returns: git_branch, git_dirty_count, git_staged_count, git_ahead, git_behind

[prompt]
format = '{git_branch:yellow} {git_dirty_count:red?}✗ {git_staged_count:green?}✓ {git_ahead:green?}↑ {git_behind:red?}↓'
```

**Display Rules:**
- `{var:color?}` - Only show if var is truthy (non-zero, non-empty)
- `{var:color!}` - Only show if var is falsy (zero, empty)
- `{var:color}` - Always show

### Phase 3 (Future): Conditional Expressions
For advanced use cases:

```toml
[prompt]
format = '{git_branch:match($git_branch, "main" => green, "dev" => yellow, _ => cyan)}'
```

### Benefits
✅ **Progressive complexity**: Simple cases are simple, complex cases are possible
✅ **Grokkable**: Each phase is easy to understand
✅ **Scalable**: Can add features without breaking existing configs
✅ **Maintainable**: Clear separation of concerns

---

## Detailed Examples

### Example 1: Hostname-Based Styling
```toml
[hostname]
style = {
    "skyfall" = "magenta,bold",
    "laptop" = "blue",
    "server-prod" = "red,bold",
    "server-dev" = "yellow",
    "default" = "white"
}

[prompt]
format = '{hostname} {cwd:green}'
```

### Example 2: Git Status with Counts
```toml
[git]
# Provider returns rich git state

[prompt]
format = '''
{git_branch:yellow} {git_dirty_count:red?}✗ {git_staged_count:green?}✓
{cwd:green}
{"$":white,bold}
'''

# Examples:
# Clean repo:     "main /home/user $"
# 3 dirty files:  "main 3✗ /home/user $"
# 2 staged:       "main 2✓ /home/user $"
# Both:           "main 3✗ 2✓ /home/user $"
```

### Example 3: Battery Warning
```toml
[battery]
# Provider returns: battery_percent, battery_low, battery_critical

[prompt]
format = '{time:cyan} {battery_critical:red,bold?}🔋! {hostname} {cwd:green}'
# 🔋! only shows if battery_critical is true
```

### Example 4: Git Branch with Style Mapping
```toml
[git.branch_style]
"main" = "green,bold"
"master" = "green,bold"
"develop" = "yellow"
"staging" = "blue"
default = "cyan"

[git]
# Provider returns: git_branch, git_dirty

[prompt]
format = '{git_branch} {git_dirty:red?}✗ {cwd:green}'
# git_branch color comes from branch_style mapping
# git_dirty only shows if true
```

---

## Implementation Phases

### Phase 1: Value Mapping (Simple Cases)
**Effort**: Medium
**Impact**: High
**Use Cases**: Hostname colors, branch colors

1. Add `style` or `colors` field to config sections
2. Modify providers to check for style mappings
3. Apply mapped style when rendering
4. Fallback to inline style or default

### Phase 2: Multi-Variables (Provider-Side Logic)
**Effort**: Medium per provider
**Impact**: High
**Use Cases**: Git counts, battery state, exit codes

1. Enhance providers to return multiple granular variables
2. Implement `?` and `!` display rules in template parser
3. Update documentation with new variables

### Phase 3: Display Rules (Template-Side Logic)
**Effort**: Medium
**Impact**: Medium
**Use Cases**: Show/hide based on conditions

1. Extend template syntax for `?` and `!` suffixes
2. Evaluate conditions during template substitution
3. Handle empty strings gracefully

### Phase 4: Advanced Conditionals (Future)
**Effort**: High
**Impact**: Medium
**Use Cases**: Complex logic, power users

1. Design safe expression syntax
2. Implement expression evaluator
3. Add validation and error handling
4. Security review

---

## Recommended Implementation Order

### 🚀 Phase 1: Value Mapping (Start Here)
**Why first**: Solves 80% of use cases with minimal complexity

**Changes needed**:
1. Add `style` field to config sections (HashMap<String, String>)
2. Modify `substitute_variables()` to check for style mappings
3. Lookup: `config.hostname.style.get(hostname_value).unwrap_or(inline_style)`

**Example PR**:
```rust
// In config.rs
#[derive(Debug, Deserialize)]
pub struct HostnameConfig {
    pub name: Option<String>,
    pub style: Option<HashMap<String, String>>, // value -> style mapping
}

// In main.rs template substitution
fn apply_style(var_name: &str, value: &str, inline_style: &str, config: &Config) -> String {
    // Check for style mapping first
    if let Some(mapped_style) = lookup_style_mapping(var_name, value, config) {
        colorize(value, mapped_style, formatter)
    } else {
        colorize(value, inline_style, formatter)
    }
}
```

### 🎯 Phase 2: Multi-Variables + Display Rules
**Why second**: Enables powerful compositions without complex syntax

**Changes needed**:
1. Enhance GitProvider to return `git_dirty_count`, `git_staged_count`, etc.
2. Add `?` suffix handling in template parser
3. Skip rendering if condition fails

### 🔮 Phase 3+: Advanced Features (Later)
Only implement if users request more complex logic.

---

## Design Decisions Summary

### ✅ DO
- Start with simple value mapping
- Use provider multi-variables for state
- Support `?` and `!` for simple show/hide
- Make default behavior intuitive
- Provide clear examples in docs

### ❌ DON'T
- Add complex expression language initially
- Implement features users don't need
- Sacrifice readability for power
- Create security vulnerabilities with eval()
- Break existing configs

---

## User Experience Examples

### Beginner User
```toml
# I just want different hostname colors
[hostname]
style = { "laptop" = "blue", "desktop" = "green" }

[prompt]
format = '{hostname} {cwd:green} {"$":white,bold}'
```
**Grokkability**: ⭐⭐⭐⭐⭐ (Extremely clear)

### Intermediate User
```toml
# I want git status with counts
[prompt]
format = '{git_branch:yellow} {git_dirty_count:red?}✗ {cwd:green}'
```
**Grokkability**: ⭐⭐⭐⭐ (Clear with ? suffix)

### Advanced User (Future)
```toml
# Complex conditional logic
[prompt]
format = '{git_branch:match($branch, "main"=>green, _=>yellow)} {cwd:green}'
```
**Grokkability**: ⭐⭐⭐ (Requires learning match syntax)

---

## Open Questions

1. **Nested conditions**: How to handle "if battery < 20, red, else if < 50, yellow, else green"?
   - **Answer**: Use value mapping with ranges in Phase 1, or provider-side logic

2. **Performance**: How expensive are condition evaluations?
   - **Answer**: Value lookups are O(1), minimal overhead

3. **Validation**: How to validate conditions at config load time?
   - **Answer**: Phase 1 is fully validatable, Phase 2+ needs runtime checks

4. **Error handling**: What happens when condition evaluation fails?
   - **Answer**: Graceful degradation - show variable without styling

---

## Recommendation

**Start with Phase 1 (Value Mapping)** - it's simple, powerful, and solves most real-world use cases:

1. Implement `style` field for all config sections
2. Add style mapping lookup in template substitution
3. Document with clear examples
4. Ship it ✅

Then wait for user feedback before implementing Phase 2+. Don't build features users don't need.

**Key Principle**: **Make simple things simple, and complex things possible.**

---

## ✅ IMPLEMENTED: Conditional Spacing with `~`

### Status

The tilde (`~`) character acts as a **conditional space** that only appears when the adjacent variable has a value. This solves the common problem of unwanted trailing spaces when optional variables are empty.

### How It Works

- `~` before a variable creates a conditional space
- The space **only appears** if the following variable has a non-empty value
- If the variable is empty or missing, the `~` disappears entirely
- Use `\~` to include a literal tilde character

**Syntax:**

```toml
{var1}~{var2}  # Space only appears if var2 exists
\~             # Literal tilde (escaped)
```

### Examples

#### Basic Usage

```toml
[prompt]
format = '{cwd}~{git_branch}'

# With git:     /home/user main     ✅ One space
# Without git:  /home/user          ✅ No trailing space
```

#### Multiple Conditional Spaces

```toml
[prompt]
format = '{hostname}~{git_branch}~{cwd}'

# All present:    laptop main /home/user      ✅ Two spaces
# No git:         laptop /home/user           ✅ One space
# Only cwd:       /home/user                  ✅ No extra spaces
```

#### Your Exact Use Case (Parenthesized Prompt)

```toml
[prompt]
format = '-({time} {hostname} {cwd}~{git_branch})-'

# With git branch:
-(10:38:34 jeffjose2.mtv.corp.google.com /usr/local/google/home/jeffjose/scripts/twig main)-

# Without git branch:
-(10:38:02 jeffjose2.mtv.corp.google.com /usr/local/google/home/jeffjose/scripts)-
                                                                                   ↑
                                          No extra space before the )! Perfect alignment.
```

#### With Colors

```toml
[prompt]
format = '{cwd:green}~{git_branch:yellow}~{git_dirty:red}'

# The conditional space works seamlessly with colored variables
```

#### Literal Tilde

```toml
[prompt]
format = '{cwd}\~{git_branch}'

# Always renders: /home/user~/main
#                          ↑ literal tilde, not conditional space
```

### Comparison with Regular Spaces

```toml
# Regular space (always present):
format = '{cwd} {git_branch}'
# Result without git: "/home/user "  ❌ trailing space

# Conditional space (appears only if needed):
format = '{cwd}~{git_branch}'
# Result without git: "/home/user"   ✅ no trailing space
```

### Advanced Usage

#### Combining with Literals

```toml
[prompt]
format = '{\">>\":white}~{git_branch:yellow}~{cwd:green}'

# With git:     >> main /home/user
# Without git:  >> /home/user
```

#### Environment Variables

```toml
[prompt]
format = '{hostname}~{$VIRTUAL_ENV}~{cwd}'

# The ~ works with environment variables too!
# Space only appears if $VIRTUAL_ENV is set and non-empty
```

### Why This Works Well

✅ **Self-documenting**: You can see exactly where conditional spacing happens
✅ **No surprises**: Behavior is explicit, not hidden magic
✅ **Familiar**: Escape syntax (`\~`) is well-understood
✅ **Minimal**: One character, one purpose
✅ **Composable**: Works with colors, styles, literals, and environment variables

### Implementation Details

The `~` is processed **before** variable substitution, so it works at the template level:

1. Template is scanned for `~` characters
2. For each `~`, the following variable is identified
3. If the variable has a value, `~` becomes a space
4. If the variable is empty/missing, `~` disappears
5. Then normal variable substitution happens with colors/styles

This means `~` is completely transparent to the rest of the template system.
