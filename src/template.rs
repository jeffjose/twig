//! The prompt templates.
//!
//! ```text
//! {name}              a variable
//! {name:green,bold}   a variable with a style
//! {"text":cyan}       text with a style
//! {$HOME}             an environment variable of the shell
//! [ ... ]             a group: left out when every variable in it is empty
//! \{ \} \[ \] \\      the characters themselves
//! ```
//!
//! A style is a list of words separated by commas or spaces: a color
//! (`red`, `bright_blue`, `208`, `#ff8800`), `bg:COLOR`, `bold`, `dim`,
//! `italic`, `underline`, or `host` for the color of this machine.

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Text(String),
    Var { name: String, style: Option<Style> },
    Styled { text: String, style: Style },
    Group(Vec<Node>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    Basic(u8),
    Index(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    /// Take the foreground color from the machine's entry in [hosts].
    pub host: bool,
}

fn color(name: &str) -> Option<Color> {
    const NAMES: [&str; 8] = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
    if let Some(i) = NAMES.iter().position(|&n| n == name) {
        return Some(Color::Basic(i as u8));
    }
    if let Some(rest) = name.strip_prefix("bright_") {
        return NAMES.iter().position(|&n| n == rest).map(|i| Color::Basic(i as u8 + 8));
    }
    match name {
        "gray" | "grey" => return Some(Color::Basic(8)),
        "orange" => return Some(Color::Index(208)),
        "violet" | "purple" => return Some(Color::Index(141)),
        "pink" => return Some(Color::Index(205)),
        _ => {}
    }
    if let Some(hex) = name.strip_prefix('#') {
        if hex.len() == 6 {
            let v = u32::from_str_radix(hex, 16).ok()?;
            return Some(Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8));
        }
        return None;
    }
    name.parse::<u8>().ok().map(Color::Index)
}

impl Style {
    pub fn parse(text: &str) -> Result<Style, String> {
        let mut s = Style::default();
        for word in text.split([',', ' ']).map(str::trim).filter(|w| !w.is_empty()) {
            match word {
                "bold" => s.bold = true,
                "dim" => s.dim = true,
                "italic" => s.italic = true,
                "underline" => s.underline = true,
                "host" => s.host = true,
                "none" | "plain" => {}
                _ => {
                    if let Some(bg) = word.strip_prefix("bg:") {
                        s.bg = Some(color(bg).ok_or_else(|| format!("unknown color \"{bg}\""))?);
                    } else {
                        s.fg = Some(color(word).ok_or_else(|| format!("unknown style \"{word}\"; try a color, bold, dim, italic, underline or host"))?);
                    }
                }
            }
        }
        Ok(s)
    }

    /// For styles that are known to be valid (the built-in defaults).
    pub fn of(text: &str) -> Style {
        Style::parse(text).unwrap_or_default()
    }

    /// Fill in a host color.
    pub fn resolve(mut self, host_color: Option<Color>) -> Style {
        if self.host {
            self.fg = host_color.or(self.fg);
            self.host = false;
        }
        self
    }

    pub fn ansi(&self) -> String {
        let mut codes: Vec<String> = Vec::new();
        if self.bold {
            codes.push("1".into());
        }
        if self.dim {
            codes.push("2".into());
        }
        if self.italic {
            codes.push("3".into());
        }
        if self.underline {
            codes.push("4".into());
        }
        let mut push = |c: Color, base: u8| match c {
            Color::Basic(n) if n < 8 => codes.push((base + n).to_string()),
            Color::Basic(n) => codes.push((base + 60 + n - 8).to_string()),
            Color::Index(n) => codes.push(format!("{};5;{n}", base + 8)),
            Color::Rgb(r, g, b) => codes.push(format!("{};2;{r};{g};{b}", base + 8)),
        };
        if let Some(c) = self.fg {
            push(c, 30);
        }
        if let Some(c) = self.bg {
            push(c, 40);
        }
        if codes.is_empty() { String::new() } else { format!("\x1b[{}m", codes.join(";")) }
    }

    pub fn paint(&self, text: &str) -> String {
        let a = self.ansi();
        if a.is_empty() || text.is_empty() { text.to_string() } else { format!("{a}{text}\x1b[0m") }
    }
}

pub fn parse(src: &str) -> Result<Vec<Node>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let nodes = parse_seq(&chars, &mut i, false)?;
    if i < chars.len() {
        return Err("a ] without a [ (write \\] for the character)".into());
    }
    Ok(nodes)
}

fn parse_seq(c: &[char], i: &mut usize, in_group: bool) -> Result<Vec<Node>, String> {
    let mut nodes = Vec::new();
    let mut text = String::new();
    let flush = |text: &mut String, nodes: &mut Vec<Node>| {
        if !text.is_empty() {
            nodes.push(Node::Text(std::mem::take(text)));
        }
    };
    while *i < c.len() {
        match c[*i] {
            '\\' if *i + 1 < c.len() && "{}[]\\".contains(c[*i + 1]) => {
                text.push(c[*i + 1]);
                *i += 2;
            }
            '[' => {
                flush(&mut text, &mut nodes);
                *i += 1;
                let inner = parse_seq(c, i, true)?;
                if *i >= c.len() {
                    return Err("a [ without a ] (write \\[ for the character)".into());
                }
                *i += 1;
                nodes.push(Node::Group(inner));
            }
            ']' if in_group => break,
            ']' => break,
            '{' => {
                flush(&mut text, &mut nodes);
                let start = *i;
                let mut j = *i + 1;
                let mut in_quote = false;
                while j < c.len() && (in_quote || c[j] != '}') {
                    if c[j] == '"' {
                        in_quote = !in_quote;
                    }
                    j += 1;
                }
                if j >= c.len() {
                    return Err(format!("a {{ without a }} at \"{}\"", c[start..].iter().take(20).collect::<String>()));
                }
                let body: String = c[start + 1..j].iter().collect();
                *i = j + 1;
                nodes.push(parse_braces(&body)?);
            }
            ch => {
                text.push(ch);
                *i += 1;
            }
        }
    }
    flush(&mut text, &mut nodes);
    Ok(nodes)
}

fn parse_braces(body: &str) -> Result<Node, String> {
    if let Some(rest) = body.strip_prefix('"') {
        let end = rest.find('"').ok_or_else(|| format!("unclosed quote in {{{body}}}"))?;
        let text = rest[..end].to_string();
        let style = match rest[end + 1..].strip_prefix(':') {
            Some(s) => Style::parse(s)?,
            None if rest[end + 1..].trim().is_empty() => Style::default(),
            None => return Err(format!("expected : after the text in {{{body}}}")),
        };
        return Ok(Node::Styled { text, style });
    }
    let (name, style) = match body.split_once(':') {
        Some((n, s)) => (n.trim(), Some(Style::parse(s)?)),
        None => (body.trim(), None),
    };
    let valid = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    let bare = name.strip_prefix('$').unwrap_or(name);
    if !valid(bare) {
        return Err(format!("\"{{{body}}}\" is not a variable name"));
    }
    Ok(Node::Var { name: name.to_string(), style })
}

/// The names of all the variables a template uses.
pub fn variables(nodes: &[Node], out: &mut Vec<String>) {
    for n in nodes {
        match n {
            Node::Var { name, .. } => out.push(name.clone()),
            Node::Group(inner) => variables(inner, out),
            _ => {}
        }
    }
}

/// Something that knows the value of variables: the text, and the style it
/// has when the template doesn't give one.
pub trait Vars {
    fn get(&self, name: &str) -> Option<(String, Style)>;
    fn host_color(&self) -> Option<Color>;
}

pub fn render(nodes: &[Node], vars: &dyn Vars) -> String {
    let mut out = String::new();
    render_into(nodes, vars, &mut out);
    out
}

/// Returns (had a variable, some variable had a value).
fn render_into(nodes: &[Node], vars: &dyn Vars, out: &mut String) -> (bool, bool) {
    let (mut any_var, mut any_value) = (false, false);
    for n in nodes {
        match n {
            Node::Text(t) => out.push_str(t),
            Node::Styled { text, style } => out.push_str(&style.resolve(vars.host_color()).paint(text)),
            Node::Var { name, style } => {
                any_var = true;
                if let Some((value, default)) = vars.get(name) {
                    if !value.is_empty() {
                        any_value = true;
                        let st = style.unwrap_or(default).resolve(vars.host_color());
                        out.push_str(&st.paint(&value));
                    }
                }
            }
            Node::Group(inner) => {
                let mut buf = String::new();
                let (v, val) = render_into(inner, vars, &mut buf);
                if !v || val {
                    out.push_str(&buf);
                }
                any_var |= v;
                any_value |= val;
            }
        }
    }
    (any_var, any_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct M(HashMap<&'static str, &'static str>);
    impl Vars for M {
        fn get(&self, name: &str) -> Option<(String, Style)> {
            self.0.get(name).map(|v| (v.to_string(), Style::default()))
        }
        fn host_color(&self) -> Option<Color> {
            Some(Color::Basic(4))
        }
    }

    fn r(t: &str, vars: &[(&'static str, &'static str)]) -> String {
        render(&parse(t).unwrap(), &M(vars.iter().cloned().collect()))
    }

    #[test]
    fn groups_vanish_when_empty() {
        assert_eq!(r("a[ {x}]b", &[]), "ab");
        assert_eq!(r("a[ {x}]b", &[("x", "1")]), "a 1b");
        assert_eq!(r("[({x}[:{y}])]", &[("x", "m")]), "(m)");
        assert_eq!(r("[({x}[:{y}])]", &[("x", "m"), ("y", "2")]), "(m:2)");
        assert_eq!(r("[lit]", &[]), "lit");
    }

    #[test]
    fn styles() {
        assert_eq!(r("{x:green,bold}", &[("x", "v")]), "\x1b[1;32mv\x1b[0m");
        assert_eq!(r("{\"$\":white}", &[]), "\x1b[37m$\x1b[0m");
        assert_eq!(r("{x:host}", &[("x", "h")]), "\x1b[34mh\x1b[0m");
        assert_eq!(Style::of("208 bg:#010203").ansi(), "\x1b[38;5;208;48;2;1;2;3m");
        assert_eq!(Style::of("bright_red").ansi(), "\x1b[91m");
        assert!(Style::parse("blurple").is_err());
    }

    #[test]
    fn escapes_and_errors() {
        assert_eq!(r("\\[{x}\\]", &[("x", "1")]), "[1]");
        assert!(parse("{x").is_err());
        assert!(parse("[x").is_err());
        assert!(parse("x]").is_err());
        assert!(parse("{a b}").is_err());
        assert_eq!(parse("{$HOME}").unwrap(), vec![Node::Var { name: "$HOME".into(), style: None }]);
    }
}
