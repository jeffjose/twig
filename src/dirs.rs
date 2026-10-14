//! Directories you use, by how often and how recently, for `z`.

use crate::paths;
use crate::util::{atomic_write, fmt_ago, now_ms, tilde};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Visits {
    pub rank: f64,
    pub last: u64,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Dirs {
    pub dirs: HashMap<String, Visits>,
    #[serde(skip)]
    pub dirty: bool,
}

const MAX_TOTAL: f64 = 10_000.0;

impl Dirs {
    pub fn load(host: &str) -> Dirs {
        fs::read(paths::dirs_file(host)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&mut self, host: &str) {
        if self.dirty {
            if let Ok(b) = serde_json::to_vec(self) {
                let _ = atomic_write(&paths::dirs_file(host), &b);
            }
            self.dirty = false;
        }
    }

    pub fn visit(&mut self, path: &str) {
        if path == "/" || path.is_empty() {
            return;
        }
        let v = self.dirs.entry(path.to_string()).or_default();
        v.rank += 1.0;
        v.last = now_ms();
        self.dirty = true;
        // Age everything once the total grows, and forget the rarely used.
        let total: f64 = self.dirs.values().map(|v| v.rank).sum();
        if total > MAX_TOTAL {
            for v in self.dirs.values_mut() {
                v.rank *= 0.9;
            }
            self.dirs.retain(|_, v| v.rank >= 1.0);
        }
    }
}

/// How much a directory counts now: often used counts, recently used
/// counts more.
pub fn score(v: &Visits, now: u64) -> f64 {
    let age = now.saturating_sub(v.last) / 1000;
    let w = match age {
        0..=3599 => 4.0,
        3600..=86_399 => 2.0,
        86_400..=604_799 => 0.5,
        _ => 0.25,
    };
    v.rank * w
}

/// The best directory whose path has every term, in order, and whose last
/// part has the last term.
pub fn best<'a>(dirs: &'a HashMap<String, Visits>, terms: &[String], cwd: &str, now: u64) -> Option<&'a str> {
    let terms: Vec<String> = terms.iter().map(|t| t.to_lowercase()).collect();
    let fits = |path: &str| {
        let p = path.to_lowercase();
        let mut from = 0;
        for t in &terms {
            match p[from..].find(t.as_str()) {
                Some(i) => from += i + t.len(),
                None => return false,
            }
        }
        let last = p.rsplit('/').next().unwrap_or("");
        terms.last().is_none_or(|t| last.contains(t.as_str()))
    };
    dirs.iter()
        .filter(|(p, _)| p.as_str() != cwd && fits(p) && Path::new(p).is_dir())
        .max_by(|a, b| score(a.1, now).total_cmp(&score(b.1, now)))
        .map(|(p, _)| p.as_str())
}

/// `twig z WORDS...`: print the directory to go to.
pub fn z_cmd(args: &[String]) -> i32 {
    if args.is_empty() {
        println!("{}", paths::home().display());
        return 0;
    }
    if args.len() == 1 && (args[0] == "-" || Path::new(&args[0]).is_dir()) {
        println!("{}", args[0]);
        return 0;
    }
    let d = Dirs::load(&paths::hostname());
    let cwd = std::env::current_dir().ok().and_then(|p| p.to_str().map(String::from)).unwrap_or_default();
    match best(&d.dirs, args, &cwd, now_ms()) {
        Some(p) => {
            println!("{p}");
            0
        }
        None => {
            eprintln!("z: no directory matches {}", args.join(" "));
            1
        }
    }
}

/// `twig dirs`: the directories z knows, best last.
pub fn list_cmd(args: &[String]) -> i32 {
    let n: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(30);
    let d = Dirs::load(&paths::hostname());
    let now = now_ms();
    let home = paths::home().to_string_lossy().into_owned();
    let mut v: Vec<(&String, &Visits)> = d.dirs.iter().collect();
    v.sort_by(|a, b| score(b.1, now).total_cmp(&score(a.1, now)));
    v.truncate(n);
    for (p, vis) in v.iter().rev() {
        println!("{:>7.1}  {:>4}  {}", score(vis, now), fmt_ago(vis.last, now), tilde(p, &home));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_by_terms_and_score() {
        let tmp = std::env::temp_dir().join(format!("twig-z-{}", std::process::id()));
        let a = tmp.join("src/twig");
        let b = tmp.join("old/twig-legacy");
        let c = tmp.join("twig/docs");
        for d in [&a, &b, &c] {
            fs::create_dir_all(d).unwrap();
        }
        let now = now_ms();
        let mut m = HashMap::new();
        m.insert(a.to_str().unwrap().to_string(), Visits { rank: 5.0, last: now });
        m.insert(b.to_str().unwrap().to_string(), Visits { rank: 50.0, last: now - 30 * 86_400_000 });
        m.insert(c.to_str().unwrap().to_string(), Visits { rank: 9.0, last: now });
        assert_eq!(best(&m, &["twig".into()], "", now), Some(a.to_str().unwrap()));
        assert_eq!(best(&m, &["twig".into(), "doc".into()], "", now), Some(c.to_str().unwrap()));
        assert_eq!(best(&m, &["legacy".into()], "", now), Some(b.to_str().unwrap()));
        assert_eq!(best(&m, &["twig".into()], a.to_str().unwrap(), now), Some(b.to_str().unwrap()));
        assert_eq!(best(&m, &["nothing".into()], "", now), None);
        fs::remove_dir_all(tmp).unwrap();
    }
}
