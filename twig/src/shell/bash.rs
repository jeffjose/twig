use super::ShellFormatter;

/// Bash formatter with \[...\] wrapping
///
/// Wraps ANSI escape codes in \[...\] to tell Bash that the enclosed
/// characters are non-printing. This prevents line editing issues.
pub struct BashFormatter;

impl ShellFormatter for BashFormatter {
    fn format_ansi(&self, ansi_code: &str, text: &str, reset_code: &str) -> String {
        // Wrap ANSI codes in \[...\]
        format!("\\[{}\\]{}\\[{}\\]", ansi_code, text, reset_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bash_formatter() {
        let formatter = BashFormatter;
        let result = formatter.format_ansi("\x1b[36m", "test", "\x1b[0m");
        assert_eq!(result, "\\[\x1b[36m\\]test\\[\x1b[0m\\]");
    }
}
