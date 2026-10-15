//! The daemon. It keeps everything slow away from the prompt: it computes git
//! status, runs probes, records history and watches jobs, and answers each
//! shell's precmd with a finished prompt.
//!
//! While a shell waits for you to type, the daemon may redraw the line above
//! the cursor when something on it changes (a git status, a notice, a job).
//! It does this only when the shell is in front on its terminal, not just
//! after drawing a prompt, and when the line fits the terminal.

use crate::config::{self, Loaded};
use crate::dirs::Dirs;
use crate::git::{self, GitStatus, Repo};
use crate::history::{self, Entry};
use crate::jobs::{self, Job};
use crate::notices::{Level, New, Notices};
use crate::paths;
use crate::probes::{self, Kind};
use crate::proto::{Ran, Request};
use crate::render::{self, Ctx};
use crate::tcsh;
use crate::util::{alive, clean, fmt_duration, now_ms, proc_groups, tty_columns, visible_width, write_tty};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// Time after answering a prompt during which the line is left alone: the
/// shell has yet to draw the prompt the daemon just sent.
const SETTLE: Duration = Duration::from_millis(300);

struct Shell {
    pid: u32,
    tty: Option<String>,
    cwd: String,
    env: HashMap<String, String>,
    /// The hash of `env` as the shell sent it (see proto::Request::Prompt).
    envh: u64,
    status: i32,
    duration_ms: Option<u64>,
    /// The command running now: (command line, start, directory).
    pending: Option<(String, u64, String)>,
    /// The history number of the last command line (see proto::Ran).
    last_event: Option<u64>,
    replied: Instant,
    time_secs: i64,
    cols: Option<u16>,
    /// The repository the shell is in.
    root: Option<PathBuf>,
    /// Show how long each part of drawing the prompt took (`twig timings`).
    timings: bool,
    /// The line above the cursor as last drawn.
    info: Option<String>,
    /// How many lines the prompt was drawn on. A redraw rewrites one line,
    /// so a change that would add or take away a line (a notice on a line
    /// of its own) waits for the next prompt.
    lines: usize,
    /// sshd is among the shell's parents: it came in over ssh, whatever its
    /// environment says.
    ssh: bool,
    /// The shell runs as root (its effective user).
    as_root: bool,
}

impl Shell {
    fn new(pid: u32) -> Shell {
        Shell {
            pid,
            ssh: over_ssh(Path::new("/proc"), pid),
            as_root: euid(Path::new("/proc"), pid) == Some(0),
            tty: tty_of(pid),
            cwd: String::new(),
            env: HashMap::new(),
            envh: 0,
            status: 0,
            duration_ms: None,
            pending: None,
            last_event: None,
            replied: Instant::now(),
            time_secs: 0,
            cols: None,
            root: None,
            timings: false,
            info: None,
            lines: 0,
        }
    }

    /// A command is running: postcmd wrote it to a file as it started, and
    /// the prompt after it takes the file away.
    fn busy(&self, rt: &Path) -> bool {
        rt.join(format!("cmd.{}", self.pid)).exists()
    }

    /// Whether the daemon may draw on this shell's terminal now.
    fn waiting_at_prompt(&self, rt: &Path) -> bool {
        if self.replied.elapsed() < SETTLE || self.info.is_none() || self.busy(rt) {
            return false;
        }
        let Some(tty) = &self.tty else { return false };
        // In front of its terminal: no command is running in it.
        if !matches!(proc_groups(self.pid), Some((pgrp, tpgid)) if pgrp == tpgid) {
            return false;
        }
        // Not resized since the prompt was drawn (a narrower terminal may
        // have wrapped the line).
        tty_columns(tty) == self.cols
    }
}

/// A process's effective user, from the Uid line of /proc/PID/status
/// (real, effective, saved, filesystem).
fn euid(proc: &Path, pid: u32) -> Option<u32> {
    let status = fs::read_to_string(proc.join(format!("{pid}/status"))).ok()?;
    status.lines().find_map(|l| l.strip_prefix("Uid:"))?.split_whitespace().nth(1)?.parse().ok()
}

/// Whether sshd (or sshd-session, as newer OpenSSH calls the process of
/// one login) is among a process's parents.
fn over_ssh(proc: &Path, pid: u32) -> bool {
    let mut p = pid;
    for _ in 0..64 {
        if p <= 1 {
            return false;
        }
        let Ok(stat) = fs::read_to_string(proc.join(format!("{p}/stat"))) else { return false };
        let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else { return false };
        if p != pid && stat[open + 1..close].starts_with("sshd") {
            return true;
        }
        let Some(ppid) = stat[close + 2..].split_whitespace().nth(1).and_then(|x| x.parse().ok()) else { return false };
        p = ppid;
    }
    false
}

/// A shell's terminal, from its controlling tty in /proc (its standard
/// input isn't always the terminal: tcsh moves it).
fn tty_of(pid: u32) -> Option<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 2..];
    let nr: u64 = rest.split_whitespace().nth(4)?.parse().ok()?;
    let major = (nr >> 8) & 0xfff;
    let minor = (nr & 0xff) | ((nr >> 12) & 0xfff00);
    let path = match major {
        136..=143 => format!("/dev/pts/{}", (major - 136) * 256 + minor),
        4 if minor < 64 => format!("/dev/tty{minor}"),
        _ => return None,
    };
    std::path::Path::new(&path).exists().then_some(path)
}

struct RepoState {
    repo: Repo,
    status: Option<GitStatus>,
    error: Option<String>,
    /// Something may have changed since the status was taken.
    stale: bool,
    refreshing: bool,
    again: bool,
    checked: Instant,
    cost: Duration,
    spent: Spent,
    fetched: Instant,
    fetching: bool,
    watches: Vec<i32>,
}

struct Slot {
    name: String,
    kind: Kind,
    every: Duration,
    next: Instant,
    running: bool,
    last: Option<Instant>,
    spent: Spent,
}

/// What something has cost so far, for `twig info`.
#[derive(Clone, Copy, Default)]
struct Spent {
    runs: u64,
    total_us: u64,
    last_us: u64,
    max_us: u64,
}

impl Spent {
    fn add(&mut self, d: Duration) {
        let us = d.as_micros() as u64;
        self.runs += 1;
        self.total_us += us;
        self.last_us = us;
        self.max_us = self.max_us.max(us);
    }

    fn json(&self) -> Value {
        json!({ "runs": self.runs, "last_us": self.last_us, "max_us": self.max_us, "avg_us": self.total_us.checked_div(self.runs).unwrap_or(0) })
    }
}

struct State {
    cfg: Arc<Loaded>,
    cfg_mtime: Option<SystemTime>,
    cfg_error: Option<String>,
    host: String,
    facts: HashMap<String, String>,
    shells: HashMap<u32, Shell>,
    repos: HashMap<PathBuf, RepoState>,
    notices: Notices,
    jobs: BTreeMap<u64, Job>,
    next_job: u64,
    history: Vec<Entry>,
    history_dirty: Option<Instant>,
    dirs: Dirs,
    dirs_saved: Instant,
    slots: Vec<Slot>,
    inotify: i32,
    watches: HashMap<i32, PathBuf>,
    /// The runtime directory (the socket, the shells' command files).
    rt: PathBuf,
    started: Instant,
    /// The daemon's steps of the last prompts, for `twig info`.
    prompts: std::collections::VecDeque<Vec<(&'static str, u64)>>,
    /// The daemon's last run of work in the background (a probe, a git
    /// status, a fetch): when it ended and how long it took, for
    /// `twig timings`.
    last_work: Option<(Instant, Duration)>,
}

impl State {
    fn worked(&mut self, took: Duration) {
        self.last_work = Some((Instant::now(), took));
    }
}

struct Shared {
    st: Mutex<State>,
    cv: Condvar,
}

type S = Arc<Shared>;

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub fn run() -> i32 {
    let rt = paths::runtime_dir_made();
    let lock = match fs::OpenOptions::new().create(true).truncate(false).write(true).open(rt.join("lock")) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("twig: {}: {e}", rt.display());
            return 1;
        }
    };
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        eprintln!("twig: the daemon is already running");
        return 0;
    }
    let sock = paths::socket();
    let _ = fs::remove_file(&sock);
    let listener = match UnixListener::bind(&sock) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("twig: {}: {e}", sock.display());
            return 1;
        }
    };
    let _ = fs::set_permissions(&sock, fs::Permissions::from_mode(0o600));
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };

    let host = paths::hostname();
    if !paths::history_file(&host).exists() {
        let old = paths::home().join(".history");
        if old.exists() {
            history::import(&old);
        }
    }
    let job_list = jobs::load_all();
    let mut st = State {
        cfg: Arc::new(config::load(config::Config::default()).expect("defaults load")),
        cfg_mtime: None,
        cfg_error: None,
        facts: HashMap::new(),
        shells: HashMap::new(),
        repos: HashMap::new(),
        notices: Notices::load(),
        next_job: job_list.last().map_or(1, |j| j.id + 1),
        jobs: job_list.into_iter().map(|j| (j.id, j)).collect(),
        history: history::load_all(),
        history_dirty: Some(Instant::now() - Duration::from_secs(60)),
        dirs: Dirs::load(&host),
        dirs_saved: Instant::now(),
        slots: Vec::new(),
        inotify: unsafe { libc::inotify_init1(libc::IN_CLOEXEC) },
        watches: HashMap::new(),
        rt: rt.clone(),
        started: Instant::now(),
        prompts: std::collections::VecDeque::new(),
        last_work: None,
        host,
    };
    reload(&mut st, true);
    let sh: S = Arc::new(Shared { st: Mutex::new(st), cv: Condvar::new() });
    eprintln!("twig: daemon {} listening on {}", std::process::id(), sock.display());
    // Shells read this to know whose daemon it is: one that systemd runs is
    // restarted by its watchdog, one that a shell started by the shells.
    let by = if std::env::var_os("NOTIFY_SOCKET").is_some() { "systemd" } else { "shell" };
    let _ = fs::write(rt.join("pid"), format!("{} {by}\n", std::process::id()));
    // A new daemon: shells that gave up on the last one can come back.
    let _ = fs::remove_file(rt.join("unanswered"));
    sd_notify("READY=1");

    let s2 = sh.clone();
    thread::spawn(move || tick_loop(s2));
    let s3 = sh.clone();
    thread::spawn(move || watch_loop(s3));

    // Connections go to threads that wait for them: handing one over
    // takes 5 µs, starting a thread for it 20. `idle` is the threads
    // waiting less the connections waiting; when it goes below zero
    // (every thread is busy, most likely waiting for git) one more thread
    // is started, so no prompt queues behind another.
    let (tx, rx) = mpsc::channel::<(UnixStream, Instant)>();
    let rx = Arc::new(Mutex::new(rx));
    let idle = Arc::new(AtomicIsize::new(0));
    let worker = |sh: S, rx: Arc<Mutex<mpsc::Receiver<(UnixStream, Instant)>>>, idle: Arc<AtomicIsize>| {
        thread::spawn(move || loop {
            idle.fetch_add(1, Ordering::SeqCst);
            let job = rx.lock().unwrap_or_else(|e| e.into_inner()).recv();
            let Ok((conn, t0)) = job else { return };
            serve(&sh, conn, t0);
        })
    };
    for conn in listener.incoming() {
        let Ok(conn) = conn else { continue };
        if idle.fetch_sub(1, Ordering::SeqCst) <= 0 {
            worker(sh.clone(), rx.clone(), idle.clone());
        }
        let _ = tx.send((conn, Instant::now()));
    }
    drop(lock);
    0
}

/// One connection: a request, its answer. `t0` is when it was accepted.
/// A prompt's history entry is written to the file after the answer is
/// on its way: the shell doesn't wait for that.
fn serve(sh: &S, mut conn: UnixStream, t0: Instant) {
    let got = Instant::now();
    let _ = conn.set_read_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    if BufReader::new(&conn).read_line(&mut line).is_err() {
        return;
    }
    let (reply, later) = match serde_json::from_str::<Request>(&line) {
        Ok(Request::Prompt { pid, status, cwd, envh, env, ran }) => prompt(sh, pid, status, cwd, envh, env, ran, t0, got),
        Ok(req) => (handle(sh, req), None),
        Err(e) => (json!({ "error": format!("bad request: {e}") }), None),
    };
    let _ = conn.write_all(reply.to_string().as_bytes());
    drop(conn);
    if let Some(e) = later {
        if let Err(err) = history::append(&e.h, &e) {
            eprintln!("twig: history: {err}");
        }
    }
}

fn handle(sh: &S, req: Request) -> Value {
    match req {
        Request::Prompt { .. } => json!({ "error": "a prompt is served apart" }),
        Request::Timings { pid } => {
            let mut st = sh.lock();
            let s = st.shells.entry(pid).or_insert_with(|| Shell::new(pid));
            s.timings = !s.timings;
            json!({ "timings": s.timings })
        }
        Request::Hook { pid, cmd, cwd, event } => {
            let mut st = sh.lock();
            let keep = history::keep(&cmd, &st.cfg.raw.history.ignore);
            let root = git::find(Path::new(&cwd)).map(|r| r.root);
            let shell = st.shells.entry(pid).or_insert_with(|| Shell::new(pid));
            // The same history number again: an empty line, nothing runs.
            if event.is_some() && event == shell.last_event {
                return json!({});
            }
            shell.last_event = event.or(shell.last_event);
            if keep && shell.pending.is_none() {
                shell.pending = Some((cmd.trim().to_string(), now_ms(), cwd));
            }
            if let Some(r) = root.and_then(|r| st.repos.get_mut(&r)) {
                r.stale = true;
            }
            json!({})
        }
        Request::Notify { text, level, ttl, shows, pid, key } => {
            let Some(level) = Level::parse(&level) else { return json!({"error": "level is info, warn or error"}) };
            sh.lock().notices.put(New { key, text: clean(&text), level, ttl_ms: ttl, shows, pid });
            redraw(sh, |_| true);
            json!({})
        }
        Request::Ack { id, pid } => {
            let n = sh.lock().notices.ack(id, pid);
            redraw(sh, |_| true);
            json!({ "acked": n })
        }
        Request::Notices => {
            let st = sh.lock();
            json!({ "notices": st.notices.list, "past": st.notices.past, "muted": st.notices.muted })
        }
        Request::Mute { pattern, undo } => {
            let mut st = sh.lock();
            let reply = if undo {
                json!({ "unmuted": st.notices.unmute(&pattern) })
            } else {
                json!({ "gone": st.notices.mute(&pattern) })
            };
            // The units check runs again now, with the pattern (or without).
            for slot in st.slots.iter_mut().filter(|s| s.kind == Kind::Units) {
                slot.next = Instant::now();
            }
            drop(st);
            redraw(sh, |_| true);
            reply
        }
        Request::JobStart { cmd, cwd, shell } => {
            let mut st = sh.lock();
            let id = st.next_job;
            st.next_job += 1;
            let job = Job { id, cmd: clean(&cmd), cwd, shell, pid: 0, start: now_ms(), end: None, status: None };
            jobs::save(&job);
            st.jobs.insert(id, job);
            drop(st);
            redraw(sh, |_| true);
            json!({ "id": id })
        }
        Request::JobPid { id, pid } => {
            if let Some(j) = sh.lock().jobs.get_mut(&id) {
                j.pid = pid;
            }
            json!({})
        }
        Request::JobEnd { id, status } => {
            job_ended(sh, id, status);
            json!({})
        }
        Request::JobSeen { id } => {
            if sh.lock().notices.clear(&format!("job:{id}")) {
                redraw(sh, |_| true);
            }
            json!({})
        }
        Request::Jobs => json!({ "jobs": sh.lock().jobs.values().collect::<Vec<_>>() }),
        Request::Status => status(sh),
        Request::Cost => cost(sh),
        Request::Reload => {
            let mut st = sh.lock();
            st.cfg_mtime = None;
            reload(&mut st, true);
            match &st.cfg_error {
                Some(e) => json!({ "error": e }),
                None => json!({ "ok": "settings read again" }),
            }
        }
        Request::Stop => {
            let mut st = sh.lock();
            save_all(&mut st);
            thread::spawn(|| {
                thread::sleep(Duration::from_millis(50));
                let _ = fs::remove_file(paths::socket());
                std::process::exit(0);
            });
            json!({ "ok": "stopped" })
        }
    }
}

/// The answer to a prompt, and the history entry to write once it is sent.
#[allow(clippy::too_many_arguments)]
fn prompt(
    sh: &S,
    pid: u32,
    status: i32,
    cwd: String,
    envh: u64,
    env: Option<Vec<(String, String)>>,
    ran: Option<Ran>,
    t0: Instant,
    got: Instant,
) -> (Value, Option<Entry>) {
    // Marks for `twig timings`: the time at the end of each step.
    let mut marks: Vec<(&'static str, Instant)> = vec![("queue", got), ("read", Instant::now())];
    let now = now_ms();
    let mut st = sh.lock();
    marks.push(("lock", Instant::now()));
    let cfg = st.cfg.clone();
    let host = st.host.clone();

    let shell = st.shells.entry(pid).or_insert_with(|| Shell::new(pid));
    // The environment, kept from when it was last sent; when the shell's
    // has changed (or this is its first prompt), it is asked for.
    match env {
        Some(e) => {
            shell.env = e.into_iter().collect();
            shell.envh = envh;
        }
        None if shell.envh != envh => return (json!({ "env": "?" }), None),
        None => {}
    }
    // The command that ran: from the hook of an older shell, else brought
    // by the prompt. The same history number again is an empty line:
    // nothing ran. It ran where the shell was before this prompt.
    let ran = shell.pending.take().or_else(|| {
        let r = ran?;
        let again = r.event.is_some() && r.event == shell.last_event;
        shell.last_event = r.event.or(shell.last_event);
        if again || !history::keep(&r.cmd, &cfg.raw.history.ignore) {
            return None;
        }
        let there = if shell.cwd.is_empty() { cwd.clone() } else { shell.cwd.clone() };
        Some((r.cmd.trim().to_string(), r.start, there))
    });
    let moved = shell.cwd != cwd;
    shell.duration_ms = ran.as_ref().map(|r| now.saturating_sub(r.1));
    shell.status = status;
    shell.cwd = cwd.clone();
    // A prompt is on its way: no redraws until it has been drawn (SETTLE
    // after the answer). Not only after it: the git status this prompt
    // waits for may finish, and redraw, before then, and twig prompt runs
    // in the shell's own process group, so the shell looks idle meanwhile.
    shell.replied = Instant::now();
    shell.time_secs = (now / 1000) as i64;
    if shell.tty.is_none() {
        shell.tty = tty_of(pid);
    }
    shell.cols = shell.tty.as_deref().and_then(tty_columns);

    let later = ran.as_ref().map(|(cmd, start, wd)| {
        let e = Entry { t: *start, d: now.saturating_sub(*start), s: Some(status), c: cmd.clone(), w: wd.clone(), h: host.clone(), p: pid };
        st.history.push(e.clone());
        st.history_dirty.get_or_insert_with(Instant::now);
        e
    });
    if moved {
        st.dirs.visit(&cwd);
    }
    marks.push(("state", Instant::now()));

    let mut root = None;
    if cfg.raw.git.enabled {
        if let Some(repo) = git::find(Path::new(&cwd)) {
            marks.push(("git find", Instant::now()));
            let r = repo.root.clone();
            if !st.repos.contains_key(&r) {
                let watches = add_watches(&mut st, &repo);
                st.repos.insert(r.clone(), RepoState {
                    repo,
                    status: None,
                    error: None,
                    stale: true,
                    refreshing: false,
                    again: false,
                    checked: Instant::now(),
                    cost: Duration::ZERO,
                    fetched: Instant::now(),
                    fetching: false,
                    watches,
                    spent: Spent::default(),
                });
            }
            let rs = st.repos.get_mut(&r).expect("just added");
            let poll_due = cfg.git_poll.is_some_and(|p| rs.checked.elapsed() > p);
            if ran.is_some() || rs.status.is_none() || poll_due {
                rs.stale = true;
            }
            if rs.stale {
                start_refresh(sh, &mut st, &r);
            }
            let deadline = Instant::now() + cfg.git_wait;
            while st.repos.get(&r).is_some_and(|rs| rs.refreshing) {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                st = sh.cv.wait_timeout(st, left).unwrap_or_else(|e| e.into_inner()).0;
            }
            root = Some(r);
        }
    }
    marks.push(("git wait", Instant::now()));
    if let Some(s) = st.shells.get_mut(&pid) {
        s.root = root;
    }

    let Some((p, right)) = render_for(&st, pid, now) else { return (json!({ "error": "no shell" }), later) };
    marks.push(("render", Instant::now()));
    st.notices.shown(pid, now);
    if let Some(s) = st.shells.get_mut(&pid) {
        s.info = render::info_line(&p).map(String::from);
        s.lines = p.split('\n').count();
        s.replied = Instant::now();
    }
    let (p, right) = (tcsh::escape(&p), tcsh::escape(&right));
    marks.push(("escape", Instant::now()));
    let mut reply = json!({ "prompt": p, "right": right });
    // Microseconds per step, in order, kept for `twig info`. A step that
    // didn't happen (no repository) is not there.
    let mut steps: Vec<(&'static str, u64)> = Vec::new();
    let mut prev = t0;
    for (name, at) in &marks {
        steps.push((name, at.duration_since(prev).as_micros() as u64));
        prev = *at;
    }
    steps.push(("total", prev.duration_since(t0).as_micros() as u64));
    if st.shells.get(&pid).is_some_and(|s| s.timings) {
        let list: Vec<Value> = steps[..steps.len() - 1].iter().map(|(n, u)| json!([n, u])).collect();
        // What the daemon costs, all told, for reference.
        // The daemon's last run in the background, for reference.
        let background = st.last_work.map(|(at, d)| json!({ "us": d.as_micros() as u64, "ago_s": at.elapsed().as_secs() }));
        reply["timings"] = json!({ "steps": list, "total": steps[steps.len() - 1].1, "background": background });
    }
    if st.prompts.len() >= 1000 {
        st.prompts.pop_front();
    }
    st.prompts.push_back(steps);
    (reply, later)
}

fn render_for(st: &State, pid: u32, now: u64) -> Option<(String, String)> {
    let sh = st.shells.get(&pid)?;
    let rs = sh.root.as_ref().and_then(|r| st.repos.get(r));
    let fqdn = st.facts.get("fqdn").map(String::as_str).unwrap_or(&st.host);
    let ctx = Ctx {
        cfg: &st.cfg,
        host: &st.host,
        fqdn,
        cwd: &sh.cwd,
        env: &sh.env,
        status: sh.status,
        duration_ms: sh.duration_ms,
        time_secs: sh.time_secs,
        facts: &st.facts,
        branch: rs.filter(|r| r.status.is_none()).and_then(|r| git::head(&r.repo)),
        git: rs.and_then(|r| r.status.as_ref()),
        notice: st.notices.top(pid, now),
        jobs: jobs_text(&st.jobs, now),
        ssh: sh.ssh,
        root: sh.as_root,
    };
    Some(render::render(&ctx, sh.cols))
}

fn jobs_text(jobs: &BTreeMap<u64, Job>, now: u64) -> String {
    let running: Vec<&Job> = jobs.values().filter(|j| j.end.is_none() && (j.pid == 0 || alive(j.pid))).collect();
    match running.as_slice() {
        [] => String::new(),
        [j] => format!("⏵ {} {}", j.short(), fmt_duration(now.saturating_sub(j.start).max(1000) / 1000 * 1000)),
        v => format!("⏵ {} jobs", v.len()),
    }
}

/// Start computing a repository's status, or ask for another round when one
/// is under way.
fn start_refresh(sh: &S, st: &mut State, root: &Path) {
    let untracked = st.cfg.raw.git.untracked;
    let Some(r) = st.repos.get_mut(root) else { return };
    if r.refreshing {
        r.again = true;
        return;
    }
    r.refreshing = true;
    r.stale = false;
    let repo = r.repo.clone();
    let mut last = r.status.clone();
    let sh = sh.clone();
    let root = root.to_path_buf();
    thread::spawn(move || {
        let mut changed = false;
        loop {
            let t = Instant::now();
            let res = git::status(&repo, untracked, Duration::from_secs(30), last.as_ref());
            if let Ok(s) = &res {
                last = Some(s.clone());
            }
            let mut st = sh.lock();
            let key = format!("git:{}", root.display());
            // A failure is said on the prompt, so stale numbers aren't taken
            // for fresh ones.
            changed |= match &res {
                Ok(_) => st.notices.clear(&key),
                Err(e) => {
                    let home = paths::home().to_string_lossy().into_owned();
                    let text = format!("{e} in {}", crate::util::tilde(&root.to_string_lossy(), &home));
                    st.notices.put(New { key: Some(key), text, level: Level::Warn, ttl_ms: None, shows: None, pid: None })
                }
            };
            let took = t.elapsed();
            st.worked(took);
            let Some(r) = st.repos.get_mut(&root) else { return };
            r.cost = took;
            r.spent.add(took);
            r.checked = Instant::now();
            match res {
                Ok(s) => {
                    changed |= r.status.as_ref() != Some(&s);
                    r.status = Some(s);
                    r.error = None;
                }
                Err(e) => r.error = Some(e),
            }
            if r.again {
                r.again = false;
                r.stale = false;
                continue;
            }
            r.refreshing = false;
            sh.cv.notify_all();
            break;
        }
        if changed {
            redraw(&sh, |s| s.root.as_deref() == Some(root.as_path()));
        }
    });
}

/// Redraw the line above the cursor in the shells that `which` picks, where
/// it changed and it is safe to.
fn redraw(sh: &S, which: impl Fn(&Shell) -> bool) {
    let mut writes = Vec::new();
    {
        let mut st = sh.lock();
        if !st.cfg.raw.live.enabled {
            return;
        }
        let now = now_ms();
        let pids: Vec<u32> = st.shells.values().filter(|s| which(s)).map(|s| s.pid).collect();
        for pid in pids {
            if !st.shells.get(&pid).is_some_and(|s| s.waiting_at_prompt(&st.rt)) {
                continue;
            }
            let Some((p, _)) = render_for(&st, pid, now) else { continue };
            let Some(line) = render::info_line(&p) else { continue };
            let s = st.shells.get_mut(&pid).expect("listed");
            if s.info.as_deref() == Some(line) || p.split('\n').count() != s.lines {
                continue;
            }
            let cols = s.cols.unwrap_or(80) as usize;
            if visible_width(line) >= cols || s.info.as_deref().is_some_and(|o| visible_width(o) >= cols) {
                continue;
            }
            s.info = Some(line.to_string());
            if let Some(tty) = &s.tty {
                // Save the cursor, go up a line, write it, come back.
                writes.push((tty.clone(), format!("\x1b7\x1b[1A\r{line}\x1b[0m\x1b[K\x1b8")));
            }
        }
    }
    for (tty, bytes) in writes {
        let _ = write_tty(&tty, bytes.as_bytes());
    }
}

fn job_ended(sh: &S, id: u64, status: i32) {
    let mut st = sh.lock();
    let now = now_ms();
    let desktop_after = st.cfg.desktop_after;
    let Some(j) = st.jobs.get_mut(&id) else { return };
    if j.end.is_some() {
        return;
    }
    j.end = Some(now);
    j.status = Some(status);
    let j = j.clone();
    let took = now.saturating_sub(j.start);
    let shell = st.shells.contains_key(&j.shell).then_some(j.shell);
    let n = if status == 0 {
        New {
            key: Some(format!("job:{id}")),
            text: format!("✔ {} {}", j.short(), fmt_duration(took)),
            level: Level::Info,
            ttl_ms: Some(30 * 60_000),
            shows: Some(3),
            pid: shell,
        }
    } else {
        New {
            key: Some(format!("job:{id}")),
            text: format!("✘ {} exit {status}, twig log {id}", j.short()),
            level: Level::Error,
            ttl_ms: None,
            shows: None,
            pid: shell,
        }
    };
    st.notices.put(n);
    let keep = st.cfg.raw.jobs.keep;
    drop(st);
    if desktop_after.is_some_and(|d| took >= d.as_millis() as u64) {
        let title = if status == 0 { format!("✔ {}", j.cmd) } else { format!("✘ {} (exit {status})", j.cmd) };
        let _ = Command::new("notify-send").args(["-a", "twig", &title, &format!("took {}", fmt_duration(took))]).spawn().map(|mut c| {
            thread::spawn(move || c.wait());
        });
    }
    jobs::prune(keep);
    redraw(sh, |_| true);
}

/// Read the settings when the file has changed (or `force`). A broken file
/// leaves the last good settings in place and says so on the prompt.
fn reload(st: &mut State, force: bool) -> bool {
    let path = paths::config_file();
    let mtime = fs::metadata(&path).and_then(|m| m.modified()).ok();
    if !force && mtime == st.cfg_mtime {
        return false;
    }
    st.cfg_mtime = mtime;
    match config::read(&path) {
        Ok(cfg) => {
            st.cfg = Arc::new(cfg);
            st.cfg_error = None;
            st.notices.clear("config");
            let fqdn = st.facts.get("fqdn").cloned().unwrap_or_else(|| st.host.clone());
            let old: HashMap<String, (Instant, bool, Option<Instant>, Spent)> =
                st.slots.iter().map(|s| (s.name.clone(), (s.next, s.running, s.last, s.spent))).collect();
            st.slots = probes::schedule(&st.cfg, &fqdn, &st.host)
                .into_iter()
                .map(|(name, kind, every)| {
                    let (next, running, last, spent) = old.get(&name).copied().unwrap_or((Instant::now(), false, None, Spent::default()));
                    Slot { name, kind, every, next, running, last, spent }
                })
                .collect();
            // Facts and notices of probes that no longer exist.
            let names: Vec<String> = st.cfg.probes.iter().map(|p| p.0.name.clone()).collect();
            let gone: Vec<String> = st.notices.list.iter().filter_map(|n| n.key.clone()).filter(|k| k.strip_prefix("probe:").is_some_and(|n| !names.iter().any(|x| x == n))).collect();
            for k in gone {
                st.notices.clear(&k);
            }
        }
        Err(e) => {
            let what = config::summary(&e, fs::read_to_string(&path).ok().as_deref());
            eprintln!("twig: settings: {e}");
            st.cfg_error = Some(e);
            st.notices.put(New {
                key: Some("config".into()),
                text: format!("twig settings not read: {what} (twig info)"),
                level: Level::Error,
                ttl_ms: None,
                shows: None,
                pid: None,
            });
        }
    }
    true
}

fn tick_loop(sh: S) {
    loop {
        thread::sleep(Duration::from_secs(1));
        let mut redraw_all = false;
        let now = now_ms();
        let mut st = sh.lock();
        // Alive: the state can be locked. A daemon that stops saying so (stuck,
        // or stopped) is restarted by systemd (WatchdogSec in the unit).
        sd_notify("WATCHDOG=1");
        redraw_all |= reload(&mut st, false);
        redraw_all |= st.notices.expire(now);

        // Shells that have gone.
        let dead: Vec<u32> = st.shells.keys().copied().filter(|&p| !alive(p)).collect();
        for p in dead {
            st.shells.remove(&p);
            st.notices.orphan(p);
            let _ = fs::remove_file(st.rt.join(format!("cmd.{p}")));
            let _ = fs::remove_file(st.rt.join(format!("pick.{p}")));
        }
        // And the files of shells that never drew a prompt (a script with
        // the aliases runs postcmd, not precmd) and have gone.
        if (now / 1000).is_multiple_of(60) {
            for f in fs::read_dir(&st.rt).into_iter().flatten().flatten() {
                let name = f.file_name();
                let name = name.to_string_lossy();
                let pid = name.strip_prefix("cmd.").or_else(|| name.strip_prefix("pick.")).and_then(|p| p.parse::<u32>().ok());
                if pid.is_some_and(|p| !alive(p)) {
                    let _ = fs::remove_file(f.path());
                }
            }
        }

        // Jobs whose keeper was killed before it could report.
        let lost: Vec<u64> = st.jobs.values().filter(|j| j.end.is_none() && j.pid != 0 && !alive(j.pid)).map(|j| j.id).collect();
        for id in lost {
            let ended = jobs::load(id).and_then(|j| j.status);
            drop(st);
            job_ended(&sh, id, ended.unwrap_or(-1));
            st = sh.lock();
        }
        // A running job's time on the prompt.
        if st.jobs.values().any(|j| j.end.is_none()) && (now / 1000).is_multiple_of(10) {
            redraw_all = true;
        }

        // Repositories: drop those no shell has been in for a while, look
        // again at the others now and then, and fetch.
        let used: Vec<PathBuf> = st.shells.values().filter_map(|s| s.root.clone()).collect();
        let unused: Vec<PathBuf> =
            st.repos.iter().filter(|(r, s)| !used.contains(r) && s.checked.elapsed() > Duration::from_secs(600)).map(|(r, _)| r.clone()).collect();
        for r in unused {
            if let Some(rs) = st.repos.remove(&r) {
                for wd in rs.watches {
                    unsafe { libc::inotify_rm_watch(st.inotify, wd) };
                    st.watches.remove(&wd);
                }
            }
        }
        let (poll, fetch) = (st.cfg.git_poll, st.cfg.git_fetch);
        for r in used {
            let Some(rs) = st.repos.get_mut(&r) else { continue };
            if poll.is_some_and(|p| rs.checked.elapsed() > p && rs.cost < Duration::from_secs(2)) && !rs.refreshing {
                start_refresh(&sh, &mut st, &r);
                continue;
            }
            if let Some(f) = fetch {
                if !rs.fetching && rs.status.as_ref().is_some_and(|s| s.upstream) && rs.fetched.elapsed() > f {
                    rs.fetching = true;
                    let (repo, sh2, root) = (rs.repo.clone(), sh.clone(), r.clone());
                    thread::spawn(move || {
                        let t = Instant::now();
                        git::fetch(&repo, Duration::from_secs(120));
                        let mut st = sh2.lock();
                        st.worked(t.elapsed());
                        if let Some(rs) = st.repos.get_mut(&root) {
                            rs.fetching = false;
                            rs.fetched = Instant::now();
                        }
                        start_refresh(&sh2, &mut st, &root);
                    });
                }
            }
        }

        // Probes that are due.
        let cfg = st.cfg.clone();
        let muted = st.notices.muted.clone();
        for i in 0..st.slots.len() {
            let slot = &mut st.slots[i];
            if slot.running || slot.next > Instant::now() {
                continue;
            }
            slot.running = true;
            let (kind, name, cfg, sh2, muted) = (slot.kind.clone(), slot.name.clone(), cfg.clone(), sh.clone(), muted.clone());
            thread::spawn(move || {
                let t = Instant::now();
                let out = probes::run(&kind, &cfg, &muted);
                let took = t.elapsed();
                let mut st = sh2.lock();
                st.worked(took);
                if let Some(slot) = st.slots.iter_mut().find(|s| s.name == name) {
                    slot.spent.add(took);
                    slot.running = false;
                    slot.next = Instant::now() + slot.every;
                    slot.last = Some(Instant::now());
                }
                let mut changed = false;
                let mut fqdn_changed = false;
                for (k, v) in out.facts {
                    let before = st.facts.get(&k).cloned();
                    if before != v {
                        changed = true;
                        fqdn_changed |= k == "fqdn";
                        match v {
                            Some(v) => st.facts.insert(k, v),
                            None => st.facts.remove(&k),
                        };
                    }
                }
                for (key, n) in out.notices {
                    changed |= match n {
                        Some(n) => st.notices.put(n),
                        None => st.notices.clear(&key),
                    };
                }
                if fqdn_changed {
                    // Probes limited to some hosts may apply now.
                    reload(&mut st, true);
                }
                drop(st);
                if changed {
                    redraw(&sh2, |_| true);
                }
            });
        }

        // History for new shells' arrow keys, and the directories.
        if st.history_dirty.is_some_and(|t| t.elapsed() > Duration::from_secs(2)) {
            st.history_dirty = None;
            let size = st.cfg.raw.history.size;
            if let Err(e) = history::export_tcsh(&st.history, size, &paths::tcsh_history()) {
                eprintln!("twig: history: {e}");
            }
        }
        if st.dirs_saved.elapsed() > Duration::from_secs(10) {
            let host = st.host.clone();
            st.dirs.save(&host);
            st.dirs_saved = Instant::now();
        }
        drop(st);
        if redraw_all {
            redraw(&sh, |_| true);
        }
    }
}

fn save_all(st: &mut State) {
    let host = st.host.clone();
    st.dirs.save(&host);
    let size = st.cfg.raw.history.size;
    let _ = history::export_tcsh(&st.history, size, &paths::tcsh_history());
}

// ---- watching repositories for changes made by git ----

/// Files in a git directory whose change means the status may have changed.
const WATCHED: [&str; 10] =
    ["HEAD", "index", "MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD", "REBASE_HEAD", "ORIG_HEAD", "packed-refs", "stash", "rebase-merge"];

fn add_watches(st: &mut State, repo: &Repo) -> Vec<i32> {
    let mut wds = Vec::new();
    if st.inotify < 0 {
        return wds;
    }
    let mut dirs = vec![repo.gitdir.clone(), repo.common.join("logs"), repo.common.join("logs/refs")];
    dirs.dedup();
    for d in dirs {
        let Ok(c) = std::ffi::CString::new(d.to_string_lossy().as_bytes()) else { continue };
        let mask = libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_CREATE | libc::IN_DELETE;
        let wd = unsafe { libc::inotify_add_watch(st.inotify, c.as_ptr(), mask) };
        if wd >= 0 {
            st.watches.insert(wd, repo.root.clone());
            wds.push(wd);
        }
    }
    wds
}

fn watch_loop(sh: S) {
    let fd = sh.lock().inotify;
    if fd < 0 {
        return;
    }
    let mut buf = [0u8; 8192];
    loop {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            thread::sleep(Duration::from_millis(100));
            continue;
        }
        let mut roots: Vec<PathBuf> = Vec::new();
        let mut off = 0usize;
        let st = sh.lock();
        while off + 16 <= n as usize {
            let wd = i32::from_ne_bytes(buf[off..off + 4].try_into().unwrap());
            let len = u32::from_ne_bytes(buf[off + 12..off + 16].try_into().unwrap()) as usize;
            let name = &buf[off + 16..(off + 16 + len).min(n as usize)];
            let name = String::from_utf8_lossy(name);
            let name = name.trim_end_matches('\0');
            off += 16 + len;
            if WATCHED.contains(&name) {
                if let Some(r) = st.watches.get(&wd) {
                    if !roots.contains(r) {
                        roots.push(r.clone());
                    }
                }
            }
        }
        drop(st);
        if roots.is_empty() {
            continue;
        }
        // Let a burst of git's writes finish first.
        thread::sleep(Duration::from_millis(50));
        let mut st = sh.lock();
        for r in roots {
            if let Some(rs) = st.repos.get_mut(&r) {
                rs.stale = true;
            }
            start_refresh(&sh, &mut st, &r);
        }
    }
}

fn status(sh: &S) -> Value {
    let st = sh.lock();
    let now = Instant::now();
    let shells: Vec<Value> = st
        .shells
        .values()
        .map(|s| {
            // What is running, from the file postcmd wrote as it started.
            let running = s.pending.as_ref().map(|p| p.0.clone()).or_else(|| {
                let line = fs::read_to_string(st.rt.join(format!("cmd.{}", s.pid))).ok()?;
                Some(history::parse_tcsh_line(&line).0)
            });
            json!({ "pid": s.pid, "tty": s.tty, "cwd": s.cwd, "running": running })
        })
        .collect();
    let repos: Vec<Value> = st
        .repos
        .iter()
        .map(|(r, s)| {
            json!({
                "root": r, "branch": s.status.as_ref().map(|g| &g.branch), "error": s.error,
                "cost_ms": s.cost.as_millis() as u64, "checked_s": s.checked.elapsed().as_secs(),
            })
        })
        .collect();
    let probes: Vec<Value> = st
        .slots
        .iter()
        .map(|s| {
            json!({
                "name": s.name, "every_s": s.every.as_secs(),
                "next_s": s.next.saturating_duration_since(now).as_secs(),
                "last_s": s.last.map(|l| l.elapsed().as_secs()),
            })
        })
        .collect();
    json!({
        "pid": std::process::id(),
        "uptime_s": st.started.elapsed().as_secs(),
        "host": st.host,
        "config": paths::config_file(),
        "config_error": st.cfg_error,
        "shells": shells,
        "repos": repos,
        "facts": st.facts,
        "notices": st.notices.list.iter().map(|n| json!({ "id": n.id, "text": n.text, "level": n.level })).collect::<Vec<_>>(),
        "jobs_running": st.jobs.values().filter(|j| j.end.is_none()).map(|j| json!({ "id": j.id, "cmd": j.cmd, "start": j.start })).collect::<Vec<_>>(),
        "history": st.history.len(),
        "probes": probes,
        "live": st.cfg.raw.live.enabled,
    })
}

/// Tell systemd something (READY=1, WATCHDOG=1), when it runs the daemon.
fn sd_notify(msg: &str) {
    use std::os::unix::net::{SocketAddr, UnixDatagram};
    let Some(path) = std::env::var_os("NOTIFY_SOCKET") else { return };
    let Ok(sock) = UnixDatagram::unbound() else { return };
    let path = path.to_string_lossy();
    if let Some(name) = path.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        if let Ok(addr) = SocketAddr::from_abstract_name(name.as_bytes()) {
            let _ = sock.send_to_addr(msg.as_bytes(), &addr);
        }
    } else {
        let _ = sock.send_to(msg.as_bytes(), &*path);
    }
}

/// What each part of twig costs, from what the daemon has seen (`twig info`).
fn cost(sh: &S) -> Value {
    let st = sh.lock();
    let mut used = Vec::new();
    for nodes in [Some(&st.cfg.format), st.cfg.narrow.as_ref(), Some(&st.cfg.right)].into_iter().flatten() {
        crate::template::variables(nodes, &mut used);
    }
    let probes: Vec<Value> = st
        .slots
        .iter()
        .map(|s| {
            // The variables a probe gives; the others only make notices.
            let vars: Vec<String> = match &s.kind {
                Kind::Ip => vec!["ip".into()],
                Kind::Fqdn => vec!["fqdn".into()],
                Kind::Battery => vec!["battery".into()],
                Kind::Load => vec!["load".into()],
                Kind::Custom(i) => st.cfg.probes.get(*i).map(|p| vec![p.0.name.clone()]).unwrap_or_default(),
                Kind::Disk | Kind::Units | Kind::Reboot => vec![],
            };
            let shown: Vec<&String> = vars.iter().filter(|v| used.contains(v)).collect();
            let value: Vec<String> = vars.iter().filter_map(|v| st.facts.get(v).cloned()).collect();
            json!({
                "name": s.name, "every_s": s.every.as_secs_f64(), "spent": s.spent.json(),
                "vars": vars, "shown": shown, "value": value.join(" "),
                "notices": matches!(s.kind, Kind::Disk | Kind::Units | Kind::Reboot | Kind::Battery | Kind::Custom(_)),
            })
        })
        .collect();
    let repos: Vec<Value> = st
        .repos
        .iter()
        .map(|(r, s)| json!({ "root": r, "spent": s.spent.json(), "checked_s": s.checked.elapsed().as_secs() }))
        .collect();
    // Each step's times over the prompts kept, sorted, for percentiles.
    let mut by_step: Vec<(&str, Vec<u64>)> = Vec::new();
    for p in &st.prompts {
        for (name, us) in p {
            match by_step.iter_mut().find(|(n, _)| n == name) {
                Some((_, v)) => v.push(*us),
                None => by_step.push((name, vec![*us])),
            }
        }
    }
    let pct = |v: &[u64], p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    let steps: Vec<Value> = by_step
        .iter_mut()
        .map(|(name, v)| {
            v.sort_unstable();
            json!({ "name": name, "count": v.len(), "p50": pct(v, 0.5), "p95": pct(v, 0.95), "max": v[v.len() - 1] })
        })
        .collect();
    let (cpu_ms, rss_kb) = own_use();
    json!({
        "uptime_s": st.started.elapsed().as_secs(),
        "cpu_ms": cpu_ms,
        "rss_kb": rss_kb,
        "prompts": st.prompts.len(),
        "steps": steps,
        "probes": probes,
        "repos": repos,
        "used": used,
        "git_wait_ms": st.cfg.git_wait.as_millis() as u64,
        "live": st.cfg.raw.live.enabled,
        "skipped": st.cfg.probes.iter().map(|p| p.0.name.clone()).filter(|n| !st.slots.iter().any(|s| s.name == format!("probe:{n}"))).collect::<Vec<_>>(),
    })
}

/// The daemon's use of the machine: CPU time so far (ms), its own and that
/// of the commands it ran (git, systemctl, probes); and its memory
/// (resident, KB).
fn own_use() -> (u64, u64) {
    let tick = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as u64;
    let cpu_ms = fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|s| {
            let f: Vec<u64> = s[s.rfind(')')? + 2..].split_whitespace().skip(11).take(4).filter_map(|x| x.parse().ok()).collect();
            Some(f.iter().sum::<u64>() * 1000 / tick)
        })
        .unwrap_or(0);
    let rss_kb = fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmRSS:"))?.split_whitespace().nth(1)?.parse::<u64>().ok())
        .unwrap_or(0);
    (cpu_ms, rss_kb)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A /proc of processes: (pid, name, parent).
    fn fake_proc(procs: &[(u32, &str, u32)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("twig-proc-{}-{}", std::process::id(), procs.len()));
        for (pid, name, ppid) in procs {
            fs::create_dir_all(root.join(pid.to_string())).unwrap();
            fs::write(root.join(format!("{pid}/stat")), format!("{pid} ({name}) S {ppid} {pid} {pid} 0 -1")).unwrap();
        }
        root
    }

    #[test]
    fn root_by_effective_user() {
        let p = fake_proc(&[(7, "tcsh", 1), (8, "tcsh", 1)]);
        fs::write(p.join("7/status"), "Name:\ttcsh\nUid:\t1000\t0\t0\t0\n").unwrap();
        fs::write(p.join("8/status"), "Name:\ttcsh\nUid:\t1000\t1000\t1000\t1000\n").unwrap();
        assert_eq!(euid(&p, 7), Some(0));
        assert_eq!(euid(&p, 8), Some(1000));
        assert_eq!(euid(&p, 9), None);
        // This test's own shell, as the daemon would see it.
        assert_eq!(euid(Path::new("/proc"), std::process::id()), Some(unsafe { libc::geteuid() }));
    }

    #[test]
    fn ssh_among_the_parents() {
        // sshd → sshd-session → tcsh → screen → tcsh
        let p = fake_proc(&[(10, "sshd", 1), (20, "sshd-session", 10), (30, "tcsh", 20), (40, "screen", 30), (50, "tcsh", 40)]);
        assert!(over_ssh(&p, 50));
        assert!(over_ssh(&p, 30));
        // A desktop terminal: no sshd.
        let p = fake_proc(&[(100, "systemd", 1), (110, "xterm", 100), (120, "tcsh", 110)]);
        assert!(!over_ssh(&p, 120));
    }
}
