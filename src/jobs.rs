//! Jobs: commands handed to twig to run in the background. Their output goes
//! to a file instead of the terminal, they carry on when the shell closes,
//! and the prompt says when they are running and how they ended.
//!
//! `twig run` asks the daemon for a number and starts a keeper, a twig
//! process in a session of its own, which runs the command, waits for it
//! and reports how it ended. The keeper writes the job's file itself, so a
//! job is recorded correctly even when the daemon is not running.

use crate::client;
use crate::paths;
use crate::proto::{self, Request};
use crate::template::Style;
use crate::util::{atomic_write, fmt_ago, fmt_duration, now_ms, spawn_detached, tilde};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: u64,
    pub cmd: String,
    pub cwd: String,
    /// The shell it was started from.
    pub shell: u32,
    /// The command's process (and process group).
    #[serde(default)]
    pub pid: u32,
    pub start: u64,
    #[serde(default)]
    pub end: Option<u64>,
    #[serde(default)]
    pub status: Option<i32>,
}

impl Job {
    pub fn running(&self) -> bool {
        self.end.is_none() && (self.pid == 0 || crate::util::alive(self.pid))
    }

    /// A short name for the prompt: the command, cut to 24 columns.
    pub fn short(&self) -> String {
        let c: String = self.cmd.chars().take(24).collect();
        if c.len() < self.cmd.len() { format!("{c}…") } else { c }
    }
}

pub fn file(id: u64) -> PathBuf {
    paths::jobs_dir().join(format!("{id}.json"))
}

pub fn log_file(id: u64) -> PathBuf {
    paths::jobs_dir().join(format!("{id}.log"))
}

pub fn save(job: &Job) {
    if let Ok(b) = serde_json::to_vec_pretty(job) {
        let _ = atomic_write(&file(job.id), &b);
    }
}

pub fn load(id: u64) -> Option<Job> {
    serde_json::from_slice(&fs::read(file(id)).ok()?).ok()
}

/// Every job on record, oldest first.
pub fn load_all() -> Vec<Job> {
    let mut v: Vec<Job> = fs::read_dir(paths::jobs_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|f| f.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|f| serde_json::from_slice(&fs::read(f.path()).ok()?).ok())
        .collect();
    v.sort_by_key(|j| j.id);
    v
}

/// Forget all but the newest `keep` finished jobs.
pub fn prune(keep: usize) {
    let all = load_all();
    let done: Vec<&Job> = all.iter().filter(|j| !j.running()).collect();
    if done.len() > keep {
        for j in &done[..done.len() - keep] {
            let _ = fs::remove_file(file(j.id));
            let _ = fs::remove_file(log_file(j.id));
        }
    }
}

/// `twig run COMMAND...`
pub fn run(args: &[String]) -> i32 {
    let args: Vec<String> = if args.first().map(String::as_str) == Some("--") { args[1..].to_vec() } else { args.to_vec() };
    if args.is_empty() {
        eprintln!("usage: twig run COMMAND [ARGS...]\n\nRuns the command in the background; `twig log` shows its output.");
        return 2;
    }
    let cmd = args.join(" ");
    let cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let shell = unsafe { libc::getppid() } as u32;
    let id = match client::ask(&Request::JobStart { cmd: cmd.clone(), cwd, shell }) {
        Ok(v) => v["id"].as_u64().unwrap_or(0),
        Err(e) => {
            eprintln!("twig: {e}");
            return 1;
        }
    };
    let Ok(exe) = std::env::current_exe() else { return 1 };
    let mut keeper = Command::new(exe);
    keeper.arg("__keeper").arg(id.to_string()).arg("--").args(&args);
    if let Err(e) = spawn_detached(&mut keeper, Some(&log_file(id))) {
        eprintln!("twig: couldn't start the job: {e}");
        let _ = proto::send(&Request::JobEnd { id, status: 127 }, Duration::from_secs(2));
        return 1;
    }
    println!("[{id}] {cmd}   (twig log {id})");
    0
}

/// `twig __keeper ID -- COMMAND...`: run the command, wait, report.
pub fn keeper(args: &[String]) -> i32 {
    let Some(id) = args.first().and_then(|a| a.parse::<u64>().ok()) else { return 2 };
    let argv: Vec<&String> = args[1..].iter().skip_while(|a| *a == "--").collect();
    if argv.is_empty() {
        return 2;
    }
    let mut job = load(id).unwrap_or_else(|| Job {
        id,
        cmd: argv.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "),
        cwd: String::new(),
        shell: 0,
        pid: 0,
        start: now_ms(),
        end: None,
        status: None,
    });
    // A single argument is a command line for the shell; more are a
    // program and its arguments.
    let mut cmd = if argv.len() == 1 {
        let mut c = Command::new("sh");
        c.arg("-c").arg(argv[0]);
        c
    } else {
        let mut c = Command::new(argv[0]);
        c.args(&argv[1..]);
        c
    };
    // Its own process group, so `twig kill` stops it and what it started,
    // and not the keeper, which still has to say how it ended.
    unsafe {
        cmd.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        });
    }
    let status = match cmd.spawn() {
        Ok(mut child) => {
            job.pid = child.id();
            save(&job);
            let _ = proto::send(&Request::JobPid { id, pid: job.pid }, Duration::from_secs(2));
            match child.wait() {
                Ok(st) => st.code().unwrap_or_else(|| 128 + st.signal().unwrap_or(0)),
                Err(_) => 1,
            }
        }
        Err(e) => {
            eprintln!("twig: {}: {e}", argv[0]);
            127
        }
    };
    job.end = Some(now_ms());
    job.status = Some(status);
    save(&job);
    let _ = proto::send(&Request::JobEnd { id, status }, Duration::from_secs(5));
    0
}

fn pick(args: &[String]) -> Result<Job, String> {
    match args.iter().find(|a| !a.starts_with('-')) {
        Some(a) => {
            let id: u64 = a.trim_start_matches('%').parse().map_err(|_| format!("\"{a}\" is not a job number"))?;
            load(id).ok_or_else(|| format!("there is no job {id}"))
        }
        None => load_all().pop().ok_or_else(|| "no jobs yet; start one with twig run COMMAND".to_string()),
    }
}

/// `twig jobs`
pub fn list(args: &[String]) -> i32 {
    let all = load_all();
    let show_all = args.iter().any(|a| a == "-a" || a == "--all");
    let now = now_ms();
    let home = paths::home().to_string_lossy().into_owned();
    let color = unsafe { libc::isatty(1) } == 1;
    let paint = |s: &str, st: &str| if color { Style::of(st).paint(s) } else { s.to_string() };
    let list: Vec<&Job> = if show_all { all.iter().collect() } else { all.iter().rev().take(15).collect::<Vec<_>>().into_iter().rev().collect() };
    if list.is_empty() {
        println!("no jobs; start one with twig run COMMAND");
    }
    for j in list {
        let (state, took) = match (j.running(), j.status) {
            (true, _) => (paint("running", "cyan"), fmt_duration(now - j.start)),
            (false, Some(0)) => (paint("✔ done", "green"), fmt_duration(j.end.unwrap_or(now) - j.start)),
            (false, Some(s)) => (paint(&format!("✘ exit {s}"), "red"), fmt_duration(j.end.unwrap_or(now) - j.start)),
            (false, None) => (paint("lost", "yellow"), String::new()),
        };
        println!("{:>4}  {:<16} {:>7}  {:>4} ago  {}  {}", j.id, state, took, fmt_ago(j.start, now), j.cmd, paint(&tilde(&j.cwd, &home), "dim"));
    }
    0
}

/// `twig log [ID] [-f]`: the output of a job (the newest by default).
pub fn log(args: &[String]) -> i32 {
    let job = match pick(args) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("twig: {e}");
            return 1;
        }
    };
    let follow = args.iter().any(|a| a == "-f" || a == "--follow");
    let mut f = match fs::File::open(log_file(job.id)) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("twig: no output for job {}: {e}", job.id);
            return 1;
        }
    };
    let mut out = io::stdout().lock();
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    let _ = out.write_all(&buf);
    if follow {
        let mut pos = buf.len() as u64;
        loop {
            let _ = out.flush();
            thread::sleep(Duration::from_millis(200));
            buf.clear();
            let _ = f.seek(SeekFrom::Start(pos));
            let _ = f.read_to_end(&mut buf);
            pos += buf.len() as u64;
            if out.write_all(&buf).is_err() {
                return 0;
            }
            if load(job.id).is_none_or(|j| !j.running()) && buf.is_empty() {
                break;
            }
        }
    }
    let _ = out.flush();
    let job = load(job.id).unwrap_or(job);
    if !job.running() {
        let how = match job.status {
            Some(0) => "✔ done".to_string(),
            Some(s) => format!("✘ exit {s}"),
            None => "lost".to_string(),
        };
        eprintln!("── [{}] {}: {how} in {}", job.id, job.cmd, fmt_duration(job.end.unwrap_or(job.start) - job.start));
        let _ = proto::send(&Request::JobSeen { id: job.id }, Duration::from_secs(1));
    }
    0
}

/// `twig kill [ID]`
pub fn kill(args: &[String]) -> i32 {
    let job = match pick(args) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("twig: {e}");
            return 1;
        }
    };
    if !job.running() || job.pid == 0 {
        eprintln!("twig: job {} is not running", job.id);
        return 1;
    }
    let sig = if args.iter().any(|a| a == "-9") { libc::SIGKILL } else { libc::SIGTERM };
    if unsafe { libc::kill(-(job.pid as i32), sig) } != 0 {
        eprintln!("twig: couldn't stop job {}: {}", job.id, io::Error::last_os_error());
        return 1;
    }
    println!("stopped [{}] {}", job.id, job.cmd);
    0
}
