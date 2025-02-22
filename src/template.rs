use colored::*;
use rstest::rstest;
use std::error::Error;

#[derive(Debug)]
pub enum TemplateError {
    InvalidSyntax(String),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::InvalidSyntax(msg) => write!(f, "Invalid syntax: {}", msg),
        }
    }
}

impl Error for TemplateError {}

fn apply_color(
    text: &str,
    color: &str,
    show_warnings: bool,
    mode: Option<&str>,
) -> Result<String, TemplateError> {
    match mode {
        Some("tcsh") => {
            let color_code = match color {
                "red" => "31",
                "green" => "32",
                "yellow" => "33",
                "blue" => "34",
                "magenta" => "35",
                "cyan" => "36",
                "white" => "37",
                "bright_red" => "1;31",
                "bright_green" => "1;32",
                "bright_yellow" => "1;33",
                "bright_blue" => "1;34",
                "bright_magenta" => "1;35",
                "bright_cyan" => "1;36",
                "bright_white" => "1;37",
                unknown => {
                    if show_warnings {
                        eprintln!("Warning: unknown color '{}', using white instead", unknown);
                    }
                    "37"
                }
            };
            Ok(format!("%{{\x1b[{}m%}}{}%{{\x1b[0m%}}", color_code, text))
        }
        None => {
            let result = match color {
                "red" => text.red().to_string(),
                "green" => text.green().to_string(),
                "yellow" => text.yellow().to_string(),
                "blue" => text.blue().to_string(),
                "magenta" => text.magenta().to_string(),
                "cyan" => text.cyan().to_string(),
                "white" => text.white().to_string(),
                "bright_red" => text.bright_red().to_string(),
                "bright_green" => text.bright_green().to_string(),
                "bright_yellow" => text.bright_yellow().to_string(),
                "bright_blue" => text.bright_blue().to_string(),
                "bright_magenta" => text.bright_magenta().to_string(),
                "bright_cyan" => text.bright_cyan().to_string(),
                "bright_white" => text.bright_white().to_string(),
                unknown => {
                    if show_warnings {
                        eprintln!("Warning: unknown color '{}', using white instead", unknown);
                    }
                    text.white().to_string()
                }
            };
            Ok(result)
        }
        Some(unknown_mode) => {
            if show_warnings {
                eprintln!(
                    "Warning: unknown mode '{}', using default colors",
                    unknown_mode
                );
            }
            apply_color(text, color, show_warnings, None)
        }
    }
}

fn apply_format(
    text: &str,
    format_str: &str,
    show_warnings: bool,
    mode: Option<&str>,
) -> Result<String, TemplateError> {
    match mode {
        Some("tcsh") => {
            // TODO: Implement tcsh formatting
            Ok(text.to_string())
        }
        None => {
            let formats: Vec<&str> = format_str.split(',').map(str::trim).collect();
            let mut colored = text.normal(); // Start with normal style

            for fmt in formats {
                colored = match fmt {
                    // Colors
                    "red" => colored.red(),
                    "green" => colored.green(),
                    "yellow" => colored.yellow(),
                    "blue" => colored.blue(),
                    "magenta" => colored.magenta(),
                    "cyan" => colored.cyan(),
                    "white" => colored.white(),
                    "bright_red" => colored.bright_red(),
                    "bright_green" => colored.bright_green(),
                    "bright_yellow" => colored.bright_yellow(),
                    "bright_blue" => colored.bright_blue(),
                    "bright_magenta" => colored.bright_magenta(),
                    "bright_cyan" => colored.bright_cyan(),
                    "bright_white" => colored.bright_white(),
                    // Styles
                    "bold" => colored.bold(),
                    "italic" => colored.italic(),
                    "normal" => colored.normal(),
                    unknown => {
                        if show_warnings {
                            eprintln!("Warning: unknown format '{}', ignoring", unknown);
                        }
                        colored
                    }
                };
            }
            Ok(colored.to_string())
        }
        Some(unknown_mode) => {
            if show_warnings {
                eprintln!(
                    "Warning: unknown mode '{}', using default formatting",
                    unknown_mode
                );
            }
            apply_format(text, format_str, show_warnings, None)
        }
    }
}

fn validate_variables(template: &str, _variables: &[(&str, &str)]) -> Result<(), TemplateError> {
    // Only validate syntax (unclosed braces)
    let mut pos = 0;
    while let Some(start) = template[pos..].find('{') {
        let start = start + pos;
        if let Some(end) = template[start..].find('}') {
            pos = start + end + 1;
        } else {
            return Err(TemplateError::InvalidSyntax("Unclosed variable".into()));
        }
    }
    Ok(())
}

fn process_variables(
    template: &str,
    variables: &[(&str, &str)],
    show_warnings: bool,
    mode: Option<&str>,
) -> Result<String, TemplateError> {
    let mut result = template.to_string();

    // Process colored variables first (including quoted text)
    for (name, value) in variables {
        let pattern = format!("{{{}:", name);
        let mut position = 0;
        while let Some(start) = result[position..].find(&pattern) {
            let start = start + position;
            let after_var = start + pattern.len();

            if let Some(end) = result[after_var..].find('}') {
                let end = end + after_var;
                let color = &result[after_var..end];
                let colored_value = apply_format(value, color, show_warnings, mode)?;
                result.replace_range(start..end + 1, &colored_value);
                position = start + colored_value.len();
            }
        }
    }

    // Process quoted text (both colored and uncolored)
    let mut position = 0;
    let mut replacements = Vec::new();

    while let Some(start) = result[position..].find("{\"") {
        let start = position + result[position..].find("{\"").unwrap();
        // Find the matching quote that isn't escaped
        let mut quote_end = None;
        let mut pos = start + 2;

        // Convert to char indices for proper Unicode handling
        let chars: Vec<(usize, char)> = result[pos..].char_indices().collect();
        for (i, ch) in chars {
            if ch == '"' {
                let quote_pos = pos + i;
                // Check if this quote is escaped
                let prev_char_pos = result[..quote_pos].chars().count() - 1;
                if prev_char_pos > 0 && result.chars().nth(prev_char_pos) == Some('\\') {
                    continue;
                }
                quote_end = Some(pos + i);
                break;
            }
        }

        if let Some(quote_end) = quote_end {
            let text = &result[start + 2..quote_end];
            // Unescape the text (convert \\" to ")
            let unescaped = text.replace("\\\"", "\"");

            // Check if there's a color specification
            if result[quote_end + 1..].starts_with(':') {
                if let Some(end) = result[quote_end + 1..].find('}') {
                    let end = quote_end + 1 + end;
                    let color = &result[quote_end + 2..end];
                    let formatted_text = apply_format(&unescaped, color, show_warnings, mode)?;
                    replacements.push((start..end + 1, formatted_text));
                    position = end + 1;
                    continue;
                }
            } else if result[quote_end + 1..].starts_with('}') {
                // No color specification, just replace the quoted text
                replacements.push((start..quote_end + 2, unescaped));
                position = quote_end + 2;
                continue;
            }
        }
        position = start + 1;
    }

    // Apply replacements in reverse order to maintain correct indices
    for (range, replacement) in replacements.into_iter().rev() {
        result.replace_range(range, &replacement);
    }

    // Then process non-colored variables
    for (name, value) in variables {
        let pattern = format!("{{{}}}", name);
        while result.contains(&pattern) {
            result = result.replace(&pattern, value);
        }
    }

    // Handle any remaining unmatched variables by keeping them as-is
    let mut pos = 0;
    while let Some(start) = result[pos..].find('{') {
        let start = start + pos;
        if let Some(end) = result[start..].find('}') {
            let end = end + start;
            let var_spec = &result[start..=end];
            let var_name = var_spec[1..end - start].split(':').next().unwrap_or("");

            if show_warnings && !var_name.starts_with('\"') {
                eprintln!("Warning: undefined variable '{}'", var_name);
            }

            pos = end + 1;
        } else {
            pos = start + 1;
        }
    }

    Ok(result)
}

pub fn format_template(
    template: &str,
    variables: &[(&str, &str)],
    show_warnings: bool,
    mode: Option<&str>,
) -> Result<String, TemplateError> {
    // Validate variables first
    validate_variables(template, variables)?;

    if mode == Some("tcsh") {
        // Process variables
        let result = process_variables(template, variables, show_warnings, mode)?;

        // Convert newlines to literal "\n" for tcsh mode
        let mut final_result = String::new();
        for ch in result.chars() {
            if ch == '\n' {
                final_result.push_str("\\n");
            } else {
                final_result.push(ch);
            }
        }
        Ok(final_result)
    } else {
        // For non-tcsh mode, process line by line
        let lines: Vec<&str> = template.lines().collect();
        let mut result_lines = Vec::with_capacity(lines.len());

        for line in lines {
            let processed = process_variables(line, variables, show_warnings, mode)?;
            result_lines.push(processed);
        }

        // Filter empty lines in non-tcsh mode
        Ok(result_lines
            .into_iter()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    // Basic variable substitution
    #[case::basic_var("{var}", "value", vec![("var", "value")])]
    #[case::multiple_vars("{a}{b}", "12", vec![("a", "1"), ("b", "2")])]
    #[case::env_var("{$USER}", "testuser", vec![("$USER", "testuser")])]
    #[case::quoted_text("{\"hello\"}", "hello", vec![])]
    #[case::quoted_with_var("{\"prefix-\"}{var}", "prefix-value", vec![("var", "value")])]
    #[case::repeated_var("{var} {var}", "test test", vec![("var", "test")])]
    #[case::with_punctuation("Hello, {name}!", "Hello, world!", vec![("name", "world")])]
    #[case::plain_text("plain text", "plain text", vec![])]
    #[case::wrapped_var("({var})", "(value)", vec![("var", "value")])]
    #[case::multiple_env_vars("{$USER}@{$HOST}", "testuser@testhost", vec![("$USER", "testuser"), ("$HOST", "testhost")])]
    // Edge cases
    #[case::empty_template("", "", vec![])]
    #[case::invalid_var("{}", "{}", vec![])]
    #[case::nested_braces("{{var}}", "{value}", vec![("var", "value")])]
    #[case::triple_var("{var}{var}{var}", "valuevaluevalue", vec![("var", "value")])]
    #[case::invalid_var_spaces("{ var }", "{ var }", vec![("var", "value")])]
    // Special characters
    #[case::unicode_arrow("{\"→\"}", "→", vec![])]
    #[case::escaped_quotes("{\"\\\"\"}", "\"", vec![])]
    #[case::repeated_with_colon("{var}:{var}", "test:test", vec![("var", "test")])]
    #[case::prefix_suffix("pre{var}post", "prevaluepost", vec![("var", "value")])]
    #[case::multiple_with_dash("{a}-{b}-{c}", "1-2-3", vec![("a", "1"), ("b", "2"), ("c", "3")])]
    // Environment variables
    #[case::path_var("{$PATH}", "/usr/bin", vec![("$PATH", "/usr/bin")])]
    #[case::home_and_user("{$HOME}/{$USER}", "/home/testuser", vec![("$HOME", "/home"), ("$USER", "testuser")])]
    #[case::shell_and_term("{$SHELL}-{$TERM}", "bash-xterm", vec![("$SHELL", "bash"), ("$TERM", "xterm")])]
    // Mixed cases
    #[case::quoted_var_quoted("{\"prefix\"}{var}{\"suffix\"}", "prefixvaluesuffix", vec![("var", "value")])]
    #[case::vars_with_quoted("{a}{\"mid\"}{b}", "1mid2", vec![("a", "1"), ("b", "2")])]
    fn test_basic_substitution(
        #[case] template: &str,
        #[case] expected: &str,
        #[case] vars: Vec<(&str, &str)>,
    ) {
        let output = format_template(template, &vars, false, None).unwrap();
        println!("\nTemplate:        {:?}", template);
        println!("Variables:       {:?}", vars);
        println!("Actual output:   {:?}", output);
        println!("Expected output: {:?}", expected);
        assert_eq!(
            output, expected,
            "Template {:?} with vars {:?} produced unexpected output",
            template, vars
        );
    }

    #[rstest]
    // Basic multiline
    #[case::basic_two_lines("line1\nline2", "line1\nline2", vec![], None)]
    #[case::var_in_both_lines("hello {name}\nbye {name}", "hello world\nbye world", vec![("name", "world")], None)]
    #[case::filter_empty_middle("line1\n\nline2", "line1\nline2", vec![], None)]
    // tcsh mode
    #[case::tcsh_basic("line1\nline2", "line1\\nline2", vec![], Some("tcsh"))]
    #[case::tcsh_with_env_vars("{$USER}\n{$HOST}", "testuser\\ntesthost", vec![("$USER", "testuser"), ("$HOST", "testhost")], Some("tcsh"))]
    #[case::preserve_indent("line1\n  line2\n    line3", "line1\n  line2\n    line3", vec![], None)]
    // Empty lines
    #[case::all_empty("\n\n\n", "", vec![], None)]
    #[case::all_whitespace("  \n  \n  ", "", vec![], None)]
    #[case::multiple_empty_lines("start\n\n\nend", "start\nend", vec![], None)]
    // Indentation
    #[case::increasing_spaces("  a\n    b\n      c", "  a\n    b\n      c", vec![], None)]
    #[case::increasing_tabs("\ta\n\t\tb\n\t\t\tc", "\ta\n\t\tb\n\t\t\tc", vec![], None)]
    // Variables at different positions
    #[case::var_at_line_start("{var}\n{var}", "value\nvalue", vec![("var", "value")], None)]
    #[case::var_mixed_position("start {var}\n{var} end", "start value\nvalue end", vec![("var", "value")], None)]
    // Mixed content
    #[case::three_sections("Title\n---\nContent", "Title\n---\nContent", vec![], None)]
    #[case::markdown_headers("# {title}\n## {subtitle}", "# Header\n## Subheader", vec![("title", "Header"), ("subtitle", "Subheader")], None)]
    // tcsh mode variations
    #[case::tcsh_three_lines("a\nb\nc", "a\\nb\\nc", vec![], Some("tcsh"))]
    #[case::tcsh_repeated_var("{var}\n{var}", "value\\nvalue", vec![("var", "value")], Some("tcsh"))]
    #[case::tcsh_preserve_spaces("  spaces  \n  matter  ", "  spaces  \\n  matter  ", vec![], Some("tcsh"))]
    // Complex cases
    #[case::quoted_arrows("{\">\"}\n{var}\n{\"<\"}", ">\nvalue\n<", vec![("var", "value")], None)]
    #[case::numbered_lines("Line 1 {a}\nLine 2 {b}\nLine 3 {c}", "Line 1 1\nLine 2 2\nLine 3 3", vec![("a", "1"), ("b", "2"), ("c", "3")], None)]
    #[case::markdown_doc("# {title}\n\n## {subtitle}\n\n{content}", "# Header\n## Subheader\nText", vec![("title", "Header"), ("subtitle", "Subheader"), ("content", "Text")], None)]
    // Edge cases
    #[case::single_newline("\n", "", vec![], None)]
    #[case::trailing_newline("a\n", "a", vec![], None)]
    #[case::leading_newline("\na", "a", vec![], None)]
    #[case::multiple_empty_groups("a\n\nb\n\nc", "a\nb\nc", vec![], None)]
    fn test_multiline(
        #[case] template: &str,
        #[case] expected: &str,
        #[case] vars: Vec<(&str, &str)>,
        #[case] mode: Option<&str>,
    ) {
        let output = format_template(template, &vars, false, mode).unwrap();
        println!("\nTemplate:        {:?}", template);
        println!("Variables:       {:?}", vars);
        println!("Mode:           {:?}", mode);
        println!("Actual output:   {:?}", output);
        println!("Expected output: {:?}", expected);
        assert_eq!(
            output, expected,
            "Template {:?} with vars {:?} in mode {:?} produced unexpected output",
            template, vars, mode
        );
    }
}
