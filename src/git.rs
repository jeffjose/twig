//! Git repositories: finding one, reading its branch without running git,
//! and its status, which the daemon computes away from the prompt.

use crate::template::Style;
use crate::util::{read_trimmed, run_with_limit};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Repo {
    /// The top of the working tree.
    pub root: PathBuf,
    /// This working tree's git directory (.git, or one under
    /// .git/worktrees for a linked worktree).
    pub gitdir: PathBuf,
    /// The directory with refs and logs, shared by all worktrees.
    pub common: PathBuf,
}

/// The repository a directory is in, by looking for .git upward.
pub fn find(cwd: &Path) -> Option<Repo> {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        let dotgit = d.join(".git");
        if let Ok(meta) = fs::metadata(&dotgit) {
            let gitdir = if meta.is_dir() {
                dotgit
            } else {
                let text = fs::read_to_string(&dotgit).ok()?;
                let p = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
                if p.is_absolute() { p } else { d.join(p) }
            };
            let common = match read_trimmed(gitdir.join("commondir")) {
                Some(c) if Path::new(&c).is_absolute() => PathBuf::from(c),
                Some(c) => gitdir.join(c),
                None => gitdir.clone(),
            };
            return Some(Repo { root: d.to_path_buf(), gitdir, common });
        }
        dir = d.parent();
    }
    None
}

/// The branch from HEAD, or the start of the commit when detached.
pub fn head(repo: &Repo) -> Option<String> {
    let text = read_trimmed(repo.gitdir.join("HEAD"))?;
    match text.strip_prefix("ref: ") {
        Some(r) => Some(r.strip_prefix("refs/heads/").unwrap_or(r).to_string()),
        None => Some(format!(":{}", text.get(..7)?)),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GitStatus {
    pub branch: String,
    pub upstream: bool,
    pub ahead: u32,
    pub behind: u32,
    pub staged: u32,
    pub changed: u32,
    pub untracked: u32,
    pub conflicts: u32,
    pub stash: u32,
    pub state: Option<&'static str>,
}

impl GitStatus {
    pub fn dirty(&self) -> bool {
        self.staged + self.changed + self.untracked + self.conflicts > 0
    }

    /// The variables {git_*}; {git} is all of the status parts together.
    pub fn var(&self, name: &str) -> Option<(String, Style)> {
        let n = |v: u32, mark: &str, style: &str| (v > 0).then(|| (format!("{mark}{v}"), Style::of(style)));
        match name {
            "git_branch" => Some((self.branch.clone(), Style::of("magenta"))),
            "git_ahead" => n(self.ahead, "↑", "magenta"),
            "git_behind" => n(self.behind, "↓", "magenta"),
            "git_staged" => n(self.staged, "+", "blue"),
            "git_changed" => n(self.changed, "~", "yellow"),
            "git_untracked" => n(self.untracked, "?", "red"),
            "git_conflicts" => n(self.conflicts, "=", "red,bold"),
            "git_stash" => n(self.stash, "≡", "cyan"),
            "git_clean" => (!self.dirty()).then(|| ("✔".into(), Style::of("green"))),
            "git_dirty" => self.dirty().then(|| ("*".into(), Style::of("yellow"))),
            "git_state" => self.state.map(|s| (s.to_string(), Style::of("magenta,bold"))),
            "git" => {
                let parts: String = [
                    "git_state", "git_ahead", "git_behind", "git_conflicts", "git_staged", "git_changed",
                    "git_untracked", "git_stash", "git_clean",
                ]
                .iter()
                .filter_map(|v| self.var(v))
                .map(|(t, s)| s.paint(&t))
                .collect();
                Some((parts, Style::default()))
            }
            _ => None,
        }
    }
}

/// Read porcelain v2 output into a status.
pub fn parse_porcelain(out: &str, st: &mut GitStatus) {
    let mut oid = "";
    for line in out.lines() {
        if let Some(h) = line.strip_prefix("# ") {
            if let Some(b) = h.strip_prefix("branch.head ") {
                st.branch = b.to_string();
            } else if let Some(o) = h.strip_prefix("branch.oid ") {
                oid = o;
            } else if h.starts_with("branch.upstream ") {
                st.upstream = true;
            } else if let Some(ab) = h.strip_prefix("branch.ab ") {
                for part in ab.split_whitespace() {
                    if let Some(a) = part.strip_prefix('+') {
                        st.ahead = a.parse().unwrap_or(0);
                    } else if let Some(b) = part.strip_prefix('-') {
                        st.behind = b.parse().unwrap_or(0);
                    }
                }
            }
            continue;
        }
        let mut f = line.splitn(3, ' ');
        match (f.next(), f.next()) {
            (Some("1" | "2"), Some(xy)) => {
                let b = xy.as_bytes();
                if b.first().is_some_and(|&c| c != b'.') {
                    st.staged += 1;
                }
                if b.get(1).is_some_and(|&c| c != b'.') {
                    st.changed += 1;
                }
            }
            (Some("u"), _) => st.conflicts += 1,
            (Some("?"), _) => st.untracked += 1,
            _ => {}
        }
    }
    if st.branch == "(detached)" {
        st.branch = format!(":{}", oid.get(..7).unwrap_or(oid));
    }
}

/// Run git status. --no-optional-locks keeps it from writing the index,
/// which would wake the daemon's watch on it and start the next status.
pub fn status(repo: &Repo, untracked: bool, limit: Duration) -> Result<GitStatus, String> {
    let mut cmd = Command::new("git");
    cmd.arg("--no-optional-locks").arg("-C").arg(&repo.root);
    cmd.args(["status", "--porcelain=v2", "--branch", "--ignore-submodules=dirty"]);
    cmd.arg(if untracked { "--untracked-files=normal" } else { "--untracked-files=no" });
    cmd.env("GIT_OPTIONAL_LOCKS", "0");
    let ran = run_with_limit(&mut cmd, limit).map_err(|e| format!("git: {e}"))?;
    if ran.timed_out {
        return Err(format!("git status took longer than {}s", limit.as_secs()));
    }
    if ran.status != Some(0) {
        return Err("git status failed".into());
    }
    let mut st = GitStatus::default();
    parse_porcelain(&ran.stdout, &mut st);
    st.stash = fs::read_to_string(repo.common.join("logs/refs/stash")).map(|s| s.lines().count() as u32).unwrap_or(0);
    st.state = state(&repo.gitdir);
    Ok(st)
}

fn state(gitdir: &Path) -> Option<&'static str> {
    let has = |f: &str| gitdir.join(f).exists();
    if has("rebase-merge") || has("rebase-apply") {
        Some("REBASE")
    } else if has("MERGE_HEAD") {
        Some("MERGE")
    } else if has("CHERRY_PICK_HEAD") {
        Some("PICK")
    } else if has("REVERT_HEAD") {
        Some("REVERT")
    } else if has("BISECT_LOG") {
        Some("BISECT")
    } else {
        None
    }
}

/// Fetch from upstream without ever asking for a password.
pub fn fetch(repo: &Repo, limit: Duration) -> bool {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(&repo.root).args(["fetch", "--quiet", "--no-tags", "--no-recurse-submodules"]);
    cmd.env("GIT_TERMINAL_PROMPT", "0").env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes -o ConnectTimeout=10");
    matches!(run_with_limit(&mut cmd, limit), Ok(r) if r.status == Some(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain() {
        let out = "# branch.oid 1234567890abcdef\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\n\
                   1 M. N... 100644 100644 100644 a b file1\n1 .M N... 100644 100644 100644 a b file2\n\
                   1 MM N... 100644 100644 100644 a b file3\n2 R. N... 100644 100644 100644 a b R100 new\told\n\
                   u UU N... 1 2 3 4 a b c conflict\n? untracked1\n? untracked2\n! ignored\n";
        let mut st = GitStatus::default();
        parse_porcelain(out, &mut st);
        assert_eq!(st.branch, "main");
        assert!(st.upstream);
        assert_eq!((st.ahead, st.behind), (2, 1));
        assert_eq!((st.staged, st.changed, st.untracked, st.conflicts), (3, 2, 2, 1));
        assert!(st.dirty());
    }

    #[test]
    fn detached() {
        let mut st = GitStatus::default();
        parse_porcelain("# branch.oid abcdef1234\n# branch.head (detached)\n", &mut st);
        assert_eq!(st.branch, ":abcdef1");
        assert_eq!(st.var("git_clean").unwrap().0, "✔");
    }

    #[test]
    fn finds_repo_and_head() {
        let dir = std::env::temp_dir().join(format!("twig-git-test-{}", std::process::id()));
        let sub = dir.join("a/b");
        fs::create_dir_all(&sub).unwrap();
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join(".git/HEAD"), "ref: refs/heads/feature/x\n").unwrap();
        let repo = find(&sub).unwrap();
        assert_eq!(repo.root, dir);
        assert_eq!(head(&repo).unwrap(), "feature/x");
        fs::write(dir.join(".git/HEAD"), "0123456789abcdef\n").unwrap();
        assert_eq!(head(&repo).unwrap(), ":0123456");
        fs::remove_dir_all(&dir).unwrap();
    }
}
