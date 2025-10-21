use super::ShellFormatter;

/// Zsh formatter with %{...%} wrapping
///
/// Wraps ANSI escape codes in %{...%} to tell Zsh that the enclosed
/// characters don't consume visible space. This prevents line editing issues.
pub struct ZshFormatter;

impl ShellFormatter for ZshFormatter {
    fn format_ansi(&self, ansi_code: &str, text: &str, reset_code: &str) -> String {
        // Wrap ANSI codes in %{...%}
        format!("%{{{}%}}{}%{{{}%}}", ansi_code, text, reset_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zsh_formatter() {
        let formatter = ZshFormatter;
        let result = formatter.format_ansi("\x1b[36m", "test", "\x1b[0m");
        assert_eq!(result, "%{\x1b[36m%}test%{\x1b[0m%}");
    }
}
