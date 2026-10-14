//! The side of twig that runs in the shell: it asks the daemon, and when the
//! daemon can't answer it starts one and gets by on its own.

use crate::config;
use crate::git;
use crate::paths;
use crate::proto::{self, Request};
use crate::render::{self, Ctx};
use crate::tcsh;
use crate::util::{now_ms, spawn_detached};
use std::collections::HashMap;
use std::env;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use crate::template::Style;

/// Longer than the daemon ever takes to answer a prompt (it answers within
/// git.wait), short enough that a stuck daemon costs little.
const PROMPT_LIMIT: Duration = Duration::from_millis(400);

fn cwd() -> String {
    env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .or_else(|| env::var("PWD").ok())
        .unwrap_or_else(|| "?".into())
}

/// Start the daemon in the background, unless one is starting already.
pub fn start_daemon() {
    let Ok(exe) = env::current_exe() else { return };
    let log = paths::state_dir().join("daemon.log");
    let _ = spawn_detached(Command::new(exe).arg("daemon"), Some(&log));
}

/// Ask the daemon, starting it if it isn't running.
pub fn ask(req: &Request) -> Result<serde_json::Value, String> {
    match proto::send(req, Duration::from_secs(5)) {
        Ok(v) => return reply(v),
        Err(e) if !matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused) => {
            return Err(format!("the daemon didn't answer: {e}"));
        }
        Err(_) => {}
    }
    start_daemon();
    let start = Instant::now();
    loop {
        thread::sleep(Duration::from_millis(20));
        match proto::send(req, Duration::from_secs(5)) {
            Ok(v) => return reply(v),
            Err(e) if start.elapsed() > Duration::from_secs(3) => {
                return Err(format!("couldn't start the daemon ({e}); see {}", paths::state_dir().join("daemon.log").display()));
            }
            Err(_) => {}
        }
    }
}

fn reply(v: serde_json::Value) -> Result<serde_json::Value, String> {
    match v["error"].as_str() {
        Some(e) => Err(e.to_string()),
        None => Ok(v),
    }
}

/// `twig prompt PID STATUS`, from precmd. Prints the prompt and the right
/// prompt, one per line, written for tcsh.
pub fn prompt(args: &[String]) -> i32 {
    let start = Instant::now();
    let pid: u32 = args.first().and_then(|a| a.parse().ok()).unwrap_or_else(|| unsafe { libc::getppid() } as u32);
    let status: i32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(0);
    let cwd = cwd();
    let env: Vec<(String, String)> = env::vars().collect();
    let req = Request::Prompt { pid, status, cwd: cwd.clone(), env };
    let prep = start.elapsed();
    let (p, r) = match proto::send_timed(&req, PROMPT_LIMIT) {
        Ok((v, connect)) if v["prompt"].is_string() => {
            let mut p = v["prompt"].as_str().unwrap_or("").to_string();
            if v["timings"].is_object() {
                let total = start.elapsed();
                let lines = timings_box(&v["timings"], prep, connect, total);
                // Escaped as one, so each line's closing escape before the
                // newline gets the space tcsh needs.
                p = format!("{}{p}", tcsh::escape(&(lines.join("\n") + "\n")));
            }
            (p, v["right"].as_str().unwrap_or("").to_string())
        }
        other => {
            if let Err(e) = &other {
                if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused) {
                    start_daemon();
                }
            }
            local_prompt(&cwd, status)
        }
    };
    let mut out = io::stdout().lock();
    // An empty line would be dropped by tcsh's word splitting, and
    // $_tw[2] would then be out of range.
    let r = if r.is_empty() { "%{%}".to_string() } else { r };
    let _ = writeln!(out, "{p}\n{r}");
    0
}

/// The prompt drawn without the daemon: everything that is quick to find
/// out here, and nothing that isn't.
fn local_prompt(cwd: &str, status: i32) -> (String, String) {
    let cfg = config::read(&paths::config_file()).or_else(|_| config::load(config::Config::default())).expect("defaults load");
    let env: HashMap<String, String> = env::vars().collect();
    let host = paths::hostname();
    let facts = HashMap::new();
    let branch = git::find(Path::new(cwd)).and_then(|r| git::head(&r));
    let ctx = Ctx {
        cfg: &cfg,
        host: &host,
        fqdn: &host,
        cwd,
        env: &env,
        status,
        duration_ms: None,
        time_secs: (now_ms() / 1000) as i64,
        facts: &facts,
        branch,
        git: None,
        notice: None,
        jobs: String::new(),
    };
    let (p, r) = render::render(&ctx, None);
    (tcsh::escape(&p), tcsh::escape(&r))
}

/// `twig hook PID [COMMAND]`, from postcmd. Without COMMAND, the command is
/// read from the file postcmd wrote it to.
pub fn hook(args: &[String]) -> i32 {
    let Some(pid) = args.first().and_then(|a| a.parse::<u32>().ok()) else { return 2 };
    let (cmd, event) = if args.len() > 1 {
        (args[1..].join(" "), None)
    } else {
        let line = std::fs::read_to_string(paths::runtime_dir().join(format!("cmd.{pid}"))).unwrap_or_default();
        parse_history_line(&line)
    };
    let req = Request::Hook { pid, cmd, cwd: cwd(), event };
    let _ = proto::send(&req, Duration::from_millis(200));
    0
}

/// `twig reload`, `twig stop`.
pub fn simple(what: &str) -> i32 {
    let req = if what == "stop" { Request::Stop } else { Request::Reload };
    match proto::send(&req, Duration::from_secs(5)) {
        Ok(v) => {
            if let Some(e) = v["error"].as_str() {
                eprintln!("twig: {e}");
                return 1;
            }
            println!("{}", v["ok"].as_str().unwrap_or("done"));
            0
        }
        Err(_) => {
            eprintln!("twig: the daemon is not running");
            1
        }
    }
}

/// The box `twig timings` puts above the prompt: each step of drawing it,
/// with its time and share of the whole.
fn timings_box(t: &serde_json::Value, prep: Duration, connect: Duration, total: Duration) -> Vec<String> {
    let us = |d: Duration| d.as_micros() as u64;
    let total_us = us(total).max(1);
    let daemon_us = t["total"].as_u64().unwrap_or(0);
    // What the round trip took beyond the daemon's own work: writing and
    // reading the socket, and waiting to be scheduled.
    let wire_us = us(total).saturating_sub(us(prep) + us(connect) + daemon_us);
    let mut rows: Vec<(String, u64)> = vec![("prep".into(), us(prep)), ("connect".into(), us(connect)), ("daemon".into(), daemon_us)];
    for s in t["steps"].as_array().into_iter().flatten() {
        rows.push((format!("  {}", s[0].as_str().unwrap_or("")), s[1].as_u64().unwrap_or(0)));
    }
    rows.push(("wire".into(), wire_us));

    let dim = Style::of("dim");
    let ms = |u: u64| format!("{:>6.2} ms", u as f64 / 1000.0);
    let bar = |u: u64| {
        // Ten cells for the whole, in eighths.
        let eighths = (u * 80 / total_us) as usize;
        let mut b = "█".repeat(eighths / 8);
        if !eighths.is_multiple_of(8) {
            b.push(['▏', '▎', '▍', '▌', '▋', '▊', '▉'][eighths % 8 - 1]);
        }
        b
    };
    let mut body: Vec<String> = rows
        .iter()
        .map(|(name, u)| {
            let pct = u * 100 / total_us;
            let line = format!("{name:<11}{} {pct:>3}% ", ms(*u));
            let b = Style::of(if pct >= 25 { "yellow" } else { "cyan" }).paint(&format!("{:<10}", bar(*u)));
            if name.starts_with(' ') { format!("{}{b}", dim.paint(&line)) } else { format!("{line}{b}") }
        })
        .collect();
    if let Some(g) = t["git_status"].as_u64() {
        body.push(dim.paint(&format!("{:<11}{}  in the background", "git status", ms(g))));
    }
    let width = body.iter().map(|l| crate::util::visible_width(l)).max().unwrap_or(0);
    let title = format!("─ twig {:.2} ms ", total_us as f64 / 1000.0);
    let mut out = vec![dim.paint(&format!("╭{title}{}╮", "─".repeat((width + 2).saturating_sub(crate::util::visible_width(&title)))))];
    for l in body {
        let pad = " ".repeat(width - crate::util::visible_width(&l));
        out.push(format!("{} {l}{pad} {}", dim.paint("│"), dim.paint("│")));
    }
    out.push(dim.paint(&format!("╰{}╯", "─".repeat(width + 2))));
    out
}

/// `twig timings`: show, above each prompt of this shell, how long drawing
/// it took; again to stop.
pub fn timings() -> i32 {
    let pid = unsafe { libc::getppid() } as u32;
    match ask(&Request::Timings { pid }) {
        Ok(v) if v["timings"].as_bool() == Some(true) => {
            println!("timings on for this shell, from the next prompt; twig timings again to stop");
            0
        }
        Ok(_) => {
            println!("timings off");
            0
        }
        Err(e) => {
            eprintln!("twig: {e}");
            1
        }
    }
}

/// A line of tcsh's `history 1`: "   123\t14:02\tmake test" (the number, the
/// time, the command). Anything else is taken as the command alone.
fn parse_history_line(line: &str) -> (String, Option<u64>) {
    let line = line.trim_end_matches('\n');
    let mut f = line.splitn(3, '\t');
    match (f.next().and_then(|n| n.trim().parse().ok()), f.next(), f.next()) {
        (Some(n), Some(_), Some(cmd)) => (cmd.to_string(), Some(n)),
        _ => (line.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_lines() {
        assert_eq!(parse_history_line("   123\t14:02\tmake test\n"), ("make test".into(), Some(123)));
        assert_eq!(parse_history_line("     7\t9:05\techo a\tb"), ("echo a\tb".into(), Some(7)));
        assert_eq!(parse_history_line("make test"), ("make test".into(), None));
    }
}
