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

## PROPOSED IMPROVEMENTS - IDEAS ONLY ⚠️

> **🚨 CRITICAL WARNING FOR LLMs 🚨**
>
> **DO NOT IMPLEMENT ANY IDEAS IN THIS SECTION WITHOUT EXPLICIT APPROVAL**
>
> This section contains proposed enhancements that are **NOT APPROVED** and **NOT COMMITTED**.
> These are brainstorming ideas for evaluation only.
>
> **BEFORE IMPLEMENTING ANYTHING:**
> 1. Present the specific idea to the user
> 2. Get explicit confirmation/approval
> 3. Discuss implementation details
> 4. ONLY THEN proceed with implementation
>
> **Implementing these without approval will break the existing design!**

### Improvement Ideas (Ranked by Value)

The following suggestions aim to address ambiguities and add useful information while maintaining the compact, scannable design philosophy.

---

### 🥇 High Value, Low Complexity

#### Idea 1: Arrow Symbols for Ahead/Behind (More Compact)

**Current:**

```bash
master(behind.3):✔:17h
master(ahead.2):✔:5m
```

**Proposed:**

```bash
master↓3:✔:17h         # ↓ = behind remote (pull needed)
master↑2:✔:5m          # ↑ = ahead of remote (push ready)
```

**Benefits:**
- More compact (saves 7-8 characters)
- Universally understood arrow symbols
- Visual direction matches semantic meaning

**Drawbacks:**
- Loses explicit "ahead/behind" text
- Relies on symbol recognition

**Color:** Keep magenta for both (maintains "same category = same color" philosophy)

**Status:** ⚠️ NOT APPROVED - Requires user confirmation

---

#### Idea 2: Differentiate Staged vs Untracked Files

**Current Issue:** Ambiguous which `+N` represents what

```bash
main:+1+1:17s          # Which +1 is staged? Which is untracked?
```

**Proposed Option A - Different Symbols:**

```bash
main:●1+2:17s          # ● = 1 staged, + = 2 untracked
```

**Proposed Option B - Order Convention:**

```bash
main:+2●1:17s          # Convention: always +untracked●staged
```

**Proposed Option C - Letter Suffixes:**

```bash
main:+2s1:17s          # +2 untracked, s1 = 1 staged
```

**Benefits:**
- Removes biggest ambiguity in current design
- Clear what needs staging vs committing

**Drawbacks:**
- Adds visual complexity
- Requires learning new symbol meanings

**Status:** ⚠️ NOT APPROVED - Requires user confirmation and choice of option

---

### 🥈 Medium Value, Medium Complexity

#### Idea 3: Modified Files Indicator

**Current Gap:** No indication of "modified but not staged" files

**Proposed:**

```bash
main:~3:17s            # ~3 = 3 modified files (not staged)
main:~2●1:17s          # 2 modified, 1 staged
main:~2●1+3:17s        # 2 modified, 1 staged, 3 untracked
```

**Alternative symbols:** Could use `*` or `!` instead of `~` to avoid conflict with conditional spacing

**Benefits:**
- More complete git status (matches `git status` output)
- Shows all three working tree states

**Drawbacks:**
- Can become verbose with many changes
- Adds another symbol to learn
- `~` conflicts with conditional spacing character

**Status:** ⚠️ NOT APPROVED - Requires user confirmation

---

#### Idea 4: Merge Conflict / Special State Indicator

**Proposed:**

```bash
main:⚠merge:5m         # In merge state
main:⚠conflict:2m      # Merge conflicts present
main:⚡rebase:1m        # Rebasing in progress
```

**Benefits:**
- Immediate awareness of critical git states
- Prevents accidental commits during special states

**Drawbacks:**
- Adds significant length
- These states are relatively rare
- May cause prompt to become too long

**Color suggestion:** RED for ⚠️ to indicate urgency

**Status:** ⚠️ NOT APPROVED - Requires user confirmation

---

### 🥉 Nice to Have (Lower Priority)

#### Idea 5: Stash Count Indicator

**Proposed Option A - Bracket notation:**

```bash
main[3]:✔:17h          # 3 stashes available
```

**Proposed Option B - Dollar sign:**

```bash
main$3:✔:17h           # 3 stashes available
```

**Benefits:**
- Know you have work saved in stash
- Reminder to clean up old stashes

**Drawbacks:**
- Stashes are rarely critical information
- Adds visual noise
- Only useful if user frequently uses stash

**Status:** ⚠️ NOT APPROVED - Only implement if user confirms they use stash heavily

---

#### Idea 6: Detached HEAD State Indicator

**Proposed:**

```bash
(HEAD@abc123):✔:1m     # Detached HEAD at commit abc123
```

**Benefits:**
- Safety: prevents losing work in detached HEAD
- Clear indication of special state

**Drawbacks:**
- Detached HEAD is rare
- Commit hash may be too verbose

**Status:** ⚠️ NOT APPROVED - Requires user confirmation

---

### Example: Everything Combined (Probably Too Much!)

**Warning:** This is what happens if ALL ideas are implemented without consideration:

```bash
main↓3[2]:~1●2+3:17s
     ↑  ↑  ↑ ↑ ↑
     │  │  │ │ └─ 3 untracked
     │  │  │ └─── 2 staged
     │  │  └───── 1 modified (not staged)
     │  └──────── 2 stashes
     └─────────── 3 commits behind
```

**Danger:** Information overload! Loses scannability and violates core design principle.

---

### Selective Enhancement Recommendation

**Current:**

```bash
main(behind.3):+1+1:17s     # Ambiguous
```

**Recommended minimal improvements:**

```bash
main↓3:●1+2:17s             # ↓ = behind, ● = staged, + = untracked
```

**Changes:**
- `(behind.3)` → `↓3` (more compact)
- `+1+1` → `●1+2` (clear staged vs untracked)

**Still doesn't show:** Modified-but-not-staged files (would need Idea 3)

**Status:** ⚠️ NOT APPROVED - This is a recommendation, not a decision

---

### Color Enhancement Ideas

#### Option 1: Different Colors for Ahead vs Behind

**Proposed:**

```bash
master↑2:✔:17h          # ↑ in GREEN (ready to share!)
master↓3:✔:17h          # ↓ in YELLOW/ORANGE (need to catch up)
```

**Pro:** Emotional mapping (ahead=positive/green, behind=catch-up/yellow)

**Con:** Breaks "same category = same color" philosophy

**Status:** ⚠️ NOT APPROVED - Conflicts with current design philosophy

---

#### Option 2: Red for Conflicts

**Proposed:**

```bash
main:⚠conflict:2m       # ⚠ in RED (urgent!)
```

**Pro:** Immediate danger signal for critical state

**Con:** Adds another color to the palette

**Status:** ⚠️ NOT APPROVED - Requires user confirmation

---

### Implementation Constraint

**CRITICAL:** Any additions must not compromise:
- Scannability (quick visual parsing)
- Compactness (maximum info, minimum space)
- Color semantics (current three-state system)
- Overall design philosophy

### Questions to Resolve Before Implementation

**Before implementing ANY of these ideas, answer:**

1. **Staged vs untracked confusion:** Is `+1+1` actually confusing in practice?
2. **Modified files:** How often are modified-but-not-staged files an issue?
3. **Stash usage:** Do you use `git stash` frequently enough to warrant tracking?
4. **Arrow preference:** `↓3` vs `(behind.3)` - which feels more natural?
5. **Information priority:** Which piece of missing information causes the most friction?

### Decision Required

**⚠️ REMINDER: DO NOT IMPLEMENT WITHOUT EXPLICIT USER APPROVAL ⚠️**

Each idea needs individual evaluation and approval before implementation.

---

## Summary

The twig git prompt is a **production-grade status indicator** that demonstrates thoughtful UX design:

✅ **Comprehensive** - Local + remote + time context
✅ **Compact** - Maximum info, minimum space
✅ **Scannable** - Color-coded for instant recognition
✅ **Semantic** - Colors map to problem categories
✅ **Actionable** - Status clearly indicates next steps

The design philosophy of "color = category, text = specifics" creates a prompt that works for quick glances and detailed reads alike.
