//! The settings, from ~/.config/twig/config.toml. Every setting has a
//! default, so the file only needs what you want different.

use crate::template::{self, Node, Style};
use crate::util::{glob, parse_duration};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::time::Duration;

/// Written by `twig setup` when there is no settings file yet.
pub const STARTER: &str = include_str!("../config.toml");

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub prompt: Prompt,
    /// Host name pattern (`*` and `?` allowed) to the color of that machine,
    /// for the `host` style. The full name is tried, then the short one.
    pub hosts: toml::Table,
    pub git: Git,
    pub live: Live,
    pub notices: Notices,
    pub disk: Disk,
    pub battery: Battery,
    pub units: Units,
    pub history: History,
    pub jobs: Jobs,
    pub probe: Vec<Probe>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Prompt {
    pub format: String,
    /// Used instead of `format` when that is wider than the terminal.
    pub narrow: String,
    /// At the right edge of the line above the cursor (or of the line you
    /// type on, for a prompt of one line).
    pub right: String,
    /// strftime format for {time}.
    pub time: String,
    pub symbol: String,
    pub symbol_style: String,
    /// The symbol's style after a command that failed.
    pub symbol_error_style: String,
    /// {duration} shows only for commands that took at least this long.
    pub duration_min: String,
    /// Columns to keep free on the right before switching to `narrow`.
    pub padding: usize,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Git {
    pub enabled: bool,
    /// How long a prompt waits for a fresh status before showing the last
    /// one; the newer one replaces it on screen when it is ready.
    pub wait: String,
    /// Fetch every repository a shell is in this often, to see what is new
    /// upstream. "off" to never fetch.
    pub fetch: String,
    /// Check a repository a shell is sitting in this often, to see changes
    /// made from elsewhere (an editor, another machine). "off" to not.
    pub poll: String,
    pub untracked: bool,
    /// What goes before each count in {git_staged} and the others.
    pub marks: Marks,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Marks {
    pub ahead: String,
    pub behind: String,
    pub staged: String,
    pub changed: String,
    pub untracked: String,
    pub conflicts: String,
    pub stash: String,
    pub clean: String,
    pub dirty: String,
}

impl Default for Marks {
    fn default() -> Self {
        Marks {
            ahead: "↑".into(),
            behind: "↓".into(),
            staged: "+".into(),
            changed: "~".into(),
            untracked: "?".into(),
            conflicts: "=".into(),
            stash: "≡".into(),
            clean: "✔".into(),
            dirty: "*".into(),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Live {
    /// Redraw the line above the one you type on while the shell waits for
    /// you, when something on it changes.
    pub enabled: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Notices {
    pub info: String,
    pub warn: String,
    pub error: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Disk {
    pub paths: Vec<String>,
    /// Percent used at which to say so.
    pub warn: u8,
    pub every: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Battery {
    /// Percent below which, on battery, to say so.
    pub low: u8,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Units {
    /// Say when a systemd unit (system or user) has failed.
    pub enabled: bool,
    pub every: String,
    /// Units never to mention (names, with or without .service; * and ?
    /// allowed): ones that fail and are meant to.
    pub ignore: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct History {
    /// How many commands a new shell loads for the arrow keys.
    pub size: usize,
    /// Commands not to keep (patterns with * and ?).
    pub ignore: Vec<String>,
    /// The key that opens the history search, as bindkey writes it.
    pub key: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Jobs {
    /// Show a desktop notification for jobs that took at least this long.
    pub desktop_after: String,
    /// How many finished jobs to keep the output of.
    pub keep: usize,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    /// The variable it sets, {name}: the first line the command prints.
    pub name: String,
    /// A command for sh -c.
    pub run: String,
    #[serde(default = "minute")]
    pub every: String,
    /// Only on these machines (patterns); every machine when empty.
    #[serde(default)]
    pub hosts: Vec<String>,
    /// A notice to show while the command fails; {value} is its output.
    #[serde(default)]
    pub notice: Option<String>,
    #[serde(default = "warn")]
    pub level: String,
    #[serde(default = "ten_seconds")]
    pub timeout: String,
}

fn minute() -> String {
    "1m".into()
}
fn warn() -> String {
    "warn".into()
}
fn ten_seconds() -> String {
    "10s".into()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            prompt: Prompt::default(),
            hosts: toml::Table::new(),
            git: Git::default(),
            live: Live::default(),
            notices: Notices::default(),
            disk: Disk::default(),
            battery: Battery::default(),
            units: Units::default(),
            history: History::default(),
            jobs: Jobs::default(),
            probe: Vec::new(),
        }
    }
}

impl Default for Prompt {
    fn default() -> Self {
        Prompt {
            format: "{time:cyan} {host} {cwd:green,bold}[ {git_branch:magenta}[ {git}]][ {jobs}][ {notice}]\n{symbol} ".into(),
            narrow: "{time:cyan} {host}\n{cwd:green,bold}[ {git_branch:magenta}[ {git}]][\n{jobs}][\n{notice}]\n{symbol} ".into(),
            right: "[{duration}][ {status}]".into(),
            time: "%H:%M:%S".into(),
            symbol: "%".into(),
            symbol_style: "bold".into(),
            symbol_error_style: "red,bold".into(),
            duration_min: "2s".into(),
            padding: 2,
        }
    }
}

impl Default for Git {
    fn default() -> Self {
        Git { enabled: true, wait: "80ms".into(), fetch: "off".into(), poll: "1m".into(), untracked: true, marks: Marks::default() }
    }
}

impl Default for Live {
    fn default() -> Self {
        Live { enabled: true }
    }
}

impl Default for Notices {
    fn default() -> Self {
        Notices { info: "cyan".into(), warn: "yellow,bold".into(), error: "red,bold".into() }
    }
}

impl Default for Disk {
    fn default() -> Self {
        Disk { paths: vec!["/".into(), "~".into()], warn: 90, every: "30s".into() }
    }
}

impl Default for Battery {
    fn default() -> Self {
        Battery { low: 15 }
    }
}

impl Default for Units {
    fn default() -> Self {
        Units { enabled: true, every: "1m".into(), ignore: Vec::new() }
    }
}

impl Default for History {
    fn default() -> Self {
        History { size: 10000, ignore: Vec::new(), key: "^R".into() }
    }
}

impl Default for Jobs {
    fn default() -> Self {
        Jobs { desktop_after: "30s".into(), keep: 100 }
    }
}

/// The settings with their templates and durations already parsed, so the
/// daemon does that once per change of the file rather than per prompt.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub raw: Config,
    pub format: Vec<Node>,
    pub narrow: Option<Vec<Node>>,
    pub right: Vec<Node>,
    pub symbol_style: Style,
    pub symbol_error_style: Style,
    pub duration_min: u64,
    pub git_wait: Duration,
    pub git_fetch: Option<Duration>,
    pub git_poll: Option<Duration>,
    pub disk_every: Duration,
    pub units_every: Option<Duration>,
    pub desktop_after: Option<Duration>,
    pub levels: [Style; 3],
    pub hosts: Vec<(String, Style)>,
    pub probes: Vec<(Probe, Duration, Duration)>,
}

/// An error in one line, for the prompt: where it is and what is wrong
/// ("line 120: unknown field `format_narrow`"), from the parser's message,
/// which has the line first and the reason last.
pub fn summary(err: &str, text: Option<&str>) -> String {
    if text.is_some_and(looks_old) {
        return "this is the old twig's settings file; see twig setup".into();
    }
    let lines: Vec<&str> = err.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let line = lines.first().and_then(|l| l.split("at line ").nth(1)).and_then(|l| l.split(',').next());
    let reason = lines.last().copied().unwrap_or("").split(", expected").next().unwrap_or("");
    match line {
        Some(n) if lines.len() > 1 => format!("line {n}: {reason}"),
        _ => reason.to_string(),
    }
}

/// The settings file of the twig before the rewrite, which used the same
/// path with other names.
pub fn looks_old(text: &str) -> bool {
    ["format_narrow", "{hostname", "{ip_address", "{git_status_", "width_threshold"].iter().any(|s| text.contains(s))
}

pub fn read(path: &Path) -> Result<Loaded, String> {
    match fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => load(Config::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

pub fn parse(text: &str) -> Result<Loaded, String> {
    let raw: Config = toml::from_str(text).map_err(|e| e.to_string())?;
    load(raw)
}

fn dur(what: &str, text: &str) -> Result<Option<Duration>, String> {
    parse_duration(text).map_err(|e| format!("{what}: {e}"))
}

fn tpl(what: &str, text: &str) -> Result<Vec<Node>, String> {
    template::parse(text).map_err(|e| format!("{what}: {e}"))
}

fn style(what: &str, text: &str) -> Result<Style, String> {
    Style::parse(text).map_err(|e| format!("{what}: {e}"))
}

pub fn load(raw: Config) -> Result<Loaded, String> {
    let p = &raw.prompt;
    let mut hosts = Vec::new();
    for (pattern, value) in &raw.hosts {
        let s = value.as_str().ok_or_else(|| format!("hosts.\"{pattern}\": give a color as a string"))?;
        hosts.push((pattern.clone(), style(&format!("hosts.\"{pattern}\""), s)?));
    }
    let mut probes = Vec::new();
    for pr in &raw.probe {
        let what = format!("probe \"{}\"", pr.name);
        if pr.name.is_empty() || !pr.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("{what}: a name is letters, digits and _"));
        }
        if !["info", "warn", "error"].contains(&pr.level.as_str()) {
            return Err(format!("{what}: level is info, warn or error"));
        }
        let every = dur(&what, &pr.every)?.ok_or_else(|| format!("{what}: every can't be off"))?;
        let timeout = dur(&what, &pr.timeout)?.unwrap_or(Duration::from_secs(10));
        probes.push((pr.clone(), every, timeout));
    }
    Ok(Loaded {
        format: tpl("prompt.format", &p.format)?,
        narrow: if p.narrow.is_empty() { None } else { Some(tpl("prompt.narrow", &p.narrow)?) },
        right: tpl("prompt.right", &p.right)?,
        symbol_style: style("prompt.symbol_style", &p.symbol_style)?,
        symbol_error_style: style("prompt.symbol_error_style", &p.symbol_error_style)?,
        duration_min: dur("prompt.duration_min", &p.duration_min)?.map_or(0, |d| d.as_millis() as u64),
        // Under the 400 ms a shell gives the daemon to answer at all.
        git_wait: dur("git.wait", &raw.git.wait)?.unwrap_or_default().min(Duration::from_millis(250)),
        git_fetch: dur("git.fetch", &raw.git.fetch)?,
        git_poll: dur("git.poll", &raw.git.poll)?,
        disk_every: dur("disk.every", &raw.disk.every)?.unwrap_or(Duration::from_secs(30)),
        units_every: if raw.units.enabled { dur("units.every", &raw.units.every)? } else { None },
        desktop_after: dur("jobs.desktop_after", &raw.jobs.desktop_after)?,
        levels: [
            style("notices.info", &raw.notices.info)?,
            style("notices.warn", &raw.notices.warn)?,
            style("notices.error", &raw.notices.error)?,
        ],
        hosts,
        probes,
        raw,
    })
}

impl Loaded {
    /// This machine's color from [hosts], trying the full name first.
    pub fn host_style(&self, fqdn: &str, short: &str) -> Option<Style> {
        for name in [fqdn, short] {
            if name.is_empty() {
                continue;
            }
            if let Some((_, s)) = self.hosts.iter().find(|(p, _)| glob(p, name)) {
                return Some(*s);
            }
        }
        None
    }

    pub fn probe_applies(&self, probe: &Probe, fqdn: &str, short: &str) -> bool {
        probe.hosts.is_empty() || probe.hosts.iter().any(|p| glob(p, fqdn) || glob(p, short))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_starter_load() {
        load(Config::default()).unwrap();
        parse(STARTER).unwrap();
        parse("").unwrap();
    }

    #[test]
    fn mistakes_are_reported() {
        assert!(parse("[prompt]\nformat = '{x'").unwrap_err().contains("prompt.format"));
        assert!(parse("[prompt]\nfromat = 'x'").is_err());
        assert!(parse("[git]\nwait = 'soon'").unwrap_err().contains("git.wait"));
        assert!(parse("[hosts]\nx = 'blurple'").is_err());
        assert!(parse("[[probe]]\nname = 'a b'\nrun = 'x'").is_err());
    }

    #[test]
    fn errors_in_one_line() {
        let e = parse("[prompt]\nformat = 'x'\n\nnarow = 'y'\n").unwrap_err();
        assert_eq!(summary(&e, None), "line 4: unknown field `narow`");
        let old = "[prompt]\nformat_narrow = '{cwd}'\n";
        assert!(summary(&parse(old).unwrap_err(), Some(old)).contains("old twig"));
        assert!(!looks_old(STARTER));
    }

    #[test]
    fn hosts_match_full_then_short() {
        let l = parse("[hosts]\n\"*.corp.example.com\" = 'red'\n\"box*\" = 'blue'").unwrap();
        assert_eq!(l.host_style("a.corp.example.com", "a"), Some(Style::of("red")));
        assert_eq!(l.host_style("box", "box"), Some(Style::of("blue")));
        assert_eq!(l.host_style("other", "other"), None);
    }
}
