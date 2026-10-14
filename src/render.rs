//! Everything a prompt can show, gathered for one shell, and the choice
//! between the wide and the narrow layout.

use crate::config::Loaded;
use crate::git::GitStatus;
use crate::notices::Level;
use crate::template::{self, Color, Style, Vars};
use crate::util::{fmt_duration, strftime, tilde, visible_width};
use std::collections::HashMap;

pub struct Ctx<'a> {
    pub cfg: &'a Loaded,
    pub host: &'a str,
    pub fqdn: &'a str,
    pub cwd: &'a str,
    pub env: &'a HashMap<String, String>,
    pub status: i32,
    pub duration_ms: Option<u64>,
    pub time_secs: i64,
    pub facts: &'a HashMap<String, String>,
    /// The branch alone (read from .git/HEAD), when there is no status yet.
    pub branch: Option<String>,
    pub git: Option<&'a GitStatus>,
    /// The notice to show, and how many more there are.
    pub notice: Option<(String, Level, usize)>,
    pub jobs: String,
}

impl Vars for Ctx<'_> {
    fn host_color(&self) -> Option<Color> {
        self.cfg.host_style(self.fqdn, self.host).and_then(|s| s.fg)
    }

    fn get(&self, name: &str) -> Option<(String, Style)> {
        let plain = Style::default();
        if let Some(var) = name.strip_prefix('$') {
            return self.env.get(var).map(|v| (v.clone(), plain));
        }
        if name.starts_with("git") {
            return match (self.git, name) {
                (Some(g), _) => g.var(name),
                (None, "git_branch") => self.branch.clone().map(|b| (b, plain)),
                _ => None,
            };
        }
        let v = match name {
            "time" => strftime(&self.cfg.raw.prompt.time, self.time_secs),
            "host" => return Some((self.host.to_string(), self.cfg.host_style(self.fqdn, self.host).unwrap_or_default())),
            "fqdn" => self.fqdn.to_string(),
            "user" => self.env.get("USER").cloned().unwrap_or_default(),
            "cwd" => tilde(self.cwd, self.env.get("HOME").map(String::as_str).unwrap_or("")),
            "dir" => match self.cwd.rsplit('/').next() {
                Some("") | None => "/".into(),
                Some(d) => d.to_string(),
            },
            "symbol" => {
                let st = if self.status != 0 { self.cfg.symbol_error_style } else { self.cfg.symbol_style };
                return Some((self.cfg.raw.prompt.symbol.clone(), st));
            }
            "status" => {
                return (self.status != 0).then(|| (format!("✘{}", self.status), Style::of("red,bold")));
            }
            "duration" => {
                let d = self.duration_ms.filter(|&d| d >= self.cfg.duration_min.max(1))?;
                return Some((fmt_duration(d), Style::of("yellow")));
            }
            "notice" => {
                let (text, level, more) = self.notice.clone()?;
                let text = if more > 0 { format!("{text} (+{more})") } else { text };
                return Some((text, self.cfg.levels[level as usize]));
            }
            "jobs" => return (!self.jobs.is_empty()).then(|| (self.jobs.clone(), Style::of("cyan"))),
            "battery" => return self.facts.get("battery").map(|b| (b.clone(), Style::of("yellow"))),
            _ => return self.facts.get(name).map(|v| (v.clone(), plain)),
        };
        Some((v, plain))
    }
}

/// The prompt and the right prompt, as text with ANSI escapes. The narrow
/// layout is used when a line of the wide one doesn't fit.
///
/// tcsh doesn't draw `rprompt` with a prompt of more than one line, so
/// there the right part goes at the end of the line above the cursor,
/// when the width of the terminal is known and it fits.
pub fn render(ctx: &Ctx, cols: Option<u16>) -> (String, String) {
    let cfg = ctx.cfg;
    let mut prompt = template::render(&cfg.format, ctx);
    if let (Some(cols), Some(narrow)) = (cols, &cfg.narrow) {
        let widest = prompt.lines().map(visible_width).max().unwrap_or(0);
        if widest + cfg.raw.prompt.padding > cols as usize {
            prompt = template::render(narrow, ctx);
        }
    }
    let right = template::render(&cfg.right, ctx);
    if !prompt.contains('\n') {
        return (prompt, right);
    }
    if let (Some(cols), false) = (cols, right.is_empty()) {
        let mut lines: Vec<String> = prompt.split('\n').map(String::from).collect();
        let i = lines.len() - 2;
        let (lw, rw) = (visible_width(&lines[i]), visible_width(&right));
        // Two columns free at the end: tcsh::escape puts a space after a
        // closing escape before a newline, and a line that then fills the
        // terminal wraps onto an empty one.
        if lw + 2 + rw < cols as usize {
            lines[i] = format!("{}{}{right}", lines[i], " ".repeat(cols as usize - 2 - lw - rw));
            prompt = lines.join("\n");
        }
    }
    (prompt, String::new())
}

/// The line just above the one you type on, which is the line the daemon
/// may redraw while the shell waits.
pub fn info_line(prompt: &str) -> Option<&str> {
    let lines: Vec<&str> = prompt.split('\n').collect();
    (lines.len() >= 2).then(|| lines[lines.len() - 2])
}
