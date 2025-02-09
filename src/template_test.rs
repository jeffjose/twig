#[cfg(test)]
mod tests {
    use super::super::format_template;

    #[test]
    fn test_tcsh_multiline_format() {
        let template = r#"
-({time:cyan} {host:magenta} {local:magenta} {dir:green})-
# "#;

        let variables = [
            ("time", "12:34:56"),
            ("host", "skyfall"),
            ("local", "192.168.1.1"),
            ("dir", "/home/user"),
        ];

        let expected = "\\n-(%{\u{1b}[36m%}12:34:56%{\u{1b}[0m%} %{\u{1b}[35m%}skyfall%{\u{1b}[0m%} %{\u{1b}[35m%}192.168.1.1%{\u{1b}[0m%} %{\u{1b}[32m%}/home/user%{\u{1b}[0m%})-\\n# ";

        let result = format_template(template, &variables, false, Some("tcsh")).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_tcsh_debug_format() {
        let template = "{time:cyan} {host:magenta}";
        let variables = [("time", "12:34:56"), ("host", "skyfall")];

        let result = format_template(template, &variables, false, Some("tcsh_debug")).unwrap();

        // Split the result into lines
        let lines: Vec<&str> = result.lines().collect();

        // First line should have the normal colored output (with ANSI colors)
        assert!(lines[0].contains("12:34:56"));
        assert!(lines[0].contains("skyfall"));

        // Second line should have the debug format
        assert!(lines[1].starts_with("DEBUG: "));
        assert!(lines[2].contains("{cyan}12:34:56{/cyan}"));
        assert!(lines[2].contains("{magenta}skyfall{/magenta}"));
    }
}
