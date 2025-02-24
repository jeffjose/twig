use colored::*;
use serde::{Deserialize, Serialize};

pub fn print_color_test() {
    println!("Basic colors:");
    println!("red: {}", "Hello World".red());
    println!("green: {}", "Hello World".green());
    println!("yellow: {}", "Hello World".yellow());
    println!("blue: {}", "Hello World".blue());
    println!("magenta: {}", "Hello World".magenta());
    println!("cyan: {}", "Hello World".cyan());
    println!("white: {}", "Hello World".white());

    println!("\nBright colors:");
    println!("bright_red: {}", "Hello World".bright_red());
    println!("bright_green: {}", "Hello World".bright_green());
    println!("bright_yellow: {}", "Hello World".bright_yellow());
    println!("bright_blue: {}", "Hello World".bright_blue());
    println!("bright_magenta: {}", "Hello World".bright_magenta());
    println!("bright_cyan: {}", "Hello World".bright_cyan());
    println!("bright_white: {}", "Hello World".bright_white());

    println!("\nStyles:");
    println!("bold: {}", "Hello World".bold());
    println!("italic: {}", "Hello World".italic());
    println!("dimmed: {}", "Hello World".dimmed());
    println!("underline: {}", "Hello World".underline());
    println!("reversed: {}", "Hello World".reversed());

    println!("\nCombinations:");
    println!("red+bold: {}", "Hello World".red().bold());
    println!("blue+italic: {}", "Hello World".blue().italic());
    println!("bright_green+bold: {}", "Hello World".bright_green().bold());
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ColorCondition {
    pub value: Option<String>,
    pub color: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ColorConfig {
    #[serde(default)]
    pub colors: Vec<ColorCondition>,
}

impl ColorConfig {
    #[cfg(test)]
    pub fn get_color_for_value(&self, value: &str) -> Option<&str> {
        // First try exact matches
        for condition in &self.colors {
            if let Some(pattern) = &condition.value {
                if pattern == value {
                    return Some(&condition.color);
                }
            }
        }

        // Then try pattern matches
        for condition in &self.colors {
            if let Some(pattern) = &condition.value {
                if pattern.contains('*') {
                    if glob::Pattern::new(pattern)
                        .ok()
                        .map(|p| p.matches(value))
                        .unwrap_or(false)
                    {
                        return Some(&condition.color);
                    }
                }
            }
        }

        // Finally look for a default
        self.colors
            .iter()
            .find(|c| c.value.is_none())
            .map(|c| c.color.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_exact_match() {
        let config: ColorConfig = serde_json::from_value(json!({
            "colors": [
                {"value": "skyfall", "color": "blue"},
                {"value": "nomad", "color": "green"},
                {"color": "white"}
            ]
        }))
        .unwrap();

        assert_eq!(config.get_color_for_value("skyfall"), Some("blue"));
        assert_eq!(config.get_color_for_value("nomad"), Some("green"));
        assert_eq!(config.get_color_for_value("unknown"), Some("white"));
    }

    #[test]
    fn test_pattern_match() {
        let config: ColorConfig = serde_json::from_value(json!({
            "colors": [
                {"value": "dev-*", "color": "yellow"},
                {"value": "prod-*", "color": "red"},
                {"color": "white"}
            ]
        }))
        .unwrap();

        assert_eq!(config.get_color_for_value("dev-1"), Some("yellow"));
        assert_eq!(config.get_color_for_value("prod-main"), Some("red"));
        assert_eq!(config.get_color_for_value("staging"), Some("white"));
    }

    #[test]
    fn test_no_conditions() {
        let config = ColorConfig::default();
        assert_eq!(config.get_color_for_value("anything"), None);
    }
}
