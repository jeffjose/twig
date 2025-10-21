# TCSH Prompt Format Reference

## Overview

This document captures the technical details and gotchas for generating TCSH-compatible prompt strings. TCSH prompt handling is tricky and requires specific formatting that differs from other shells.

## Critical Requirements

### 1. ANSI Code Wrapping: `%{...%}`

**The Problem:**
TCSH calculates prompt width to manage line editing, cursor positioning, and line wrapping. Without proper wrapping, ANSI escape codes are counted as visible characters, causing:
- Cursor misalignment when editing commands
- Broken line wrapping for long commands
- Arrow keys (up/down/left/right) not working correctly
- Ctrl+A and Ctrl+E jumping to wrong positions

**The Solution:**
Wrap ALL non-printing ANSI sequences in `%{...%}`:

```tcsh
# WRONG - ANSI codes not wrapped
\x1b[36mtext\x1b[0m

# CORRECT - ANSI codes wrapped in %{...%}
%{\x1b[36m%}text%{\x1b[0m%}
```

**Implementation:**
```rust
fn format_ansi(&self, ansi_code: &str, text: &str, reset_code: &str) -> String {
    // TCSH: wrap ANSI codes in %{...%}
    format!("%{{{}%}}{}%{{{}%}}", ansi_code, text, reset_code)
}
```

**Visual Example:**
```
Input:  {time:cyan} {hostname:magenta}
Output: %{\x1b[36m%}23:17:49%{\x1b[0m%} %{\x1b[35m%}nomad%{\x1b[0m%}
        └─ wrapped ─┘└ visible┘└─ wrapped ─┘ └─ wrapped ─┘└vis┘└─ wrapped ─┘
```

### 2. Newline Handling: Literal `\n` Not Real Newlines

**The Problem:**
TCSH prompts are set as strings, and TCSH interprets certain escape sequences within those strings. A real newline byte (0x0a) is treated as a string terminator or literal character, NOT as "insert newline in prompt".

**The Solution:**
Replace actual newline characters (`\n` byte 0x0a) with the literal two-character sequence backslash-n (`\` + `n` = bytes 0x5c 0x6e):

```tcsh
# WRONG - Real newline byte (0x0a)
set prompt = "-(time host)-<ACTUAL_NEWLINE_BYTE>$ "
# Result: Single line prompt, broken display

# CORRECT - Literal backslash-n
set prompt = "-(time host)-\n$ "
# Result: Proper two-line prompt
```

**Hex Comparison:**
```
WRONG (raw newline):
  00000060  29 2d 0a 24    )  -  \n $
                 ^^ actual newline byte (0x0a)

CORRECT (literal \n):
  00000060  29 2d 5c 6e 24    )  -  \  n  $
                 ^^  ^^ backslash (0x5c) + n (0x6e)
```

**Implementation:**
```rust
fn finalize(&self, output: &str) -> String {
    // Replace real newlines with literal \n for TCSH
    output.replace('\n', "\\n")
}
```

**User Config:**
```toml
[prompt]
format = '''-(time host)-
$ '''
```

**What happens internally:**
1. Template has real newline character between lines
2. `substitute_variables()` produces: `"-(23:17:49 nomad)-\n$ "`
   - At this point, `\n` is a **real newline byte** (0x0a)
3. `finalize()` escapes it: `"-(23:17:49 nomad)-\\n$ "`
   - Now `\n` is **two characters**: backslash + n

**Why this matters:**
```tcsh
# When TCSH evaluates this:
set prompt = "text\nmore"
          #       ^^ TCSH interprets this as "newline here"
          #          It's NOT a real byte, it's TCSH's escape sequence

# So we must give TCSH the literal string "\n"
# NOT a real newline byte
```

### 3. TCSH Escape Sequences

TCSH interprets several escape sequences in prompt strings:

| Sequence | Meaning | Example |
|----------|---------|---------|
| `\n` | Newline | `"line1\nline2"` |
| `%/` | Current directory | `"%/"` → `/home/user` |
| `%~` | Current directory with ~ | `"%~"` → `~/projects` |
| `%c` | Trailing component of cwd | `"%c"` → `projects` |
| `%h` | Current history number | `"%h"` → `42` |
| `%M` | Full hostname | `"%M"` → `nomad.local` |
| `%m` | Hostname up to first . | `"%m"` → `nomad` |
| `%n` | Username | `"%n"` → `jeffjose` |
| `%t` | 12-hour time (hh:mm AM/PM) | `"%t"` → `11:30 PM` |
| `%T` | 24-hour time (hh:mm) | `"%T"` → `23:30` |
| `%p` | 24-hour time with seconds | `"%p"` → `23:30:45` |
| `%P` | 12-hour time with seconds | `"%P"` → `11:30:45 PM` |
| `%B` | Start bold | `"%Bbold%b"` |
| `%b` | End bold | `"%Bbold%b"` |
| `%{...%}` | Non-printing sequence | `"%{\x1b[36m%}"` |

**Important:** Since we're generating the entire prompt string ourselves (with colors, time, etc.), we don't use most of TCSH's built-in escapes. We only need `\n` and `%{...%}`.

## Complete Example

### User's Config (TOML)
```toml
[time]
format = "%H:%M:%S"

[prompt]
format = '''-(time:cyan} {hostname:magenta} {cwd:green})-
{"$":white,bold} '''
```

### Processing Steps

**Step 1: Template has real newline**
```
Template: "-(time:cyan} {hostname:magenta} {cwd:green})-\n{\"$\":white,bold} "
          (where \n is actual byte 0x0a)
```

**Step 2: Variable substitution with TCSH formatter**
```rust
// Each variable is wrapped:
"{time:cyan}" → "%{\x1b[36m%}23:17:49%{\x1b[0m%}"
"{hostname:magenta}" → "%{\x1b[35m%}nomad%{\x1b[0m%}"
"{cwd:green}" → "%{\x1b[32m%}/home/jeffjose/scripts/twig%{\x1b[0m%}"
{"$":white,bold}" → "%{\x1b[37;1m%}$%{\x1b[0m%}"

// Result after substitution:
"-(%{\x1b[36m%}23:17:49%{\x1b[0m%} %{\x1b[35m%}nomad%{\x1b[0m%} %{\x1b[32m%}/home/jeffjose/scripts/twig%{\x1b[0m%})-\n%{\x1b[37;1m%}$%{\x1b[0m%} "
                                                                                                                       ^^ still real newline
```

**Step 3: Finalize (escape newlines)**
```rust
output.replace('\n', "\\n")

// Result:
"-(%{\x1b[36m%}23:17:49%{\x1b[0m%} %{\x1b[35m%}nomad%{\x1b[0m%} %{\x1b[32m%}/home/jeffjose/scripts/twig%{\x1b[0m%})-\\n%{\x1b[37;1m%}$%{\x1b[0m%} "
                                                                                                                       ^^ now literal \n
```

**Step 4: TCSH receives and interprets**
```tcsh
set prompt = "`twig --mode tcsh`"

# TCSH interprets the \n as "newline here"
# TCSH interprets %{...%} as "non-printing"
# TCSH sends ANSI codes to terminal
# Terminal renders colors
```

**Step 5: What user sees**
```
-(23:17:49 nomad /home/jeffjose/scripts/twig)-
$ █
```
(with colors: cyan time, magenta hostname, green path, bold white $)

## Testing Reference

### Hex Dump Analysis

**Check for proper newline escaping:**
```bash
twig --mode tcsh | hexdump -C
```

Look for the transition from first line to second line:
```
# CORRECT - Literal \n (bytes 5c 6e)
00000060  29 2d 5c 6e 25 7b    )-\n%{
              ^^^^^ backslash-n (two bytes)

# WRONG - Real newline (byte 0a)
00000060  29 2d 0a 25 7b    )-.%{
              ^^ single newline byte
```

**Check for proper ANSI wrapping:**
```bash
twig --mode tcsh | cat -A
```

Every ANSI code should be wrapped:
```
# CORRECT
%{^[[36m%}text%{^[[0m%}
  ^^^^^^^^     ^^^^^^^^ wrapped

# WRONG
^[[36mtext^[[0m
^^^^^^    ^^^^ not wrapped, TCSH thinks these are visible
```

### Visual Testing in TCSH

1. **Set the prompt:**
   ```tcsh
   set prompt="`twig --mode tcsh`"
   ```

2. **Test cursor positioning:**
   - Type a long command (80+ chars)
   - Press Home (Ctrl+A) - cursor should go to line start
   - Press End (Ctrl+E) - cursor should go to line end
   - Use arrow keys - cursor should move correctly
   - If cursor jumps to wrong positions, ANSI codes aren't wrapped

3. **Test line wrapping:**
   - Type a command longer than terminal width
   - It should wrap to next line cleanly
   - If it overwrites the prompt, width calculation is wrong

4. **Test multiline:**
   - Prompt should appear on two lines
   - Second line should show `$ ` with correct cursor position
   - If everything is on one line, newline wasn't escaped

5. **Test history:**
   - Press up arrow to get previous command
   - Press down arrow to go forward
   - If history is garbled, prompt width calculation is broken

## Critical Edge Cases

### Edge Case 1: `%}` Immediately Followed by `\n`

**The Problem:**
When a prompt ends with a colored item followed immediately by a newline, TCSH fails to parse the newline correctly:

```toml
# This format causes the bug:
format = '''-({time:cyan} {hostname:magenta} {cwd:green}
{"$":white,bold} '''
```

This generates: `%{\x1b[32m%}/path%{\x1b[0m%}\n$`

The pattern `%}\n` (closing non-printing marker immediately followed by literal newline) causes TCSH to ignore the `\n`, rendering the entire prompt on one line instead of two.

**Why It Happens:**
TCSH's prompt parser doesn't recognize `\n` as an escape sequence when it directly follows `%}`. The closing marker seems to "consume" or interfere with the newline parsing.

**The Fix:**
Insert a space between `%}` and `\n`:

```rust
// In finalize() method:
output.replace("%}\\n", "%} \\n")
```

This transforms:
- **Before:** `twig%{\x1b[0m%}\n$ ` (broken - one line)
- **After:** `twig%{\x1b[0m%} \n$ ` (fixed - two lines)

**The Solution Works Because:**
The trailing space is invisible (it's at the end of a line) but provides the separation TCSH needs to correctly parse the `\n` escape sequence.

**Visual Comparison:**
```bash
# Broken (missing newline):
-(00:15:28 skyfall /home/jeffjose/scripts/twig$

# Fixed (proper two lines):
-(00:15:21 skyfall /home/jeffjose/scripts/twig)-
$
```

**Hex Dump Analysis:**
```bash
# Broken: %} immediately followed by \n
00000060:  ...twig%{ESC[0m%}\n...
                           ^^^^^ - TCSH ignores this \n

# Fixed: %} followed by space then \n
00000060:  ...twig%{ESC[0m%} \n...
                           ^^^^^^ - TCSH correctly parses this \n
```

**Implementation:**
This fix is automatically applied in `tcsh.rs` and `zsh.rs` by the `finalize()` method. No user action required.

---

## Common Mistakes

### Mistake 1: Raw ANSI codes
```tcsh
# WRONG
set prompt = "\x1b[36mtext\x1b[0m"

# Terminal sees: ESC[36m (counts as 5+ characters)
# TCSH thinks prompt is 5+ characters wide
# Result: Cursor positioning broken
```

### Mistake 2: Actual newline bytes
```tcsh
# WRONG (from code that outputs real newline)
set prompt = "`echo -e "line1\nline2"`"
# The \n is a real byte (0x0a)
# TCSH doesn't interpret it as "newline in prompt"

# CORRECT
set prompt = "line1\nline2"
# The \n is literal text that TCSH interprets
```

### Mistake 3: Forgetting the space at the end
```tcsh
# WRONG
set prompt = "$ "
            ^^ no space after $

# CORRECT
set prompt = "$ "
              ^ space here, cursor appears after it
```

### Mistake 4: Wrapping visible text
```tcsh
# WRONG
set prompt = "%{text%}"
# Now TCSH thinks "text" is non-printing
# Prompt appears empty

# CORRECT
set prompt = "%{\x1b[36m%}text%{\x1b[0m%}"
# Only ANSI codes are wrapped
```

## Comparison with Other Shells

### Bash
- Uses `\[...\]` for non-printing sequences
- Uses real newline bytes in PS1
- Example: `PS1="\[\x1b[36m\]text\[\x1b[0m\]\n$ "`

### Zsh
- Uses `%{...%}` like TCSH (same syntax!)
- Uses literal `\n` like TCSH
- But has different prompt expansion (%, $, etc.)

### Fish
- Uses raw ANSI codes
- Handles newlines natively
- Example: `set fish_prompt "\x1b[36mtext\x1b[0m\n\$ "`

## Implementation Checklist

When implementing TCSH prompt generation:

- [ ] Wrap every ANSI escape code in `%{...%}`
- [ ] Wrap both the color code AND the reset code separately
- [ ] Replace real newlines with literal `\n` in finalize()
- [ ] Do NOT wrap visible text in `%{...%}`
- [ ] Add trailing space after final prompt character
- [ ] Test with hexdump to verify `\n` is `5c 6e` not `0a`
- [ ] Test with cat -A to verify all ANSI codes wrapped
- [ ] Test in actual TCSH shell with cursor movement
- [ ] Test with long commands to verify line wrapping
- [ ] Test multiline prompts render on separate lines
- [ ] Handle edge case: insert space before `\n` when preceded by `%}`

## Debugging

### Symptom: Cursor jumps around when editing
**Cause:** ANSI codes not wrapped in `%{...%}`
**Fix:** Every `\x1b[...m` must be inside `%{...%}`

### Symptom: Prompt is all on one line
**Possible Causes:**
1. Real newline byte instead of literal `\n`
   - **Fix:** Use `output.replace('\n', "\\n")` in finalize()
2. `%}` immediately followed by `\n` (edge case)
   - **Fix:** Insert space between them: `%} \n` instead of `%}\n`
   - See "Critical Edge Cases" section above for details

### Symptom: Colors bleed into typed commands
**Cause:** Missing reset code or reset not wrapped
**Fix:** Always end with `%{\x1b[0m%}` after colored text

### Symptom: Prompt width calculation wrong
**Cause:** Visible text wrapped in `%{...%}`
**Fix:** Only wrap ANSI codes, not actual text

## References

- TCSH Manual: https://www.tcsh.org/tcsh.html/Prompt_formatting.html
- ANSI Escape Codes: https://en.wikipedia.org/wiki/ANSI_escape_code
- Related Issue: Multiline TCSH prompts (commit 3391fe4)

## Summary

**The Golden Rules for TCSH Prompts:**

1. **All ANSI codes must be wrapped in `%{...%}`** - TCSH needs to know they're invisible
2. **Use literal `\n` not real newlines** - TCSH interprets the string `\n` as "insert newline"
3. **Always reset colors** - Use `\x1b[0m` after colored text
4. **End with a space** - Give the cursor somewhere to land

These rules are non-negotiable for TCSH. Break any of them and the prompt will be broken.
