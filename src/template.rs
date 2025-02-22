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

fn apply_format(
    text: &str,
    format_str: &str,
    show_warnings: bool,
    mode: Option<&str>,
) -> Result<String, TemplateError> {
    // Handle empty or whitespace-only format string
    if format_str.trim().is_empty() {
        return Ok(text.to_string());
    }

    match mode {
        Some("tcsh") => {
            let formats: Vec<&str> = format_str.split(',').map(str::trim).collect();
            let mut codes = Vec::new();

            for fmt in formats {
                let code = match fmt.trim() {
                    // Colors
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
                    // Styles
                    "bold" => "1",
                    "italic" => "3",
                    "normal" => "0",
                    unknown => {
                        if show_warnings {
                            eprintln!("Warning: unknown format '{}', ignoring", unknown);
                        }
                        continue;
                    }
                };
                codes.push(code);
            }

            if codes.is_empty() {
                Ok(text.to_string())
            } else {
                let combined_codes = codes.join(";");
                Ok(format!(
                    "%{{\x1b[{}m%}}{}%{{\x1b[0m%}}",
                    combined_codes, text
                ))
            }
        }
        None => {
            let formats: Vec<&str> = format_str.split(',').map(str::trim).collect();
            let mut codes = Vec::new();

            for fmt in formats {
                let code = match fmt.trim() {
                    // Colors
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
                    // Styles
                    "bold" => "1",
                    "italic" => "3",
                    "normal" => "0",
                    unknown => {
                        if show_warnings {
                            eprintln!("Warning: unknown format '{}', ignoring", unknown);
                        }
                        "37" // Default to white for unknown colors
                    }
                };
                codes.push(code);
            }

            if codes.is_empty() {
                Ok(text.to_string())
            } else {
                let combined_codes = codes.join(";");
                Ok(format!("\x1b[{}m{}\x1b[0m", combined_codes, text))
            }
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
                let format_str = &result[after_var..end];

                // Skip if there are spaces in the format string
                if format_str.contains(' ') {
                    result.replace_range(start..end + 1, value);
                    position = start + value.len();
                    continue;
                }

                // Skip if this is part of a nested format
                let prefix = &result[..start];
                if prefix.ends_with('{') {
                    result.replace_range(start..end + 1, value);
                    position = start + value.len();
                    continue;
                }

                let formatted_value = apply_format(value, format_str, show_warnings, mode)?;
                result.replace_range(start..end + 1, &formatted_value);
                position = start + formatted_value.len();
            }
        }
    }

    // Process quoted text (both colored and uncolored)
    let mut position = 0;
    let mut replacements = Vec::new();

    while let Some(_) = result[position..].find("{\"") {
        let start = position + result[position..].find("{\"").unwrap();
        // Find the matching quote that isn't escaped
        let mut quote_end = None;
        let pos = start + 2;

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

                    // Skip if there are spaces in the format string
                    if color.contains(' ') {
                        replacements.push((start..end + 1, unescaped.clone()));
                    } else {
                        let formatted_text = apply_format(&unescaped, color, show_warnings, mode)?;
                        replacements.push((start..end + 1, formatted_text));
                    }
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

        // Add ending sequence if there are any active color attributes
        // or if there are colored lines followed by non-colored lines
        let last_reset = final_result.rfind("%{\x1b[0m%}");
        let last_color = final_result.rfind("%{\x1b[");
        let lines: Vec<&str> = final_result.split("\\n").collect();
        let has_color_followed_by_plain = final_result.contains("%{\x1b[")
            && lines
                .last()
                .map_or(false, |last_line| !last_line.contains("%{\x1b["));

        if has_color_followed_by_plain
            || (last_color.is_some() && last_reset.map_or(true, |pos| pos < last_color.unwrap()))
        {
            final_result.push_str("%{\x1b[0m%}");
        }

        Ok(final_result)
    } else {
        // For non-tcsh mode, process line by line
        let lines: Vec<&str> = template.lines().collect();
        let line_count = lines.len();

        // Process each line
        let mut result_lines = Vec::with_capacity(line_count);
        for line in lines {
            let processed = process_variables(line, variables, show_warnings, mode)?;
            result_lines.push(processed);
        }

        let has_color = result_lines.iter().any(|line| line.contains("\x1b["));

        let mut result = if has_color {
            // For color templates, preserve all lines
            result_lines.join("\n")
        } else {
            // For non-color templates, filter out empty lines
            result_lines
                .into_iter()
                .filter(|line| !line.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        };

        // Add ending sequence if there are any active color attributes
        // or if there are colored lines followed by non-colored lines
        let last_reset = result.rfind("\x1b[0m");
        let last_color = result.rfind("\x1b[");
        let has_color_followed_by_plain = result.contains("\x1b[")
            && result
                .lines()
                .rev()
                .next()
                .map_or(false, |last_line| !last_line.contains("\x1b["));

        if has_color_followed_by_plain
            || (last_color.is_some() && last_reset.map_or(true, |pos| pos < last_color.unwrap()))
        {
            result.push_str("\x1b[0m");
        }

        Ok(result)
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
    fn test_noncolor_substitution(
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
    fn test_noncolor_multiline(
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

    #[rstest]
    // Basic colors
    #[case::red_var("{var:red}", "\u{1b}[31mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::green_var("{var:green}", "\u{1b}[32mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::yellow_var("{var:yellow}", "\u{1b}[33mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::blue_var("{var:blue}", "\u{1b}[34mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::magenta_var("{var:magenta}", "\u{1b}[35mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::cyan_var("{var:cyan}", "\u{1b}[36mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::white_var("{var:white}", "\u{1b}[37mvalue\u{1b}[0m", vec![("var", "value")])]
    // Bright colors
    #[case::bright_red("{var:bright_red}", "\u{1b}[1;31mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::bright_green("{var:bright_green}", "\u{1b}[1;32mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::bright_yellow("{var:bright_yellow}", "\u{1b}[1;33mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::bright_blue("{var:bright_blue}", "\u{1b}[1;34mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::bright_magenta("{var:bright_magenta}", "\u{1b}[1;35mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::bright_cyan("{var:bright_cyan}", "\u{1b}[1;36mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::bright_white("{var:bright_white}", "\u{1b}[1;37mvalue\u{1b}[0m", vec![("var", "value")])]
    // Styles
    #[case::bold_text("{var:bold}", "\u{1b}[1mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::italic_text("{var:italic}", "\u{1b}[3mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::normal_text("{var:normal}", "\u{1b}[0mvalue\u{1b}[0m", vec![("var", "value")])]
    // Multiple formats
    #[case::bold_red("{var:red,bold}", "\u{1b}[31;1mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::italic_blue("{var:blue,italic}", "\u{1b}[34;3mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::bright_bold("{var:bright_green,bold}", "\u{1b}[1;32;1mvalue\u{1b}[0m", vec![("var", "value")])]
    // Complex combinations
    #[case::multiple_vars_colors("({a:red}){b:blue}", "(\u{1b}[31m1\u{1b}[0m)\u{1b}[34m2\u{1b}[0m", vec![("a", "1"), ("b", "2")])]
    #[case::quoted_with_color("{\"→\":cyan} {var:red}", "\u{1b}[36m→\u{1b}[0m \u{1b}[31mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::env_var_color("{$USER:green}", "\u{1b}[32mtestuser\u{1b}[0m", vec![("$USER", "testuser")])]
    #[case::mixed_styles("{var:bold,red,italic}", "\u{1b}[1;31;3mvalue\u{1b}[0m", vec![("var", "value")])]
    // Invalid/Edge cases
    #[case::unknown_color("{var:unknown}", "\u{1b}[37mvalue\u{1b}[0m", vec![("var", "value")])] // Should default to white
    #[case::empty_color("{var:}", "value", vec![("var", "value")])] // No color specified
    #[case::multiple_same_var("{var:red} and {var:blue}", "\u{1b}[31mvalue\u{1b}[0m and \u{1b}[34mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::nested_color_ignored("{{var:red}:blue}", "{value:blue}", vec![("var", "value")])] // Nested format ignored
    #[case::space_in_format("{var: red }", "value", vec![("var", "value")])] // Spaces in format ignored
    // Tests for ending sequence behavior
    #[case::color_then_plain("{var:red} plain", "\u{1b}[31mvalue\u{1b}[0m plain", vec![("var", "value")])]
    #[case::plain_then_color("plain {var:red}", "plain \u{1b}[31mvalue\u{1b}[0m", vec![("var", "value")])]
    #[case::multiple_colors_with_plain("{var1:red} plain {var2:blue}", "\u{1b}[31mvalue1\u{1b}[0m plain \u{1b}[34mvalue2\u{1b}[0m", vec![("var1", "value1"), ("var2", "value2")])]
    #[case::multiple_styles_ending("{var:bold,red} plain", "\u{1b}[1;31mvalue\u{1b}[0m plain", vec![("var", "value")])]
    #[case::multiple_vars_same_color("{var1:red}{var2:red}", "\u{1b}[31mvalue1\u{1b}[0m\u{1b}[31mvalue2\u{1b}[0m", vec![("var1", "value1"), ("var2", "value2")])]
    fn test_color_substitution(
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
    // Basic multi-line colors
    #[case::multiline_red("line1\n{var:red}", "line1\n\u{1b}[31mvalue\u{1b}[0m", vec![("var", "value")], None)]
    #[case::multiline_green_blue("line1\n{var1:green}\nline2\n{var2:blue}", "line1\n\u{1b}[32mvalue1\u{1b}[0m\nline2\n\u{1b}[34mvalue2\u{1b}[0m", vec![("var1", "value1"), ("var2", "value2")], None)]
    #[case::multiline_all_colors(
        "{var1:red}\n{var2:green}\n{var3:blue}\n{var4:yellow}\n{var5:magenta}\n{var6:cyan}\n{var7:white}",
        "\u{1b}[31mvalue\u{1b}[0m\n\u{1b}[32mvalue\u{1b}[0m\n\u{1b}[34mvalue\u{1b}[0m\n\u{1b}[33mvalue\u{1b}[0m\n\u{1b}[35mvalue\u{1b}[0m\n\u{1b}[36mvalue\u{1b}[0m\n\u{1b}[37mvalue\u{1b}[0m",
        vec![("var1", "value"), ("var2", "value"), ("var3", "value"), ("var4", "value"), ("var5", "value"), ("var6", "value"), ("var7", "value")],
        None
    )]
    // Mixed styles and colors
    #[case::multiline_bold_colors(
        "{var1:bold,red}\n{var2:bold,green}\n{var3:bold,blue}",
        "\u{1b}[1;31mvalue\u{1b}[0m\n\u{1b}[1;32mvalue\u{1b}[0m\n\u{1b}[1;34mvalue\u{1b}[0m",
        vec![("var1", "value"), ("var2", "value"), ("var3", "value")],
        None
    )]
    #[case::multiline_italic_colors(
        "{var1:italic,red}\n{var2:italic,green}\n{var3:italic,blue}",
        "\u{1b}[3;31mvalue\u{1b}[0m\n\u{1b}[3;32mvalue\u{1b}[0m\n\u{1b}[3;34mvalue\u{1b}[0m",
        vec![("var1", "value"), ("var2", "value"), ("var3", "value")],
        None
    )]
    // Bright colors
    #[case::multiline_bright_colors(
        "{var1:bright_red}\n{var2:bright_green}\n{var3:bright_blue}",
        "\u{1b}[1;31mvalue\u{1b}[0m\n\u{1b}[1;32mvalue\u{1b}[0m\n\u{1b}[1;34mvalue\u{1b}[0m",
        vec![("var1", "value"), ("var2", "value"), ("var3", "value")],
        None
    )]
    // Mixed with plain text
    #[case::multiline_mixed_plain(
        "plain1\n{var1:red}\nplain2\n{var2:blue}\nplain3",
        "plain1\n\u{1b}[31mvalue\u{1b}[0m\nplain2\n\u{1b}[34mvalue\u{1b}[0m\nplain3\u{1b}[0m",
        vec![("var1", "value"), ("var2", "value")],
        None
    )]
    // With indentation
    #[case::multiline_indented(
        "  {var1:red}\n    {var2:blue}\n      {var3:green}",
        "  \u{1b}[31mvalue\u{1b}[0m\n    \u{1b}[34mvalue\u{1b}[0m\n      \u{1b}[32mvalue\u{1b}[0m",
        vec![("var1", "value"), ("var2", "value"), ("var3", "value")],
        None
    )]
    // With quoted text
    #[case::multiline_quoted(
        "{\"text1\":red}\n{\"text2\":blue}\n{\"text3\":green}",
        "\u{1b}[31mtext1\u{1b}[0m\n\u{1b}[34mtext2\u{1b}[0m\n\u{1b}[32mtext3\u{1b}[0m",
        vec![],
        None
    )]
    // Mixed variables and quoted text
    #[case::multiline_mixed_quoted(
        "{var1:red}\n{\"text\":blue}\n{var2:green}",
        "\u{1b}[31mvalue1\u{1b}[0m\n\u{1b}[34mtext\u{1b}[0m\n\u{1b}[32mvalue2\u{1b}[0m",
        vec![("var1", "value1"), ("var2", "value2")],
        None
    )]
    // With empty lines
    #[case::multiline_empty_lines(
        "{var1:red}\n\n{var2:blue}\n\n{var3:green}",
        "\u{1b}[31mvalue\u{1b}[0m\n\n\u{1b}[34mvalue\u{1b}[0m\n\n\u{1b}[32mvalue\u{1b}[0m",
        vec![("var1", "value"), ("var2", "value"), ("var3", "value")],
        None
    )]
    // With multiple variables per line
    #[case::multiline_multiple_per_line(
        "{var1:red} and {var2:blue}\n{var3:green} and {var4:yellow}",
        "\u{1b}[31mvalue\u{1b}[0m and \u{1b}[34mvalue\u{1b}[0m\n\u{1b}[32mvalue\u{1b}[0m and \u{1b}[33mvalue\u{1b}[0m",
        vec![("var1", "value"), ("var2", "value"), ("var3", "value"), ("var4", "value")],
        None
    )]
    // With nested braces (should be ignored)
    #[case::multiline_nested_braces(
        "{{var1:red}}\n{{var2:blue}}\n{{var3:green}}",
        "{value}\n{value}\n{value}",
        vec![("var1", "value"), ("var2", "value"), ("var3", "value")],
        None
    )]
    // With spaces in format (should be ignored)
    #[case::multiline_spaced_format(
        "{var1: red }\n{var2: blue }\n{var3: green }",
        "value\nvalue\nvalue",
        vec![("var1", "value"), ("var2", "value"), ("var3", "value")],
        None
    )]
    // With unknown colors (should default to white)
    #[case::multiline_unknown_colors(
        "{var1:unknown1}\n{var2:unknown2}\n{var3:unknown3}",
        "\u{1b}[37mvalue\u{1b}[0m\n\u{1b}[37mvalue\u{1b}[0m\n\u{1b}[37mvalue\u{1b}[0m",
        vec![("var1", "value"), ("var2", "value"), ("var3", "value")],
        None
    )]
    // With tcsh mode
    #[case::multiline_tcsh_mode(
        "{var1:red}\n{var2:blue}",
        "%{\x1b[31m%}value%{\x1b[0m%}\\n%{\x1b[34m%}value%{\x1b[0m%}",
        vec![("var1", "value"), ("var2", "value")],
        Some("tcsh")
    )]
    // With Unicode characters
    #[case::multiline_unicode(
        "{\"→\":red}\n{\"←\":blue}\n{\"↔\":green}",
        "\u{1b}[31m→\u{1b}[0m\n\u{1b}[34m←\u{1b}[0m\n\u{1b}[32m↔\u{1b}[0m",
        vec![],
        None
    )]
    // With escaped quotes
    #[case::multiline_escaped_quotes(
        "{\"\\\"text1\\\"\":red}\n{\"\\\"text2\\\"\":blue}",
        "\u{1b}[31m\"text1\"\u{1b}[0m\n\u{1b}[34m\"text2\"\u{1b}[0m",
        vec![],
        None
    )]
    // Complex mixed case
    #[case::multiline_complex_mixed(
        "Title: {var1:bold,red}\nSubtitle: {\"→\":blue}\nContent: {var2:italic,green}",
        "Title: \u{1b}[1;31mvalue1\u{1b}[0m\nSubtitle: \u{1b}[34m→\u{1b}[0m\nContent: \u{1b}[3;32mvalue2\u{1b}[0m",
        vec![("var1", "value1"), ("var2", "value2")],
        None
    )]
    // With multiple styles
    #[case::multiline_multiple_styles(
        "{var1:bold,italic,red}\n{var2:bold,italic,blue}",
        "\u{1b}[1;3;31mvalue\u{1b}[0m\n\u{1b}[1;3;34mvalue\u{1b}[0m",
        vec![("var1", "value"), ("var2", "value")],
        None
    )]
    // Tests for multiline ending sequence behavior
    #[case::multiline_color_then_plain(
        "{var:red}\nplain text",
        "\u{1b}[31mvalue\u{1b}[0m\nplain text\u{1b}[0m",
        vec![("var", "value")],
        None
    )]
    #[case::multiline_mixed_ending(
        "{var1:red}\nplain\n{var2:blue}\nmore plain",
        "\u{1b}[31mvalue1\u{1b}[0m\nplain\n\u{1b}[34mvalue2\u{1b}[0m\nmore plain\u{1b}[0m",
        vec![("var1", "value1"), ("var2", "value2")],
        None
    )]
    #[case::multiline_style_color_mix(
        "{var1:bold}\n{var2:red}\n{var3:italic,blue}",
        "\u{1b}[1mvalue1\u{1b}[0m\n\u{1b}[31mvalue2\u{1b}[0m\n\u{1b}[3;34mvalue3\u{1b}[0m",
        vec![("var1", "value1"), ("var2", "value2"), ("var3", "value3")],
        None
    )]
    #[case::multiline_mixed_colors_with_hash(
        "{var1:red} {var2:blue} {var3:green}\n#",
        "\u{1b}[31mvalue1\u{1b}[0m \u{1b}[34mvalue2\u{1b}[0m \u{1b}[32mvalue3\u{1b}[0m\n#\u{1b}[0m",
        vec![("var1", "value1"), ("var2", "value2"), ("var3", "value3")],
        None
    )]
    #[case::multiline_mixed_colors_with_hash_space(
        "{var1:red} {var2:blue} {var3:green}\n# ",
        "\u{1b}[31mvalue1\u{1b}[0m \u{1b}[34mvalue2\u{1b}[0m \u{1b}[32mvalue3\u{1b}[0m\n# \u{1b}[0m",
        vec![("var1", "value1"), ("var2", "value2"), ("var3", "value3")],
        None
    )]
    fn test_multiline_color_substitution(
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
