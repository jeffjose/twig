//! Putting twig on a machine, taking it off, and seeing how it is doing.

use crate::config::{self, Loaded};
use crate::git::GitStatus;
use crate::history;
use crate::notices::Level;
use crate::paths;
use crate::proto::{self, Request};
use crate::render::{self, Ctx};
use crate::template::{self, Style};
use crate::util::{fmt_duration, now_ms, tilde, visible_width};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const CSHRC_LINE: &str = "if ( -X twig ) eval \"`twig init tcsh`\"";

fn unit_file() -> PathBuf {
    let base = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| paths::home().join(".config"));
    base.join("systemd/user/twig.service")
}

fn systemctl(args: &[&str]) -> bool {
    Command::new("systemctl").arg("--user").args(args).status().is_ok_and(|s| s.success())
}

fn has_user_systemd() -> bool {
    Command::new("systemctl").args(["--user", "is-system-running"]).output().is_ok_and(|o| !o.stdout.is_empty())
}

/// `twig setup`
pub fn setup() -> i32 {
    let cfg = paths::config_file();
    let text = fs::read_to_string(&cfg).ok();
    // The old twig's settings, at the same path: set them aside. A link
    // (to dotfiles) is left alone; that file is fixed where it lives.
    let is_link = fs::symlink_metadata(&cfg).is_ok_and(|m| m.file_type().is_symlink());
    if !is_link && text.as_deref().is_some_and(config::looks_old) {
        let aside = cfg.with_extension("toml.old-twig");
        match fs::rename(&cfg, &aside) {
            Ok(_) => println!("settings:  the old twig's {} moved to {}", cfg.display(), aside.display()),
            Err(e) => eprintln!("twig: {}: {e}", cfg.display()),
        }
    }
    if cfg.exists() {
        match config::read(&cfg) {
            Ok(_) => println!("settings:  {} (kept)", cfg.display()),
            Err(e) => println!("settings:  {} (kept, but not read: {})", cfg.display(), config::summary(&e, text.as_deref())),
        }
    } else {
        if let Some(d) = cfg.parent() {
            let _ = fs::create_dir_all(d);
        }
        match fs::write(&cfg, config::STARTER) {
            Ok(_) => println!("settings:  {} (new)", cfg.display()),
            Err(e) => eprintln!("twig: {}: {e}", cfg.display()),
        }
    }

    let host = paths::hostname();
    let old = paths::home().join(".history");
    if !paths::history_file(&host).exists() && old.exists() {
        print!("history:   ");
        history::import(&old);
    } else {
        println!("history:   {}", paths::history_file(&host).display());
    }

    let Ok(exe) = env::current_exe() else { return 1 };
    // A daemon started by a shell gives way to the service.
    let _ = proto::send(&Request::Stop, Duration::from_secs(2));
    std::thread::sleep(Duration::from_millis(100));
    if has_user_systemd() {
        let path = env::var("PATH").unwrap_or_default();
        // The daemon checks in every second; one that misses 10 seconds of
        // it (stuck, or stopped) is restarted.
        let unit = format!(
            "[Unit]\nDescription=twig: the daemon behind the tcsh prompt\n\n[Service]\nType=notify\nNotifyAccess=main\n\
             ExecStart={} daemon\nEnvironment=PATH={path}\nWatchdogSec=10\nRestart=on-failure\nRestartSec=2\n\n\
             [Install]\nWantedBy=default.target\n",
            exe.display()
        );
        let f = unit_file();
        if let Some(d) = f.parent() {
            let _ = fs::create_dir_all(d);
        }
        if let Err(e) = fs::write(&f, unit) {
            eprintln!("twig: {}: {e}", f.display());
            return 1;
        }
        let ok = systemctl(&["daemon-reload"]) && systemctl(&["enable", "--now", "twig.service"]) && systemctl(&["restart", "twig.service"]);
        println!("service:   {} ({})", f.display(), if ok { "running" } else { "could not start; see journalctl --user -u twig" });
    } else {
        crate::client::start_daemon();
        println!("service:   no systemd user session here; the first prompt starts the daemon");
    }

    let cshrc = paths::home().join(".cshrc");
    let has_line = fs::read_to_string(&cshrc).is_ok_and(|t| t.contains("twig init tcsh"));
    if has_line {
        println!("shell:     ~/.cshrc already starts twig; open a new shell");
    } else {
        println!("\nTo use it, put this at the end of ~/.cshrc and open a new shell:\n\n    {CSHRC_LINE}\n");
    }
    0
}

/// `twig remove`
pub fn remove() -> i32 {
    if unit_file().exists() {
        systemctl(&["disable", "--now", "twig.service"]);
        let _ = fs::remove_file(unit_file());
        systemctl(&["daemon-reload"]);
        println!("removed the service");
    }
    let _ = proto::send(&Request::Stop, Duration::from_secs(2));
    println!(
        "stopped the daemon.\n\nLeft in place: the settings ({}), the history and directories ({}),\n\
         and the line in ~/.cshrc, which does nothing once twig is not installed.",
        paths::config_file().display(),
        paths::data_dir().display()
    );
    0
}

/// `twig status`
pub fn status() -> i32 {
    let v = match proto::send(&Request::Status, Duration::from_secs(3)) {
        Ok(v) => v,
        Err(_) => {
            println!("twig is not running (the next prompt starts it, or: twig setup)");
            return 1;
        }
    };
    let home = paths::home().to_string_lossy().into_owned();
    let s = |v: &serde_json::Value| v.as_str().unwrap_or("").to_string();
    println!("daemon     pid {}, up {}", v["pid"], fmt_duration(v["uptime_s"].as_u64().unwrap_or(0) * 1000));
    println!("settings   {}{}", tilde(&s(&v["config"]), &home), v["config_error"].as_str().map(|e| format!("\n           ✘ {e}")).unwrap_or_default());
    println!("live       {}", if v["live"].as_bool() == Some(true) { "on" } else { "off" });
    println!("history    {} commands", v["history"]);
    println!("notices    {}   jobs running {}", v["notices"], v["jobs_running"]);
    println!("\nshells");
    for sh in v["shells"].as_array().into_iter().flatten() {
        let doing = sh["running"].as_str().map(|c| format!("  running: {c}")).unwrap_or_default();
        println!("  {:>8}  {:<12} {}{doing}", sh["pid"], s(&sh["tty"]), tilde(&s(&sh["cwd"]), &home));
    }
    println!("\nrepositories");
    for r in v["repos"].as_array().into_iter().flatten() {
        let how = match r["error"].as_str() {
            Some(e) => format!("✘ {e}"),
            None => format!("{}  git status {}ms, {}s ago", s(&r["branch"]), r["cost_ms"], r["checked_s"]),
        };
        println!("  {}  {how}", tilde(&s(&r["root"]), &home));
    }
    println!("\nfacts");
    if let Some(m) = v["facts"].as_object() {
        let mut keys: Vec<&String> = m.keys().collect();
        keys.sort();
        for k in keys {
            println!("  {k:<10} {}", s(&m[k]));
        }
    }
    println!("\nprobes");
    for p in v["probes"].as_array().into_iter().flatten() {
        let last = p["last_s"].as_u64().map(|l| format!("ran {l}s ago")).unwrap_or_else(|| "not run yet".into());
        println!("  {:<18} every {:>6}  {last}", s(&p["name"]), fmt_duration(p["every_s"].as_u64().unwrap_or(0) * 1000));
    }
    0
}

const KNOWN: &[&str] = &[
    "time", "host", "fqdn", "user", "ip", "cwd", "dir", "symbol", "status", "duration", "jobs", "notice", "battery", "load",
    "git", "git_branch", "git_ahead", "git_behind", "git_staged", "git_changed", "git_untracked", "git_stash",
    "git_conflicts", "git_clean", "git_dirty", "git_state", "git_remote", "git_age", "pwd",
];

/// `twig check [FILE]`: read the settings, say what is wrong, and show the
/// prompt they make with made-up values.
pub fn check(args: &[String]) -> i32 {
    let path = args.first().map(PathBuf::from).unwrap_or_else(paths::config_file);
    let cfg: Loaded = match config::read(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("✘ {}\n  {}", path.display(), e.replace('\n', "\n  "));
            if fs::read_to_string(&path).is_ok_and(|t| config::looks_old(&t)) {
                eprintln!(
                    "\nThis is the settings file of the old twig, before the rewrite. Move it aside\n\
                     and run `twig setup` for a new one (or link your own, as ~/dotfiles/setup does)."
                );
            }
            return 1;
        }
    };
    let mut problems = 0;
    let probes: Vec<&str> = cfg.probes.iter().map(|p| p.0.name.as_str()).collect();
    for (name, nodes) in [("format", Some(&cfg.format)), ("narrow", cfg.narrow.as_ref()), ("right", Some(&cfg.right))] {
        let mut vars = Vec::new();
        template::variables(nodes.map(|n| n.as_slice()).unwrap_or(&[]), &mut vars);
        for v in vars {
            if !v.starts_with('$') && !KNOWN.contains(&v.as_str()) && !probes.contains(&v.as_str()) {
                println!("✘ prompt.{name}: no variable {{{v}}} (and no [[probe]] named {v})");
                problems += 1;
            }
        }
    }
    if problems == 0 {
        println!("✔ {}", path.display());
    }

    let env: HashMap<String, String> = env::vars().collect();
    let host = paths::hostname();
    // Made-up values, marked as such where they can't be real.
    let mut facts = HashMap::new();
    facts.insert("ip".to_string(), "192.0.2.10".to_string());
    for p in &probes {
        facts.insert(p.to_string(), format!("‹{p}›"));
    }
    let now = now_ms() / 1000;
    let git = GitStatus {
        branch: "main".into(),
        upstream: true,
        ahead: 1,
        changed: 2,
        untracked: 1,
        commit_time: Some(now - 3 * 3600),
        ..Default::default()
    };
    let cwd = env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let ctx = |status, notice: Option<(String, Level, usize)>| Ctx {
        cfg: &cfg,
        host: &host,
        fqdn: &host,
        cwd: &cwd,
        env: &env,
        status,
        duration_ms: Some(12_400),
        time_secs: now as i64,
        facts: &facts,
        branch: None,
        git: Some(&git),
        notice,
        jobs: "⏵ make test 1m12s".into(),
    };
    let term = crate::util::tty_columns("/dev/tty").unwrap_or(100) as usize;
    let dim = Style::of("dim");
    // Each sample in a box as wide as it says, so it can't be mistaken for
    // the real prompt around it.
    let boxed = |title: &str, c: &Ctx, cols: usize| {
        let (p, r) = render::render(c, Some(cols as u16));
        let mut lines: Vec<String> = p.split('\n').map(String::from).collect();
        if !r.is_empty() {
            let last = lines.len() - 1;
            lines[last] = format!("{}{}{r}", lines[last], " ".repeat(cols.saturating_sub(visible_width(&lines[last]) + visible_width(&r) + 1)));
        }
        let inner = lines.iter().map(|l| visible_width(l)).max().unwrap_or(0).max(visible_width(title) + 3);
        let head = format!("─ {title} ");
        println!("\n{}", dim.paint(&format!("╭{head}{}╮", "─".repeat(inner + 2 - visible_width(&head).min(inner + 2)))));
        for l in &lines {
            println!("{} {l}\x1b[0m{} {}", dim.paint("│"), " ".repeat(inner - visible_width(l)), dim.paint("│"));
        }
        println!("{}", dim.paint(&format!("╰{}╯", "─".repeat(inner + 2))));
    };
    let wide = term.saturating_sub(4).max(40);
    boxed(&format!("sample, made-up values, {wide} columns"), &ctx(0, Some(("disk / 93% full, 12G left".into(), Level::Warn, 1))), wide);
    boxed("after a failed command, 60 columns", &ctx(2, None), 60);
    if problems > 0 { 1 } else { 0 }
}

/// `twig restart`: the service when systemd runs it here, otherwise the
/// daemon a shell started. Either way the daemon is then the twig on disk.
pub fn restart() -> i32 {
    let by_systemd = crate::client::daemon_pid().is_some_and(|(_, s)| s) || (unit_file().exists() && has_user_systemd());
    if by_systemd {
        // A daemon a shell started meanwhile would keep the service's from starting.
        if crate::client::daemon_pid().is_some_and(|(_, s)| !s) {
            let _ = proto::send(&Request::Stop, Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    if by_systemd && systemctl(&["restart", "twig.service"]) {
        println!("restarted the service");
        return 0;
    }
    // Ask it to stop; if it doesn't answer, stop it.
    if proto::send(&Request::Stop, Duration::from_secs(2)).is_err() {
        if let Some((pid, _)) = crate::client::daemon_pid() {
            unsafe { libc::kill(pid as i32, libc::SIGKILL) };
        }
    }
    std::thread::sleep(Duration::from_millis(200));
    crate::client::start_daemon();
    for _ in 0..50 {
        std::thread::sleep(Duration::from_millis(40));
        if let Ok(v) = proto::send(&Request::Status, Duration::from_secs(1)) {
            println!("restarted the daemon (pid {})", v["pid"]);
            return 0;
        }
    }
    eprintln!("twig: the daemon didn't start; see {}", paths::state_dir().join("daemon.log").display());
    1
}
