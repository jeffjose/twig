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

        // Escape ! for TCSH history expansion
        // In tcsh, "!" triggers history expansion, so we escape it to "\!"
        let output = output.replace('!', "\\!");

        // Escape % for TCSH prompt formatting
        // In tcsh, "%" is special (e.g., %n for username, %/ for path)
        // We need to escape literal "%" to "%%" but preserve our formatting %{ and %}
        let output = output.replace('%', "%%");
        let output = output.replace("%%{", "%{");
        let output = output.replace("%%}", "%}");

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

    #[test]
    fn test_tcsh_finalize_exclamation_escaping() {
        let formatter = TcshFormatter;
        // Test that ! is escaped to \! for tcsh history expansion
        let input = "! ";
        let expected = "\\! ";
        assert_eq!(formatter.finalize(input), expected);

        // Test with formatted prompt
        let input = "%{\x1b[37m\x1b[1m%}!%{\x1b[0m%} ";
        let expected = "%{\x1b[37m\x1b[1m%}\\!%{\x1b[0m%} ";
        assert_eq!(formatter.finalize(input), expected);
    }

    #[test]
    fn test_tcsh_finalize_percent_escaping() {
        let formatter = TcshFormatter;
        // Test that % is escaped to %% for tcsh prompt formatting
        // but %{ and %} are preserved for ANSI wrapping
        let input = "%{\x1b[33m%}85%%{\x1b[0m%}";
        let expected = "%{\x1b[33m%}85%%%{\x1b[0m%}";
        assert_eq!(formatter.finalize(input), expected);

        // Test multiple percent signs
        let input = "100% complete";
        let expected = "100%% complete";
        assert_eq!(formatter.finalize(input), expected);
    }
}
