//! Small helpers shared by the client and the daemon.

use std::ffi::CString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// "850ms", "3s", "1m03s", "2h05m", "3d04h".
pub fn fmt_duration(ms: u64) -> String {
    let s = ms / 1000;
    match s {
        _ if ms < 1000 => format!("{ms}ms"),
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m{:02}s", s / 60, s % 60),
        3600..=86399 => format!("{}h{:02}m", s / 3600, s % 3600 / 60),
        _ => format!("{}d{:02}h", s / 86400, s % 86400 / 3600),
    }
}

/// "how long ago", for listings: "now", "40s", "5m", "3h", "2d".
pub fn fmt_ago(then_ms: u64, now_ms: u64) -> String {
    let s = now_ms.saturating_sub(then_ms) / 1000;
    match s {
        0..=4 => "now".into(),
        5..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=86399 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86400),
    }
}

/// "50ms", "30s", "5m", "1h", "1d", or a bare number of seconds. "0" and
/// "off" mean never, which is returned as None.
pub fn parse_duration(text: &str) -> Result<Option<Duration>, String> {
    let t = text.trim();
    if t == "0" || t == "off" || t.is_empty() {
        return Ok(None);
    }
    let split = t.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let n: f64 = num.parse().map_err(|_| format!("\"{text}\" is not a duration (try 30s, 5m, 1h)"))?;
    let secs = match unit.trim() {
        "ms" => n / 1000.0,
        "" | "s" => n,
        "m" => n * 60.0,
        "h" => n * 3600.0,
        "d" => n * 86400.0,
        _ => return Err(format!("\"{text}\" is not a duration (try 30s, 5m, 1h)")),
    };
    Ok(Some(Duration::from_secs_f64(secs)))
}

/// strftime in the local time zone.
pub fn strftime(format: &str, epoch_secs: i64) -> String {
    let Ok(fmt) = CString::new(format) else { return String::new() };
    unsafe {
        let t: libc::time_t = epoch_secs as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        let mut buf = [0u8; 128];
        let n = libc::strftime(buf.as_mut_ptr().cast(), buf.len(), fmt.as_ptr(), &tm);
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

/// The number of terminal columns a string takes, skipping escape sequences.
pub fn visible_width(s: &str) -> usize {
    let mut width = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            } else {
                chars.next();
            }
            continue;
        }
        width += char_width(c);
    }
    width
}

pub fn char_width(c: char) -> usize {
    let u = c as u32;
    if u < 0x20 || (0x7f..0xa0).contains(&u) || (0x300..0x370).contains(&u) || u == 0x200d || (0xfe00..0xfe10).contains(&u) {
        0
    } else if (0x1100..0x1160).contains(&u)
        || (0x2e80..0xa4d0).contains(&u)
        || (0xac00..0xd7a4).contains(&u)
        || (0xf900..0xfb00).contains(&u)
        || (0xff00..0xff61).contains(&u)
        || (0xffe0..0xffe7).contains(&u)
        || (0x1f300..0x1f650).contains(&u)
        || (0x1f900..0x1fa00).contains(&u)
        || (0x20000..0x3fffe).contains(&u)
    {
        2
    } else {
        1
    }
}

/// Text from outside (a notice, a command line) made safe to print: no
/// escape sequences, no newlines.
pub fn clean(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string()
}

/// Shell-style glob with `*` and `?`.
pub fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            pi += 1;
            mark = ti;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// Write a file so that a reader never sees half of it.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

pub fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

pub struct Ran {
    pub status: Option<i32>,
    pub stdout: String,
    pub timed_out: bool,
}

/// Run a command and give up on it after `limit`, killing its whole process
/// group so that nothing it started is left behind.
pub fn run_with_limit(cmd: &mut Command, limit: Duration) -> io::Result<Ran> {
    use std::os::unix::process::CommandExt;
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        });
    }
    let mut child: Child = cmd.spawn()?;
    let mut out = child.stdout.take().expect("piped");
    let reader = thread::spawn(move || {
        let mut s = Vec::new();
        let _ = out.read_to_end(&mut s);
        s
    });
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st.code();
        }
        if start.elapsed() > limit {
            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
            let _ = child.wait();
            timed_out = true;
            break None;
        }
        thread::sleep(Duration::from_millis(if start.elapsed() < Duration::from_millis(50) { 1 } else { 10 }));
    };
    let stdout = if timed_out { Vec::new() } else { reader.join().unwrap_or_default() };
    Ok(Ran { status, stdout: String::from_utf8_lossy(&stdout).into_owned(), timed_out })
}

/// Start a program that outlives us: its own session, no terminal, output to
/// a file (or nowhere).
pub fn spawn_detached(cmd: &mut Command, log: Option<&Path>) -> io::Result<u32> {
    use std::os::unix::process::CommandExt;
    let out = match log {
        Some(p) => {
            if let Some(d) = p.parent() {
                fs::create_dir_all(d)?;
            }
            Stdio::from(fs::OpenOptions::new().create(true).append(true).open(p)?)
        }
        None => Stdio::null(),
    };
    let err = match log {
        Some(p) => Stdio::from(fs::OpenOptions::new().append(true).open(p)?),
        None => Stdio::null(),
    };
    cmd.stdin(Stdio::null()).stdout(out).stderr(err);
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    Ok(child.id())
}

/// The window size of a terminal, given its path.
pub fn tty_columns(tty: &str) -> Option<u16> {
    let f = fs::OpenOptions::new().read(true).custom_flags_noctty().open(tty).ok()?;
    use std::os::unix::io::AsRawFd;
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::ioctl(f.as_raw_fd(), libc::TIOCGWINSZ, &mut ws) };
    (r == 0 && ws.ws_col > 0).then_some(ws.ws_col)
}

pub trait NoCtty {
    fn custom_flags_noctty(&mut self) -> &mut Self;
}

impl NoCtty for fs::OpenOptions {
    fn custom_flags_noctty(&mut self) -> &mut Self {
        use std::os::unix::fs::OpenOptionsExt;
        self.custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
    }
}

/// Write to a terminal without ever blocking on it (a terminal stopped with
/// Ctrl-S would otherwise hang the daemon).
pub fn write_tty(tty: &str, bytes: &[u8]) -> io::Result<()> {
    let mut f = fs::OpenOptions::new().write(true).custom_flags_noctty().open(tty)?;
    f.write_all(bytes)
}

/// The fields of /proc/PID/stat that say whether a shell is in front:
/// (process group, foreground process group of its terminal).
pub fn proc_groups(pid: u32) -> Option<(i32, i32)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    // state ppid pgrp session tty_nr tpgid
    Some((f.get(2)?.parse().ok()?, f.get(5)?.parse().ok()?))
}

pub fn alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// Replace a leading home directory with ~.
pub fn tilde(path: &str, home: &str) -> String {
    if !home.is_empty() && home != "/" {
        if path == home {
            return "~".into();
        }
        if let Some(rest) = path.strip_prefix(home) {
            if rest.starts_with('/') {
                return format!("~{rest}");
            }
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(fmt_duration(850), "850ms");
        assert_eq!(fmt_duration(3_400), "3s");
        assert_eq!(fmt_duration(63_000), "1m03s");
        assert_eq!(fmt_duration(7_500_000), "2h05m");
        assert_eq!(parse_duration("50ms").unwrap(), Some(Duration::from_millis(50)));
        assert_eq!(parse_duration("5m").unwrap(), Some(Duration::from_secs(300)));
        assert_eq!(parse_duration("90").unwrap(), Some(Duration::from_secs(90)));
        assert_eq!(parse_duration("off").unwrap(), None);
        assert!(parse_duration("5 parsecs").is_err());
    }

    #[test]
    fn globs() {
        assert!(glob("woodwind*", "woodwind.lan"));
        assert!(glob("woodwind*", "woodwind"));
        assert!(!glob("woodwind*", "nomad"));
        assert!(glob("*.corp.google.com", "a.mtv.corp.google.com"));
        assert!(glob("h?st", "host"));
    }

    #[test]
    fn widths() {
        assert_eq!(visible_width("\x1b[1;32mabc\x1b[0m"), 3);
        assert_eq!(visible_width("✔ ok"), 4);
        assert_eq!(visible_width("日本"), 4);
    }

    #[test]
    fn tildes() {
        assert_eq!(tilde("/home/j/x", "/home/j"), "~/x");
        assert_eq!(tilde("/home/j", "/home/j"), "~");
        assert_eq!(tilde("/home/jx", "/home/j"), "/home/jx");
    }
}
