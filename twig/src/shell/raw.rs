use super::ShellFormatter;

/// Raw ANSI formatter - no wrapping
///
/// Used for --prompt flag and default boxed output.
/// Produces standard ANSI escape codes without shell-specific wrapping.
pub struct RawFormatter;

impl ShellFormatter for RawFormatter {
    fn format_ansi(&self, ansi_code: &str, text: &str, reset_code: &str) -> String {
        // No wrapping, just concatenate: ANSI code + text + reset
        format!("{}{}{}", ansi_code, text, reset_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_formatter() {
        let formatter = RawFormatter;
        let result = formatter.format_ansi("\x1b[36m", "test", "\x1b[0m");
        assert_eq!(result, "\x1b[36mtest\x1b[0m");
    }
}
