//! Notices: short messages on the prompt that go away by themselves, after
//! a time, after being shown a number of times, when what they are about is
//! over (a full disk with room again), or when dismissed with `twig ack`.

use crate::client;
use crate::proto::Request;
use crate::util::{fmt_ago, now_ms, parse_duration};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
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

    /// The condition behind a keyed notice is over.
    pub fn clear(&mut self, key: &str) -> bool {
        self.acked.remove(key);
        let before = self.list.len();
        self.list.retain(|n| n.key.as_deref() != Some(key));
        before != self.list.len()
    }

    pub fn expire(&mut self, now: u64) -> bool {
        let before = self.list.len();
        self.list.retain(|n| n.expires.is_none_or(|e| e > now));
        before != self.list.len()
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
        let mut gone = 0;
        let acked = &mut self.acked;
        self.list.retain(|n| {
            let hit = match id {
                Some(i) => n.id == i,
                None => pid.is_none_or(|p| Self::visible(n, p, now)),
            };
            if hit {
                if let Some(k) = &n.key {
                    acked.insert(k.clone(), n.level);
                }
                gone += 1;
            }
            !hit
        });
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

/// `twig notices`
pub fn list_cmd(_args: &[String]) -> i32 {
    let v = match client::ask(&Request::Notices) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("twig: {e}");
            return 1;
        }
    };
    let now = now_ms();
    let list = v["notices"].as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        println!("no notices");
    }
    for n in list {
        let mut how = Vec::new();
        if let Some(e) = n["expires"].as_u64() {
            how.push(format!("for {}", fmt_ago(now, e)));
        }
        if let Some(s) = n["shows"].as_u64() {
            how.push(format!("{s} prompts"));
        }
        if n["pid"].is_u64() {
            how.push(format!("shell {}", n["pid"]));
        }
        if n["key"].is_string() && n["expires"].is_null() && n["shows"].is_null() {
            how.push("until it clears".into());
        }
        println!(
            "{:>4}  {:<5}  {:<5}  {}  ({})",
            n["id"].as_u64().unwrap_or(0),
            n["level"].as_str().unwrap_or(""),
            fmt_ago(n["created"].as_u64().unwrap_or(now), now),
            n["text"].as_str().unwrap_or(""),
            how.join(", ")
        );
    }
    0
}

/// `twig ack [ID]`: dismiss one notice, or all that this shell shows.
pub fn ack_cmd(args: &[String]) -> i32 {
    let id = match args.first() {
        Some(a) => match a.parse() {
            Ok(i) => Some(i),
            Err(_) => {
                eprintln!("usage: twig ack [ID]   (ids are in `twig notices`)");
                return 2;
            }
        },
        None => None,
    };
    let pid = if id.is_none() { Some(unsafe { libc::getppid() } as u32) } else { None };
    match client::ask(&Request::Ack { id, pid }) {
        Ok(v) => {
            let n = v["acked"].as_u64().unwrap_or(0);
            println!("dismissed {n} notice{}", if n == 1 { "" } else { "s" });
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
