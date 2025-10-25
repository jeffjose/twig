# TODO - Twig Development

## ✅ RESOLVED: Dynamic Responsive Prompt Switching

### Issue Resolution Summary
**Root Cause Found**: The `visible_length()` function was counting tcsh escape sequences (`%{...%}`) as visible characters, causing incorrect length calculations (213 chars vs actual ~92 chars).

**Fixes Applied** (2025-10-24):
1. **Fixed `visible_length()` calculation** (twig/src/main.rs:696-704)
   - Now strips both raw ANSI codes (`\x1b[...m`) AND shell wrappers (`%{...%}`)
   - Accurate visible length measurement for all shell modes

2. **Made padding configurable** (twig/src/config.rs:75)
   - New optional `padding` parameter (default: 5 characters)
   - Previously hardcoded at 10 characters
   - Lower default is less conservative, better for most terminals

3. **Updated documentation** (config.toml)
   - Clarified dynamic switching formula: `length + padding > terminal_width`
   - Added padding configuration examples

**Status**: ✅ Working correctly
- Terminal width detection: ✅ Fixed (using stderr fallback)
- Visible length calculation: ✅ Fixed (strips shell wrappers)
- Dynamic switching: ✅ Working as designed
- User tested: ✅ Confirmed working

---

## Previous Investigation (For Reference)

### Original Issue Being Investigated
**Problem**: Dynamic responsive switching from narrow to wide format doesn't appear to be working correctly in the actual shell prompt.

**User Observation**:
- Running `cargo run` (terminal width 114-140): Shows WIDE format (full prompt with time, hostname, IP, battery, git info)
- Actual shell prompt (same terminal width): Shows NARROW format (compact: cwd + git only)
- Expected: Both should show the same format based on terminal width

### What We Implemented (Completed)

#### 1. Dynamic Length-Based Responsive Switching
**File**: `twig/src/main.rs`

**How it works**:
```rust
// Dynamic mode (when width_threshold is None):
1. Renders prompt with `format` (or `format_wide` if available)
2. Measures visible length by stripping ANSI codes
3. If (visible_length + buffer) > terminal_width:
   - Switches to format_narrow
   - Re-renders with narrow format
4. Otherwise keeps original format
```

**Configuration** (`config.toml`):
- `format` = base/fallback format (REQUIRED - the long version with all info)
- `format_wide` = optional override for wide terminals (user doesn't have this)
- `format_narrow` = optional compact format (user has this)
- `width_threshold` = None for dynamic mode, Some(n) for static threshold mode

User's current config:
```toml
[prompt]
format = '''--({time:cyan} {hostname:yellow} {ip_address:yellow}~{battery_percentage:yellow}...'''
format_narrow = '''{cwd:green}~{git_branch:magenta}{git_status_clean:green}...'''
# No width_threshold = DYNAMIC MODE
```

#### 2. Debug Output for Troubleshooting
**Added comprehensive debug logging**:

```bash
[DEBUG] Initial format selected: --({time:cyan} {hostname:yellow}...
[DEBUG] Terminal width: Some(114)
[DEBUG] width_threshold: None
[DEBUG] Has format_narrow: true
[DEBUG] Rendered prompt visible length: 95
[DEBUG] Terminal width: 114
[DEBUG] Check: 95 + 10 > 114? false
[DEBUG] Keeping original format (fits in terminal)
```

Enable with: `TWIG_DEBUG=1` environment variable or `--debug` flag

#### 3. Improved Terminal Width Detection
**Problem**: When twig runs in shell prompt mode (`--mode bash`), stdout is captured by the shell, so terminal size detection fails (returns `None`)

**Solution**: Added stderr fallback
```rust
let terminal_width = terminal_size()  // Try stdout first
    .or_else(|| {
        terminal_size_using_fd(std::io::stderr().as_raw_fd())  // Fallback to stderr
    })
    .map(|(Width(w), _)| w);
```

This should allow terminal width detection even when stdout is redirected.

#### 4. UI Updates
- **Removed**: "Format: {format_string}" line from boxed output
- **Kept**: Terminal width display in Config line: `Config: /path/to/config.toml (width: 114)`
- **Updated**: `print_debug_box()` to show terminal width and format used

### Testing Limitations
**Why we can't fully test in this environment**:
- The Bash tool doesn't run in a real TTY
- Terminal width detection returns `None` when not in a TTY
- Need real shell integration to test dynamic switching

### Next Steps (TODO)

#### IMMEDIATE: Test in Real Shell
1. **Install the new binary** in your shell:
   ```bash
   cd /usr/local/google/home/jeffjose/scripts/twig/twig
   cargo build --release
   # Update your shell config to use the new binary
   ```

2. **Test with debug output** to see what's happening:
   ```bash
   export TWIG_DEBUG=1
   # Then trigger your prompt (press Enter or cd somewhere)
   ```

3. **Look for these debug lines in stderr**:
   - `[DEBUG] Terminal width: Some(N)` or `None`?
   - `[DEBUG] Rendered prompt visible length: N`
   - `[DEBUG] Switching to narrow format!` or `Keeping original format`

4. **Key questions to answer**:
   - Is terminal width being detected? (should be `Some(N)`, not `None`)
   - What is the visible length of the rendered prompt?
   - Is the switching logic triggering when you expect/don't expect?

#### POTENTIAL FIXES (depending on test results)

**If terminal width is still `None`**:
- The stderr fallback might not be working
- May need to try stdin as third option
- Or pass terminal width via environment variable

**If switching is too aggressive** (switching when you don't want it to):
- Current buffer is 10 characters
- Could make it configurable: `switch_buffer = 10` in config
- Or increase default buffer to 20-30

**If switching is not aggressive enough** (not switching when it should):
- The visible_length calculation might be wrong
- May need to account for multi-byte characters differently
- Or the buffer is too large

**If wide->narrow works but narrow->wide doesn't**:
- Current logic only switches FROM initial format TO narrow
- Doesn't switch back to wide if terminal gets bigger
- Would need bidirectional switching logic:
  ```rust
  // Try narrow first if last prompt was too long
  // Try wide again if narrow + buffer < width
  ```
- This would require state tracking between prompts

### Code Structure Reference

**Dynamic switching logic** (`twig/src/main.rs:125-154`):
```rust
let mut format_used = format.clone();
if config.prompt.width_threshold.is_none() {
    if let (Some(width), Some(ref narrow_format)) = (terminal_width, &config.prompt.format_narrow) {
        let visible_len = visible_length(&output);
        let buffer = 10;

        if visible_len + buffer > width as usize {
            // Switch to narrow
            output = substitute_variables(narrow_format, &variables, formatter.as_ref());
            output = formatter.finalize(&output);
            format_used = narrow_format.clone();
        }
    }
}
```

**get_format() logic** (`twig/src/config.rs:90-117`):
```rust
// Returns which format to start with:
// - If width_threshold set: static threshold switching
// - If no threshold: returns format_wide (if exists) for dynamic checking, else format
pub fn get_format(&self, terminal_width: Option<u16>) -> &str
```

**Test coverage**: All 57 tests passing
- `test_get_format_dynamic()` - Tests dynamic mode returns wide format
- Other tests cover static threshold switching

### Recent Commits
- `845933a` - feat: improve terminal width detection and debug output
- Previous work on responsive prompts and battery provider

### Files Modified This Session
- `twig/src/main.rs` - Dynamic switching logic, debug output, terminal detection
- `twig/src/config.rs` - Already had the get_format() logic (previous session)

### Questions to Resolve
1. Why does `cargo run` show wide format but actual prompt shows narrow?
   - Hypothesis: The actual prompt content (with real git data, long paths) is longer than cargo run test
   - Hypothesis: Terminal width detection is failing in actual shell (returns None)

2. Should we make the buffer configurable?
   - Currently hardcoded to 10
   - User might want different sensitivity

3. Do we need bidirectional switching (narrow->wide when terminal expands)?
   - Current: only wide->narrow
   - Would need state tracking

### Environment Details
- Working directory: `/usr/local/google/home/jeffjose/scripts/twig/twig`
- Config file: `/usr/local/google/home/jeffjose/.config/twig/config.toml`
- Terminal width when testing: 114-140 columns
- All tests passing: 57/57

---

## Action Items for User
- [ ] Test with `TWIG_DEBUG=1` in actual shell
- [ ] Report what terminal width is detected
- [ ] Report visible length of prompt
- [ ] Report whether switching logic is triggering
- [ ] Confirm whether stderr fallback fixed detection issue

## Action Items for Development
- [ ] Await test results from real shell
- [ ] Adjust buffer value if needed
- [ ] Consider making buffer configurable
- [ ] Consider bidirectional switching if needed
- [ ] Add visible length to regular output (not just debug)?
