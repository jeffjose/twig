//! Command history: every command from every shell, with where it ran, how
//! it ended and how long it took. One file per machine, one JSON line per
//! command, only ever appended to.

use crate::paths;
use crate::template::Style;
use crate::util::{fmt_ago, fmt_duration, now_ms, tilde};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    /// Start, ms since the epoch.
    pub t: u64,
    /// How long it took, ms.
    #[serde(default)]
    pub d: u64,
    /// Exit status; None when not known (imported).
    #[serde(default)]
    pub s: Option<i32>,
    /// The command line.
    pub c: String,
    /// Where it ran.
    #[serde(default)]
    pub w: String,
    /// Which machine.
    #[serde(default)]
    pub h: String,
    /// Which shell.
    #[serde(default)]
    pub p: u32,
}

pub fn append(host: &str, e: &Entry) -> io::Result<()> {
    let path = paths::history_file(host);
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    let mut line = serde_json::to_vec(e)?;
    line.push(b'\n');
    // One write with O_APPEND, so lines from two writers never interleave.
    OpenOptions::new().create(true).append(true).open(path)?.write_all(&line)
}

/// Every machine's history, oldest first.
pub fn load_all() -> Vec<Entry> {
    let mut all = Vec::new();
    if let Ok(dir) = fs::read_dir(paths::history_dir()) {
        for f in dir.flatten() {
            if f.path().extension().is_some_and(|x| x == "jsonl") {
                load_file(&f.path(), &mut all);
            }
        }
    }
    all.sort_by_key(|e| e.t);
    all
}

fn load_file(path: &Path, into: &mut Vec<Entry>) {
    let Ok(f) = fs::File::open(path) else { return };
    for line in BufReader::new(f).lines().map_while(Result::ok) {
        if let Ok(e) = serde_json::from_str::<Entry>(&line) {
            into.push(e);
        }
    }
}

/// Newest first, each command once.
pub fn unique_newest<'a>(entries: impl DoubleEndedIterator<Item = &'a Entry>) -> Vec<&'a Entry> {
    let mut seen = HashSet::new();
    entries.rev().filter(|e| seen.insert(e.c.as_str())).collect()
}

/// Write the last `size` distinct commands in tcsh's history format, for
/// `history -L` in a new shell.
pub fn export_tcsh(entries: &[Entry], size: usize, path: &Path) -> io::Result<()> {
    let mut list = unique_newest(entries.iter());
    list.truncate(size);
    let mut out = String::new();
    for e in list.iter().rev() {
        out.push_str(&format!("#+{}\n{}\n", e.t / 1000, e.c));
    }
    crate::util::atomic_write(path, out.as_bytes())
}

/// Read tcsh's ~/.history: "#+SECONDS" lines, each followed by a command.
pub fn import_tcsh(path: &Path, host: &str) -> io::Result<Vec<Entry>> {
    let text = fs::read(path)?;
    let text = String::from_utf8_lossy(&text);
    let mut out = Vec::new();
    let mut t = 0u64;
    for line in text.lines() {
        if let Some(secs) = line.strip_prefix("#+") {
            t = secs.trim().parse::<u64>().unwrap_or(0) * 1000;
            continue;
        }
        let c = line.trim_end();
        if c.is_empty() {
            continue;
        }
        out.push(Entry { t, d: 0, s: None, c: c.to_string(), w: String::new(), h: host.to_string(), p: 0 });
    }
    Ok(out)
}

/// Whether a command is kept: not twig's own aliases, not an ignore pattern.
pub fn keep(cmd: &str, ignore: &[String]) -> bool {
    let c = cmd.trim();
    !c.is_empty() && !c.starts_with("_twig") && !ignore.iter().any(|p| crate::util::glob(p, c))
}

pub struct Filter {
    pub terms: Vec<String>,
    pub here: Option<String>,
    pub failed: bool,
    pub ok: bool,
    pub host: Option<String>,
    pub shell: Option<u32>,
}

impl Filter {
    pub fn matches(&self, e: &Entry) -> bool {
        let c = e.c.to_lowercase();
        self.terms.iter().all(|t| c.contains(t.as_str()))
            && self.here.as_ref().is_none_or(|h| &e.w == h)
            && (!self.failed || e.s.is_some_and(|s| s != 0))
            && (!self.ok || e.s.is_none_or(|s| s == 0))
            && self.host.as_ref().is_none_or(|h| &e.h == h)
            && self.shell.is_none_or(|p| e.p == p)
    }
}

const HELP: &str = "\
twig history [OPTIONS] [WORDS...]   (or twig h)

  Commands containing all of WORDS, newest last, each once.

  -d, --here        only ones run in this directory
  -x, --failed      only ones that failed
      --ok          leave out ones that failed
  -m, --machine H   only from machine H (when histories are shared)
  -a, --all         every run, not each command once, with time and place
  -n N              how many (default 40)
      --import FILE add a tcsh history file (as ~/.history) to this machine's
";

/// `twig history`
pub fn cmd(args: &[String]) -> i32 {
    let mut f = Filter { terms: Vec::new(), here: None, failed: false, ok: false, host: None, shell: None };
    let (mut all, mut n) = (false, 40usize);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-d" | "--here" => f.here = std::env::current_dir().ok().and_then(|p| p.to_str().map(String::from)),
            "-x" | "--failed" => f.failed = true,
            "--ok" => f.ok = true,
            "-a" | "--all" => all = true,
            "-m" | "--machine" => f.host = it.next().cloned(),
            "-n" => n = it.next().and_then(|v| v.parse().ok()).unwrap_or(n),
            "--import" => {
                let Some(file) = it.next() else {
                    eprintln!("twig history --import FILE");
                    return 2;
                };
                return import(Path::new(file));
            }
            "-h" | "--help" => {
                print!("{HELP}");
                return 0;
            }
            s if s.starts_with('-') && s.len() > 1 => {
                eprintln!("twig history: unknown option {s}\n\n{HELP}");
                return 2;
            }
            _ => f.terms.push(a.to_lowercase()),
        }
    }
    let entries = load_all();
    let now = now_ms();
    let home = paths::home().to_string_lossy().into_owned();
    let color = unsafe { libc::isatty(1) } == 1;
    let dim = |s: &str| if color { Style::of("dim").paint(s) } else { s.to_string() };
    let red = |s: &str| if color { Style::of("red").paint(s) } else { s.to_string() };
    let mut out = io::stdout().lock();
    let list: Vec<&Entry> = if all {
        let mut v: Vec<&Entry> = entries.iter().rev().filter(|e| f.matches(e)).take(n).collect();
        v.reverse();
        v
    } else {
        let mut v: Vec<&Entry> = unique_newest(entries.iter().filter(|e| f.matches(e))).into_iter().take(n).collect();
        v.reverse();
        v
    };
    for e in list {
        let mark = match e.s {
            Some(0) | None => " ".to_string(),
            Some(_) => red("✘"),
        };
        let r = if all {
            let took = if e.d >= 1000 { format!(" {}", fmt_duration(e.d)) } else { String::new() };
            writeln!(out, "{} {mark} {}  {}", dim(&format!("{:>4}", fmt_ago(e.t, now))), e.c, dim(&format!("{}{took}", tilde(&e.w, &home))))
        } else {
            writeln!(out, "{} {mark} {}", dim(&format!("{:>4}", fmt_ago(e.t, now))), e.c)
        };
        if r.is_err() {
            break;
        }
    }
    0
}

/// Add an old tcsh history file to this machine's history, skipping
/// commands already there at the same second.
pub fn import(path: &Path) -> i32 {
    let host = paths::hostname();
    let new = match import_tcsh(path, &host) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("twig: {}: {e}", path.display());
            return 1;
        }
    };
    let have: HashSet<(u64, String)> = load_all().into_iter().map(|e| (e.t / 1000, e.c)).collect();
    let mut added = 0;
    for e in new.iter().filter(|e| !have.contains(&(e.t / 1000, e.c.clone()))) {
        if append(&host, e).is_err() {
            break;
        }
        added += 1;
    }
    println!("added {added} of {} commands from {}", new.len(), path.display());
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(t: u64, c: &str) -> Entry {
        Entry { t, d: 0, s: Some(0), c: c.into(), w: "/x".into(), h: "h".into(), p: 1 }
    }

    #[test]
    fn unique_keeps_newest() {
        let v = vec![e(1, "ls"), e(2, "make"), e(3, "ls")];
        let u = unique_newest(v.iter());
        assert_eq!(u.iter().map(|e| (e.t, e.c.as_str())).collect::<Vec<_>>(), vec![(3, "ls"), (2, "make")]);
    }

    #[test]
    fn reads_tcsh_history() {
        let p = std::env::temp_dir().join(format!("twig-hist-{}", std::process::id()));
        fs::write(&p, "#+1700000000\nls -la\n#+1700000005\ngit status\n").unwrap();
        let v = import_tcsh(&p, "h").unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!((v[1].t, v[1].c.as_str()), (1_700_000_005_000, "git status"));
        fs::remove_file(p).unwrap();
    }

    #[test]
    fn keeps_and_filters() {
        assert!(!keep("_twig_pick", &[]));
        assert!(!keep("ls", &["ls".into()]));
        assert!(!keep("cd /tmp", &["cd *".into()]));
        assert!(keep("make", &["ls".into()]));
        let f = Filter { terms: vec!["mak".into()], here: Some("/x".into()), failed: false, ok: true, host: None, shell: None };
        assert!(f.matches(&e(1, "Make all")));
        assert!(!f.matches(&Entry { s: Some(2), ..e(1, "make") }));
    }
}
