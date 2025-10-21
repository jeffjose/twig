# Git Prompt Design Documentation

## Overview

The twig git prompt provides comprehensive git repository status in a compact, color-coded format that enables instant visual scanning. It displays local working tree status, remote tracking information, and time context all in one concise indicator.

## Format Specification

```
branch[(ahead|behind).N]:status:elapsed_time
```

### Components

| Component | Format | Required | Description |
|-----------|--------|----------|-------------|
| `branch` | `main`, `master`, `dev`, etc. | ✅ Yes | Current branch name |
| `(ahead\|behind).N` | `(ahead.3)`, `(behind.2)` | ❌ Optional | Remote tracking status (only shown if out of sync) |
| `:status` | `:✔`, `:+1`, `:+2`, `:+1+1` | ✅ Yes | Working tree status |
| `:elapsed_time` | `:2s`, `:5m`, `:17h` | ✅ Yes | Time since last git state change |

## Color Scheme

### Semantic Color Mapping

The color scheme maps **problem categories to colors** and **problem specifics to text**, enabling instant pattern recognition:

| Element | Color | Hex/ANSI | Semantic Meaning |
|---------|-------|----------|------------------|
| **Branch name** | Yellow | `yellow` | Current branch identifier |
| **Remote tracking** | Magenta | `magenta` | 🚨 Remote sync needed |
| **Clean status (✔)** | Green | `green` | ✅ All committed, working tree clean |
| **Dirty status (+N)** | Yellow/Orange | `yellow` | ⚠️ Uncommitted changes need attention |
| **Elapsed time** | Dim/Gray | `dim` | Secondary contextual info |

### Three-State Status System

```
🟢 Green ✔           = Perfect (local clean + remote synced)
🟡 Yellow +N         = Local work (uncommitted changes)
🟣 Magenta (±N)      = Remote sync (push or pull needed)
```

## Status Indicators

### Working Tree Status

| Indicator | Meaning | Example Scenario |
|-----------|---------|------------------|
| `✔` | Clean working tree | All changes committed, no untracked files |
| `+N` | N files need attention | Could be untracked OR staged files |
| `+N+M` | N staged, M untracked | Mixed state: some files staged, others untracked |

**Note:** The `+N` indicator is context-dependent:
- After creating files → untracked files
- After `git add` → staged files
- The context is usually clear from your recent actions

### Remote Tracking Status

| Indicator | Meaning | Action Needed |
|-----------|---------|---------------|
| _(none)_ | Up to date with remote | No action needed |
| `(ahead.N)` | N commits ahead of remote | `git push` to sync |
| `(behind.N)` | N commits behind remote | `git pull` to sync |

**Design Decision:** Both `ahead` and `behind` use the same **magenta** color because:
- Both represent "out of sync with remote" (same problem category)
- The text itself (`ahead` vs `behind`) indicates direction
- Reduces cognitive load (one color = one issue type)
- Both require similar action category (push/pull)

### Elapsed Time

Shows time since last git state change in human-readable format:
- `2s`, `8s` = seconds
- `5m`, `17m` = minutes
- `6h`, `17h` = hours

**Purpose:** Provides context for how stale the current state is.

## Real-World Examples

### Example 1: Clean Repository Workflow

```bash
# Initial clean state
main:✔:2s

# Create a file
$ touch bar
main:+1:8s              # 1 untracked file

# Create another file
$ touch baz
main:+2:12s             # 2 untracked files

# Stage one file
$ git add bar
main:+1+1:17s           # 1 staged, 1 untracked

# Commit staged file
$ git commit -m "added bar"
main:+1:0s              # 1 untracked remains (baz)

# Stage remaining file
$ git add .
main:+1:6s              # 1 staged (baz)

# Commit everything
$ git commit -m "adding rest"
main:✔:0s               # Clean again!
```

### Example 2: Remote Sync Workflow

```bash
# Behind remote
master(behind.3):✔:17h   # 3 commits behind, but working tree is clean

# Pull from remote
$ git pull
master.✔:6h              # Synced! No tracking indicator shown
```

### Example 3: Visual Scanning Patterns

```bash
main.✔:6h                # 🟢 All green? Relax!

main(behind.3):✔:17h     # 🟣 Magenta? Check remote!

main:+2:12s              # 🟡 Yellow? Commit your work!

main(ahead.1):+1:5m      # 🟣🟡 Both? Sync + commit!
```

## Visual Design Benefits

### 1. Scannable at a Glance

The color-coded design enables instant status recognition without reading:
- **Green** anywhere = good
- **Yellow** status = local work pending
- **Magenta** tracking = remote action needed

### 2. Compact Yet Comprehensive

Packs multiple git commands' worth of info into one line:
- `git status` → working tree status (✔, +N)
- `git status -sb` → remote tracking (ahead/behind)
- Time context → custom enhancement

### 3. Visual Hierarchy

**Bright colors** (cyan, yellow, green, magenta) = important, actionable info
**Dim colors** (gray) = contextual, secondary info

The elapsed time is deliberately dimmed because it's informative but not actionable.

### 4. Cognitive Load Reduction

**Color = Problem Category**
**Text = Problem Specifics**

This separation means:
- Fast scan: Just look at colors
- Deep read: Read text for details

## Design Philosophy

### Principles

1. **Maximum information, minimum space**
   Every character earns its place in the prompt

2. **Color with purpose**
   Colors convey semantic meaning, not just aesthetics

3. **Progressive disclosure**
   Important info is bright, contextual info is dim

4. **Action-oriented**
   Status indicators map to clear actions:
   - Green ✔ → Keep working
   - Yellow +N → Commit your changes
   - Magenta (behind) → Pull from remote
   - Magenta (ahead) → Push to remote

5. **Fail-safe defaults**
   Missing indicators mean "all good" (no news is good news)

## Integration with Conditional Spacing

The git prompt integrates perfectly with twig's `~` conditional spacing feature:

```toml
[prompt]
format = '{cwd:green}~{git_branch:yellow}{git_tracking:magenta}{git_status}{git_elapsed:dim}'
#                    ↑ conditional space only when git info exists
```

**Benefits:**
- No extra space when not in a git repository
- Clean separation when git info is present
- Template remains readable and maintainable

## Configuration Example

```toml
[git]
# Git provider configuration
# (Specific config details depend on provider implementation)

[prompt]
# Example prompt with git info
format = '-({time:cyan} {hostname:yellow} {ip:cyan} {cwd:green}~{git_branch:yellow}{git_tracking:magenta}{git_status}{git_elapsed:dim})-'

# Result examples:
# No git:     -(16:46:32 host 100.79.8.56 /tmp/newdir)-
# Clean git:  -(16:46:32 host 100.79.8.56 /tmp/newdir main:✔:2s)-
# Dirty git:  -(16:46:38 host 100.79.8.56 /tmp/newdir main:+1:8s)-
# Behind:     -(16:50:26 host 100.79.8.56 /dotfiles master(behind.3):✔:17h)-
```

## Technical Details

### What the Git Provider Returns

The git provider supplies multiple variables that are concatenated in the template:

```rust
// Conceptual variable structure
git_branch = "main"                         // Branch name (always present in git repo)
git_tracking = "(behind.3)" | "(ahead.2)" | ""  // Remote status (empty if synced)
git_status = ":✔" | ":+1" | ":+2" | ":+1+1"     // Working tree status
git_elapsed = ":2s" | ":5m" | ":17h"           // Elapsed time since last change
```

### Template Rendering

The variables are styled and concatenated without separators (except colons):

```toml
{git_branch:yellow}{git_tracking:magenta}{git_status}{git_elapsed:dim}
```

This produces the compact format:
```
main(behind.3):✔:17h
```

## Comparison with Other Prompt Designs

### Traditional Git Prompts

Many git prompts show verbose output:
```bash
(main *% >)  # symbols are cryptic
[main|MERGING|+1~2-3]  # information overload
```

### Twig's Approach

```bash
main:+1:8s   # clean, scannable, color-coded
```

**Advantages:**
- Text-based indicators are self-documenting (`ahead` vs `*`)
- Color provides redundant encoding of status
- Time context adds temporal awareness
- Compact without being cryptic

## Future Enhancements (Ideas)

Potential additions while maintaining compactness:

1. **Merge conflict indicator**
   `main(conflict):⚠:2m` or similar

2. **Stash count**
   `main[3]:✔:5m` (3 stashes available)

3. **Detached HEAD state**
   `(HEAD@abc123):✔:1m`

4. **Upstream branch name**
   When tracking non-standard remote/branch

**Constraint:** Any additions must not compromise scannability or compactness.

---

## Summary

The twig git prompt is a **production-grade status indicator** that demonstrates thoughtful UX design:

✅ **Comprehensive** - Local + remote + time context
✅ **Compact** - Maximum info, minimum space
✅ **Scannable** - Color-coded for instant recognition
✅ **Semantic** - Colors map to problem categories
✅ **Actionable** - Status clearly indicates next steps

The design philosophy of "color = category, text = specifics" creates a prompt that works for quick glances and detailed reads alike.
