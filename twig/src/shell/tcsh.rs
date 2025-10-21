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

    fn finalize(&self, output: &str) -> String {
        // TCSH needs literal \n instead of actual newline characters
        let output = output.replace('\n', "\\n");

        // Fix edge case: when %} is immediately followed by \n, tcsh doesn't parse
        // the newline correctly. Insert a space between them.
        // The space is invisible at the end of the line but allows tcsh to parse the \n.
        output.replace("%}\\n", "%} \\n")
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

    #[test]
    fn test_tcsh_finalize_newline() {
        let formatter = TcshFormatter;
        // Test basic newline replacement
        assert_eq!(formatter.finalize("line1\nline2"), "line1\\nline2");
    }

    #[test]
    fn test_tcsh_finalize_edge_case() {
        let formatter = TcshFormatter;
        // Test edge case: when %} is immediately followed by \n
        // TCSH doesn't parse this correctly, so we insert a space
        let input = "%{\x1b[32m%}/path%{\x1b[0m%}\n$ ";
        let expected = "%{\x1b[32m%}/path%{\x1b[0m%} \\n$ ";
        assert_eq!(formatter.finalize(input), expected);
    }
}
