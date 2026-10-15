//! Notices: short messages on the prompt that go away by themselves, after
//! a time, after being shown a number of times, when what they are about is
//! over (a full disk with room again), or when dismissed with `twig ack`.

use crate::client;
use crate::proto::Request;
use crate::util::{now_ms, parse_duration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info = 0,
    Warn = 1,
    Error = 2,
}

impl Level {
    pub fn parse(s: &str) -> Option<Level> {
        match s {
            "info" => Some(Level::Info),
            "warn" | "warning" => Some(Level::Warn),
            "error" | "err" => Some(Level::Error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Notice {
    pub id: u64,
    /// Notices with a key replace each other: a probe's notice is updated in
    /// place, not added again.
    pub key: Option<String>,
    pub text: String,
    pub level: Level,
    pub created: u64,
    /// When it goes away (ms since the epoch).
    pub expires: Option<u64>,
    /// How many prompts it is shown on, in each shell.
    pub shows: Option<u32>,
    /// Only for this shell.
    pub pid: Option<u32>,
    #[serde(skip)]
    pub seen: HashMap<u32, u32>,
}

#[derive(Default)]
pub struct Notices {
    pub list: Vec<Notice>,
    next: u64,
    /// key -> the level that was dismissed. A keyed notice stays dismissed
    /// until its condition clears or it gets more severe (a disk at 91%
    /// that reaches 98%), however its text changes meanwhile.
    acked: HashMap<String, Level>,
    /// Where the dismissals are kept, so they last past a restart of the
    /// daemon (none in tests).
    file: Option<std::path::PathBuf>,
    /// The last notices to go away, oldest first, for `twig notices`.
    pub past: std::collections::VecDeque<Past>,
    /// Patterns of notices never to show (`twig notices --ignore`), kept in
    /// the state directory.
    pub muted: Vec<String>,
}

fn muted_file() -> std::path::PathBuf {
    crate::paths::state_dir().join("ignored.json")
}

/// Whether a mute pattern matches a text (a notice, or a unit's name). A
/// pattern with `*` or `?` must match the whole text; one without needs
/// only to be in it. Case doesn't count.
pub fn mute_matches(pattern: &str, text: &str) -> bool {
    let (p, t) = (pattern.to_lowercase(), text.to_lowercase());
    if p.contains(['*', '?']) { crate::util::glob(&p, &t) } else { t.contains(&p) }
}

/// How many notices that have gone away are kept.
const PAST: usize = 50;

/// A notice that has gone away, and how: dismissed, expired, or cleared
/// (what it was about is over).
#[derive(Clone, Debug, Serialize)]
pub struct Past {
    pub id: u64,
    pub text: String,
    pub level: Level,
    pub created: u64,
    pub ended: u64,
    pub how: &'static str,
}

impl Notices {
    /// The daemon's notices, with the dismissals kept from before.
    pub fn load() -> Notices {
        let file = crate::paths::state_dir().join("dismissed.json");
        let acked = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let muted = std::fs::read(muted_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Notices { acked, muted, file: Some(file), ..Notices::default() }
    }

    /// Whether a text is one of those muted.
    pub fn is_muted(&self, text: &str) -> bool {
        self.muted.iter().any(|p| mute_matches(p, text))
    }

    /// Mute a pattern: what shows now and matches it goes, and what would
    /// come later doesn't show. Returns how many went.
    pub fn mute(&mut self, pattern: &str) -> usize {
        if !self.muted.iter().any(|p| p == pattern) {
            self.muted.push(pattern.to_string());
            self.save_muted();
        }
        self.retire("ignored", |n| mute_matches(pattern, &n.text))
    }

    pub fn unmute(&mut self, pattern: &str) -> bool {
        let before = self.muted.len();
        self.muted.retain(|p| p != pattern);
        self.save_muted();
        before != self.muted.len()
    }

    fn save_muted(&self) {
        if self.file.is_some() {
            if let Ok(b) = serde_json::to_vec(&self.muted) {
                let _ = crate::util::atomic_write(&muted_file(), &b);
            }
        }
    }

    fn save_acked(&self) {
        if let (Some(f), Ok(b)) = (&self.file, serde_json::to_vec(&self.acked)) {
            let _ = crate::util::atomic_write(f, &b);
        }
    }
}

#[derive(Debug)]
pub struct New {
    pub key: Option<String>,
    pub text: String,
    pub level: Level,
    pub ttl_ms: Option<u64>,
    pub shows: Option<u32>,
    pub pid: Option<u32>,
}

impl Notices {
    /// Add a notice or update the one with its key. Returns whether anything
    /// on screen would change.
    pub fn put(&mut self, n: New) -> bool {
        let now = now_ms();
        if self.is_muted(&n.text) {
            // Muted: not shown, and an earlier text under its key goes too.
            return n.key.is_some_and(|k| self.retire("ignored", |o| o.key.as_deref() == Some(&k)) > 0);
        }
        if let Some(k) = &n.key {
            if self.acked.get(k).is_some_and(|&l| n.level <= l) {
                return false;
            }
            if let Some(old) = self.list.iter_mut().find(|o| o.key.as_ref() == Some(k)) {
                if old.text == n.text && old.level == n.level {
                    return false;
                }
                old.text = n.text;
                old.level = n.level;
                old.expires = n.ttl_ms.map(|t| now + t);
                old.shows = n.shows;
                old.seen.clear();
                return true;
            }
        }
        self.next += 1;
        self.list.push(Notice {
            id: self.next,
            key: n.key,
            text: n.text,
            level: n.level,
            created: now,
            expires: n.ttl_ms.map(|t| now + t),
            shows: n.shows,
            pid: n.pid,
            seen: HashMap::new(),
        });
        true
    }

    /// Take the notices `gone` picks out of the list, into the ones past.
    fn retire(&mut self, how: &'static str, gone: impl Fn(&Notice) -> bool) -> usize {
        let now = now_ms();
        let (out, keep): (Vec<Notice>, Vec<Notice>) = std::mem::take(&mut self.list).into_iter().partition(|n| gone(n));
        self.list = keep;
        for n in &out {
            if self.past.len() >= PAST {
                self.past.pop_front();
            }
            self.past.push_back(Past { id: n.id, text: n.text.clone(), level: n.level, created: n.created, ended: now, how });
        }
        out.len()
    }

    /// The condition behind a keyed notice is over.
    pub fn clear(&mut self, key: &str) -> bool {
        if self.acked.remove(key).is_some() {
            self.save_acked();
        }
        self.retire("cleared", |n| n.key.as_deref() == Some(key)) > 0
    }

    pub fn expire(&mut self, now: u64) -> bool {
        self.retire("expired", |n| n.expires.is_some_and(|e| e <= now)) > 0
    }

    fn visible(n: &Notice, pid: u32, now: u64) -> bool {
        n.pid.is_none_or(|p| p == pid)
            && n.expires.is_none_or(|e| e > now)
            && n.shows.is_none_or(|s| n.seen.get(&pid).copied().unwrap_or(0) < s)
    }

    /// The most important notice for a shell, and how many others there are.
    pub fn top(&self, pid: u32, now: u64) -> Option<(String, Level, usize)> {
        let mut v: Vec<&Notice> = self.list.iter().filter(|n| Self::visible(n, pid, now)).collect();
        // A shell's own notices (its jobs) first, then the most severe, then
        // the newest.
        v.sort_by(|a, b| b.pid.is_some().cmp(&a.pid.is_some()).then(b.level.cmp(&a.level)).then(b.created.cmp(&a.created)));
        v.first().map(|n| (n.text.clone(), n.level, v.len() - 1))
    }

    /// A prompt with these notices has been drawn in a shell.
    pub fn shown(&mut self, pid: u32, now: u64) {
        for n in self.list.iter_mut() {
            if n.shows.is_some() && Self::visible(n, pid, now) {
                *n.seen.entry(pid).or_insert(0) += 1;
            }
        }
    }

    /// Dismiss one notice, or every one this shell can see.
    pub fn ack(&mut self, id: Option<u64>, pid: Option<u32>) -> usize {
        let now = now_ms();
        let hit = |n: &Notice| match id {
            Some(i) => n.id == i,
            None => pid.is_none_or(|p| Self::visible(n, p, now)),
        };
        for n in self.list.iter().filter(|n| hit(n)) {
            if let Some(k) = &n.key {
                self.acked.insert(k.clone(), n.level);
            }
        }
        let gone = self.retire("dismissed", hit);
        self.save_acked();
        gone
    }

    /// A shell has gone: its notices go to every shell.
    pub fn orphan(&mut self, pid: u32) {
        for n in self.list.iter_mut() {
            if n.pid == Some(pid) {
                n.pid = None;
            }
            n.seen.remove(&pid);
        }
    }
}

/// `twig notify [--warn|--error] [--for 10m] [--shows N] [--here] [--key K] TEXT`
pub fn notify_cmd(args: &[String]) -> i32 {
    let mut level = "info".to_string();
    let (mut ttl, mut shows, mut pid, mut key) = (None, None, None, None);
    let mut words = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--info" | "--warn" | "--error" => level = a[2..].to_string(),
            "--for" | "-t" => match it.next().map(|v| parse_duration(v)) {
                Some(Ok(d)) => ttl = d.map(|d| d.as_millis() as u64),
                _ => return usage_err("--for takes a duration, like 10m"),
            },
            "--shows" | "-n" => match it.next().and_then(|v| v.parse().ok()) {
                Some(n) => shows = Some(n),
                None => return usage_err("--shows takes a number"),
            },
            "--here" => pid = Some(unsafe { libc::getppid() } as u32),
            "--key" => key = it.next().cloned(),
            "--" => words.extend(it.by_ref().cloned()),
            s if s.starts_with("--") => return usage_err(&format!("unknown option {s}")),
            _ => words.push(a.clone()),
        }
    }
    let text = words.join(" ");
    if text.is_empty() {
        return usage_err("say what the notice is: twig notify backup done");
    }
    if ttl.is_none() && shows.is_none() && key.is_none() {
        ttl = Some(3_600_000);
    }
    let req = Request::Notify { text, level, ttl, shows, pid, key };
    match client::ask(&req) {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("twig: {e}");
            1
        }
    }
}

fn usage_err(msg: &str) -> i32 {
    eprintln!("twig notify: {msg}\n\nusage: twig notify [--warn|--error] [--for 10m] [--shows N] [--here] [--key KEY] TEXT");
    2
}

/// `twig notices`: the notices showing now, and the last ones to go away
/// (dismissed, expired, or cleared when what they were about was over).
pub fn list_cmd(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("--ignore" | "--unignore") => return mute_cmd(args),
        Some(a) => {
            eprintln!("twig notices: no option {a}\n\nusage: twig notices [--ignore PATTERN|ID] [--unignore PATTERN]");
            return 2;
        }
        None => {}
    }
    let v = match client::ask(&Request::Notices) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("twig: {e}");
            return 1;
        }
    };
    let color = unsafe { libc::isatty(1) } == 1;
    let paint = |s: &str, st: &str| if color { crate::template::Style::of(st).paint(s) } else { s.to_string() };
    let dim = |s: &str| paint(s, "dim");
    let by_level = |l: &serde_json::Value| match l.as_str() {
        Some("error") => "red",
        Some("warn") => "yellow",
        _ => "cyan",
    };
    let now = now_ms();
    let ago = |t: u64| crate::util::fmt_ago(t, now);

    let mut body = Vec::new();
    for n in v["notices"].as_array().into_iter().flatten() {
        let mut how = Vec::new();
        if let Some(e) = n["expires"].as_u64() {
            how.push(format!("for {}", crate::util::fmt_ago(now, e)));
        }
        if let Some(s) = n["shows"].as_u64() {
            how.push(format!("{s} prompts"));
        }
        if n["pid"].is_u64() {
            how.push("one shell".into());
        }
        if n["key"].is_string() && how.is_empty() {
            how.push("until it's over".into());
        }
        body.push(format!(
            "{} {} {}  {}",
            dim(&format!("{:>3}", n["id"].as_u64().unwrap_or(0))),
            dim(&format!("{:>4}", ago(n["created"].as_u64().unwrap_or(now)))),
            paint(n["text"].as_str().unwrap_or(""), by_level(&n["level"])),
            dim(&how.join(", ")),
        ));
    }
    if body.is_empty() {
        body.push(dim("none"));
    }
    for l in crate::boxes::draw("notices now", &body, Some("twig ack [ID] to dismiss")) {
        println!("{l}");
    }

    let past = v["past"].as_array().cloned().unwrap_or_default();
    if !past.is_empty() {
        let body: Vec<String> = past
            .iter()
            .rev()
            .take(20)
            .map(|n| {
                format!(
                    "{} {} {}  {}",
                    dim(&format!("{:>3}", n["id"].as_u64().unwrap_or(0))),
                    dim(&format!("{:>4}", ago(n["ended"].as_u64().unwrap_or(now)))),
                    paint(n["text"].as_str().unwrap_or(""), by_level(&n["level"])),
                    dim(n["how"].as_str().unwrap_or("")),
                )
            })
            .collect();
        println!();
        for l in crate::boxes::draw("earlier, newest first", &body, None) {
            println!("{l}");
        }
    }

    let muted: Vec<String> = v["muted"].as_array().into_iter().flatten().filter_map(|m| m.as_str()).map(String::from).collect();
    if !muted.is_empty() {
        println!();
        for l in crate::boxes::draw("ignored", &muted, Some("twig notices --unignore PATTERN")) {
            println!("{l}");
        }
    }
    0
}

/// `twig notices --ignore PATTERN|ID`, `twig notices --unignore PATTERN`.
/// An ID ignores that notice's text.
fn mute_cmd(args: &[String]) -> i32 {
    let undo = args[0] == "--unignore";
    let Some(arg) = args.get(1) else {
        eprintln!("usage: twig notices --ignore PATTERN|ID   (or --unignore PATTERN)");
        return 2;
    };
    // A notice's id: its text, as it is.
    let pattern = match arg.parse::<u64>() {
        Ok(id) if !undo => {
            let v = match client::ask(&Request::Notices) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("twig: {e}");
                    return 1;
                }
            };
            match v["notices"].as_array().into_iter().flatten().find(|n| n["id"].as_u64() == Some(id)) {
                Some(n) => n["text"].as_str().unwrap_or("").to_string(),
                None => {
                    eprintln!("twig: no notice {id} now (twig notices)");
                    return 1;
                }
            }
        }
        _ => arg.clone(),
    };
    match client::ask(&Request::Mute { pattern: pattern.clone(), undo }) {
        Ok(v) if undo => {
            if v["unmuted"].as_bool() == Some(true) {
                println!("no longer ignoring \"{pattern}\"");
            } else {
                println!("\"{pattern}\" wasn't ignored (twig notices lists those that are)");
            }
            0
        }
        Ok(v) => {
            let gone = v["gone"].as_u64().unwrap_or(0);
            println!("ignoring notices with \"{pattern}\" from now on ({gone} showing went); twig notices --unignore to undo");
            0
        }
        Err(e) => {
            eprintln!("twig: {e}");
            1
        }
    }
}

/// `twig ack [ID]`: dismiss one notice, or all that this shell shows.
pub fn ack_cmd(args: &[String]) -> i32 {
    let id = match args.first() {
        Some(a) => match a.parse() {
            Ok(i) => Some(i),
            Err(_) => {
                eprintln!("usage: twig ack [ID]   (ids are in `twig status`)");
                return 2;
            }
        },
        None => None,
    };
    let pid = if id.is_none() { Some(unsafe { libc::getppid() } as u32) } else { None };
    match client::ask(&Request::Ack { id, pid }) {
        Ok(v) => {
            let n = v["acked"].as_u64().unwrap_or(0);
            println!("dismissed {n} notice{}; twig notices shows them again", if n == 1 { "" } else { "s" });
            0
        }
        Err(e) => {
            eprintln!("twig: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(text: &str, key: Option<&str>) -> New {
        New { key: key.map(String::from), text: text.into(), level: Level::Info, ttl_ms: None, shows: None, pid: None }
    }

    #[test]
    fn shows_run_out_per_shell() {
        let mut n = Notices::default();
        n.put(New { shows: Some(2), ..new("hi", None) });
        let now = now_ms();
        assert!(n.top(1, now).is_some());
        n.shown(1, now);
        n.shown(1, now);
        assert!(n.top(1, now).is_none());
        assert!(n.top(2, now).is_some());
    }

    #[test]
    fn keyed_notices_replace_and_clear() {
        let mut n = Notices::default();
        assert!(n.put(new("disk 91%", Some("disk:/"))));
        assert!(!n.put(new("disk 91%", Some("disk:/"))));
        assert!(n.put(new("disk 95%", Some("disk:/"))));
        assert_eq!(n.list.len(), 1);
        assert!(n.clear("disk:/"));
        assert!(n.list.is_empty());
    }

    #[test]
    fn muted_notices_go_and_stay_away() {
        let mut n = Notices::default();
        n.put(new("backup to basalt failed", None));
        n.put(new("disk / 94% full", Some("disk:/")));
        assert_eq!(n.mute("basalt"), 1);
        assert_eq!(n.list.len(), 1);
        assert!(!n.put(new("backup to basalt failed again", None)));
        assert_eq!(n.list.len(), 1);
        // A keyed notice whose new text is muted goes.
        n.mute("disk / 9?% full");
        assert!(n.list.is_empty());
        assert!(n.unmute("basalt"));
        assert!(n.put(new("backup to basalt failed", None)));
        assert_eq!(n.past.iter().filter(|p| p.how == "ignored").count(), 2);
    }

    #[test]
    fn mute_patterns() {
        assert!(mute_matches("rtkit", "failed: rtkit-daemon"));
        assert!(mute_matches("RTKIT", "rtkit-daemon"));
        assert!(mute_matches("racc-*", "racc-attest"));
        assert!(!mute_matches("racc-*", "failed: racc-attest"));
        assert!(!mute_matches("basalt", "backup to matterhorn"));
    }

    #[test]
    fn gone_notices_are_kept_with_how() {
        let mut n = Notices::default();
        n.put(new("one", None));
        n.put(new("disk", Some("disk:/")));
        n.put(New { ttl_ms: Some(0), ..new("brief", None) });
        n.expire(now_ms() + 1);
        n.ack(Some(1), None);
        n.clear("disk:/");
        let how: Vec<(&str, &str)> = n.past.iter().map(|p| (p.text.as_str(), p.how)).collect();
        assert_eq!(how, vec![("brief", "expired"), ("one", "dismissed"), ("disk", "cleared")]);
        assert!(n.list.is_empty());
    }

    #[test]
    fn ack_holds_until_worse_or_cleared() {
        let mut n = Notices::default();
        n.put(new("disk 91%", Some("disk:/")));
        assert_eq!(n.ack(None, Some(7)), 1);
        assert!(!n.put(new("disk 91%", Some("disk:/"))));
        assert!(!n.put(new("disk 92%, 10G left", Some("disk:/"))));
        assert!(n.put(New { level: Level::Error, ..new("disk 99%", Some("disk:/")) }));
        n.ack(None, Some(7));
        n.clear("disk:/");
        assert!(n.put(new("disk 91%", Some("disk:/"))));
    }

    #[test]
    fn most_severe_first() {
        let mut n = Notices::default();
        n.put(new("a", None));
        n.put(New { level: Level::Error, ..new("b", None) });
        n.put(new("c", None));
        let (text, level, more) = n.top(1, now_ms()).unwrap();
        assert_eq!((text.as_str(), level, more), ("b", Level::Error, 2));
    }

    #[test]
    fn scoped_to_a_shell() {
        let mut n = Notices::default();
        n.put(New { pid: Some(5), ..new("job done", None) });
        assert!(n.top(5, now_ms()).is_some());
        assert!(n.top(6, now_ms()).is_none());
        n.orphan(5);
        assert!(n.top(6, now_ms()).is_some());
    }
}
