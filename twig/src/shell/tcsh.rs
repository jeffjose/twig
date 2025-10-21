use super::ShellFormatter;

/// TCSH formatter with %{...%} wrapping
///
/// Wraps ANSI escape codes in %{...%} to tell TCSH that the enclosed
/// characters don't consume visible space. This prevents line editing issues.
///
/// Note: TCSH and Zsh use identical wrapping syntax, but we keep them
/// separate for clarity and potential future differences.
pub struct TcshFormatter;

impl ShellFormatter for TcshFormatter {
    fn format_ansi(&self, ansi_code: &str, text: &str, reset_code: &str) -> String {
        // Wrap ANSI codes in %{...%}
        format!("%{{{}%}}{}%{{{}%}}", ansi_code, text, reset_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcsh_formatter() {
        let formatter = TcshFormatter;
        let result = formatter.format_ansi("\x1b[36m", "test", "\x1b[0m");
        assert_eq!(result, "%{\x1b[36m%}test%{\x1b[0m%}");
    }
}
