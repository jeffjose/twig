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
    let pid: u32 = args.first().and_then(|a| a.parse().ok()).unwrap_or_else(|| unsafe { libc::getppid() } as u32);
    let status: i32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(0);
    let cwd = cwd();
    let env: Vec<(String, String)> = env::vars().collect();
    let req = Request::Prompt { pid, status, cwd: cwd.clone(), env };
    let (p, r) = match proto::send(&req, PROMPT_LIMIT) {
        Ok(v) if v["prompt"].is_string() => {
            (v["prompt"].as_str().unwrap_or("").to_string(), v["right"].as_str().unwrap_or("").to_string())
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
    let cmd = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        std::fs::read_to_string(paths::runtime_dir().join(format!("cmd.{pid}"))).unwrap_or_default()
    };
    let req = Request::Hook { pid, cmd, cwd: cwd() };
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
