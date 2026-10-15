//! Putting twig on a machine, taking it off, and seeing how it is doing.

use crate::boxes;
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

/// Finds twig through PATH, as a shell would, so that a copy in
/// ~/.cargo/bin or one from mise both work, whichever exists.
const UNIT: &str = include_str!("../assets/twig.service");

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

    // A daemon started by a shell gives way to the service.
    let _ = proto::send(&Request::Stop, Duration::from_secs(2));
    std::thread::sleep(Duration::from_millis(100));
    if has_user_systemd() {
        let unit = UNIT;
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

/// `twig status`: everything the daemon knows, in a box, a row per topic.
pub fn status() -> i32 {
    let v = match proto::send(&Request::Status, Duration::from_secs(3)) {
        Ok(v) => v,
        Err(_) => {
            println!("twig is not running (the next prompt starts it, or: twig restart)");
            return 1;
        }
    };
    let color = unsafe { libc::isatty(1) } == 1;
    let paint = |s: &str, st: &str| if color { Style::of(st).paint(s) } else { s.to_string() };
    let dim = |s: &str| paint(s, "dim");
    let home = paths::home().to_string_lossy().into_owned();
    let s = |v: &serde_json::Value| v.as_str().unwrap_or("").to_string();
    let n = |v: &serde_json::Value| v.as_u64().unwrap_or(0);
    let list = |v: &serde_json::Value| v.as_array().cloned().unwrap_or_default();
    // Inside the box: the terminal less the frame.
    let width = (crate::util::tty_columns("/dev/tty").unwrap_or(100) as usize).saturating_sub(4).max(40);
    let mut body: Vec<String> = Vec::new();
    let mut add = |label: &str, items: Vec<String>| body.extend(boxes::row(label, &items, width));

    // The file as it reads now; and what the daemon made of it, if that
    // differs (a twig older than the settings).
    let (_, lines) = settings(&paths::config_file());
    let mut lines = lines.into_iter();
    add("settings", lines.next().into_iter().collect());
    for l in lines.take(3) {
        add("", vec![l]);
    }
    if let Some(e) = v["config_error"].as_str() {
        add("", vec![paint(&format!("✘ the daemon couldn't read them: {}", config::summary(e, None)), "red")]);
    }
    add("live", vec![(if v["live"].as_bool() == Some(true) { "on" } else { "off" }).into()]);
    add("history", vec![format!("{} commands", thousands(n(&v["history"])))]);

    let shells = list(&v["shells"]);
    let mut items = vec![format!("{}", shells.len())];
    for sh in &shells {
        let tty = s(&sh["tty"]).trim_start_matches("/dev/").to_string();
        let tty = if tty.is_empty() { "-".to_string() } else { tty };
        let doing = sh["running"].as_str().map(|c| paint(&format!(" running {c}"), "cyan")).unwrap_or_default();
        items.push(format!("{} {}{doing}", dim(&tty), tilde(&s(&sh["cwd"]), &home)));
    }
    add("shells", items);

    let repos = list(&v["repos"]);
    let items: Vec<String> = if repos.is_empty() {
        vec![dim("none yet")]
    } else {
        repos
            .iter()
            .map(|r| {
                let root = tilde(&s(&r["root"]), &home);
                match r["error"].as_str() {
                    Some(e) => format!("{root} {}", paint(&format!("✘ {e}"), "red")),
                    None => format!(
                        "{root} {} {}",
                        paint(&s(&r["branch"]), "magenta"),
                        dim(&format!("{} ms, {} ago", n(&r["cost_ms"]), short(n(&r["checked_s"]))))
                    ),
                }
            })
            .collect()
    };
    add("git", items);

    let notices = list(&v["notices"]);
    let items: Vec<String> = if notices.is_empty() {
        vec![dim("none")]
    } else {
        notices
            .iter()
            .map(|x| {
                let st = match x["level"].as_str() {
                    Some("error") => "red",
                    Some("warn") => "yellow",
                    _ => "cyan",
                };
                format!("{} {}", dim(&n(&x["id"]).to_string()), paint(&s(&x["text"]), st))
            })
            .collect()
    };
    add("notices", items);

    let jobs = list(&v["jobs_running"]);
    let now = now_ms() / 1000;
    let items: Vec<String> = if jobs.is_empty() {
        vec![dim("none running")]
    } else {
        jobs.iter().map(|j| format!("{} {} {}", dim(&format!("[{}]", n(&j["id"]))), s(&j["cmd"]), dim(&short(now.saturating_sub(n(&j["start"]) / 1000))))).collect()
    };
    add("jobs", items);

    let mut facts: Vec<(String, String)> =
        v["facts"].as_object().map(|m| m.iter().map(|(k, x)| (k.clone(), s(x))).collect()).unwrap_or_default();
    facts.sort();
    add("facts", facts.iter().map(|(k, x)| format!("{} {x}", dim(k))).collect());

    // A probe is late when it hasn't run for well over its interval.
    let probes = list(&v["probes"]);
    let late: Vec<String> =
        probes.iter().filter(|p| p["last_s"].as_u64().is_none_or(|l| l > n(&p["every_s"]) * 2 + 5)).map(|p| s(&p["name"])).collect();
    let mut items = vec![if late.is_empty() {
        format!("{} {}", probes.len(), dim("on time"))
    } else {
        paint(&format!("{} late: {}", late.len(), late.join(", ")), "yellow")
    }];
    // What they cost together, from what the daemon has seen.
    let cost = proto::send(&Request::Cost, Duration::from_secs(3)).ok();
    if let Some(c) = &cost {
        let per_hour: f64 = c["probes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| n(&p["spent"]["avg_us"]) as f64 * 3600.0 / p["every_s"].as_f64().unwrap_or(1.0).max(0.001) / 1000.0)
            .sum();
        items.push(dim(&format!("{per_hour:.0} ms an hour")));
    }
    items.extend(probes.iter().map(|p| format!("{} {}", s(&p["name"]), dim(&short(n(&p["every_s"]))))));
    add("probes", items);

    // How fast prompts have been, in the daemon.
    if let Some(total) = cost.as_ref().and_then(|c| c["steps"].as_array().cloned()).and_then(|st| st.into_iter().find(|x| x["name"] == "total")) {
        let ms = |u: u64| format!("{:.2} ms", u as f64 / 1000.0);
        add(
            "prompts",
            vec![
                format!("{} {}", ms(n(&total["p50"])), dim("typical")),
                format!("{} {}", ms(n(&total["p95"])), dim("slow (p95)")),
                format!("{} {}", ms(n(&total["max"])), dim("worst")),
                dim(&format!("of the last {}", n(&total["count"]))),
            ],
        );
    }

    let by = match crate::client::daemon_pid() {
        Some((_, true)) => "systemd",
        Some((_, false)) => "started by a shell",
        None => "?",
    };
    let title = format!("twig  pid {} · {by} · up {}", v["pid"], short(n(&v["uptime_s"])));
    for l in boxes::draw(&title, &body, Some("twig info for all of it in detail")) {
        println!("{l}");
    }
    0
}

/// Seconds as the largest unit or two: 45s, 10m, 1h05m, 3d.
fn short(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 if secs.is_multiple_of(60) => format!("{}m", secs / 60),
        60..=3599 => format!("{}m{:02}s", secs / 60, secs % 60),
        3600..=86_399 if secs.is_multiple_of(3600) => format!("{}h", secs / 3600),
        3600..=86_399 => format!("{}h{:02}m", secs / 3600, secs % 3600 / 60),
        _ => format!("{}d", secs / 86_400),
    }
}

/// 4915 as 4,915.
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

const KNOWN: &[&str] = &[
    "time", "host", "fqdn", "user", "ip", "cwd", "dir", "symbol", "status", "duration", "jobs", "notice", "battery", "load",
    "git", "git_branch", "git_ahead", "git_behind", "git_staged", "git_changed", "git_untracked", "git_stash",
    "git_conflicts", "git_clean", "git_dirty", "git_state", "git_remote", "git_age", "pwd",
    "battery_pct", "battery_watts", "battery_time", "ssh", "root",
];

/// The settings file: whether it reads, and anything in it that can't work
/// (a variable no template has). The settings when they read, and what to
/// say, a line each, first the file and how it is.
fn settings(path: &std::path::Path) -> (Option<Loaded>, Vec<String>) {
    let home = paths::home().to_string_lossy().into_owned();
    let shown = tilde(&path.to_string_lossy(), &home);
    let red = |s: &str| Style::of("red").paint(s);
    let dim = |s: &str| Style::of("dim").paint(s);
    let cfg = match config::read(path) {
        Ok(c) => c,
        Err(e) => {
            let text = fs::read_to_string(path).ok();
            let mut lines = vec![format!("{shown} {}", red(&format!("✘ {}", config::summary(&e, text.as_deref()))))];
            if text.as_deref().is_some_and(config::looks_old) {
                lines.push(dim("move it aside and run twig setup for new settings, or link your own"));
            } else {
                lines.extend(e.lines().filter(|l| !l.trim().is_empty()).map(|l| dim(&format!("  {l}"))));
            }
            return (None, lines);
        }
    };
    let mut problems = Vec::new();
    let probes: Vec<&str> = cfg.probes.iter().map(|p| p.0.name.as_str()).collect();
    for (name, nodes) in [("format", Some(&cfg.format)), ("narrow", cfg.narrow.as_ref()), ("right", Some(&cfg.right))] {
        let mut vars = Vec::new();
        template::variables(nodes.map(|n| n.as_slice()).unwrap_or(&[]), &mut vars);
        for v in vars {
            if !v.starts_with('$') && !KNOWN.contains(&v.as_str()) && !probes.contains(&v.as_str()) {
                problems.push(red(&format!("✘ prompt.{name}: no variable {{{v}}}, and no [[probe]] named {v}")));
            }
        }
    }
    let first = if problems.is_empty() { format!("{shown} {}", Style::of("green").paint("✔")) } else { shown };
    (Some(cfg), std::iter::once(first).chain(problems).collect())
}

/// `twig info [FILE]` (also check, cost, debug): the settings and what is
/// wrong with them, what each part of twig costs, and the prompt the
/// settings make, with made-up values.
pub fn info(args: &[String]) -> i32 {
    let path = args.first().map(PathBuf::from).unwrap_or_else(paths::config_file);
    let (cfg, lines) = settings(&path);
    let ok = cfg.is_some() && lines.len() == 1;
    for l in boxes::draw("settings", &lines, None) {
        println!("{l}");
    }
    match proto::send(&Request::Cost, Duration::from_secs(3)) {
        Ok(v) => cost_boxes(&v),
        Err(_) => println!("{}", Style::of("dim").paint("\nthe daemon is not running (twig restart): no costs to show")),
    }
    if let Some(cfg) = cfg {
        preview(&cfg);
    }
    if ok { 0 } else { 1 }
}

/// The prompt the settings make, with made-up values, in boxes: at the
/// terminal's width, and narrow after a failed command.
fn preview(cfg: &Loaded) {
    let probes: Vec<&str> = cfg.probes.iter().map(|p| p.0.name.as_str()).collect();
    let env: HashMap<String, String> = env::vars().collect();
    let host = paths::hostname();
    // Made-up values, marked as such where they can't be real.
    let mut facts = HashMap::new();
    facts.insert("ip".to_string(), "192.0.2.10".to_string());
    facts.insert("battery".to_string(), "85%".to_string());
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
        cfg,
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
        ssh: false,
        root: false,
    };
    let term = crate::util::tty_columns("/dev/tty").unwrap_or(100) as usize;
    // Each sample in a box as wide as it says, so it can't be mistaken for
    // the real prompt around it.
    let boxed = |title: &str, c: &Ctx, cols: usize| {
        let (p, r) = render::render(c, Some(cols as u16));
        let mut lines: Vec<String> = p.split('\n').map(String::from).collect();
        if !r.is_empty() {
            let last = lines.len() - 1;
            lines[last] = format!("{}{}{r}", lines[last], " ".repeat(cols.saturating_sub(visible_width(&lines[last]) + visible_width(&r) + 1)));
        }
        println!();
        for l in boxes::draw(title, &lines, None) {
            println!("{l}");
        }
    };
    let wide = term.saturating_sub(4).max(40);
    boxed(&format!("sample, made-up values, {wide} columns"), &ctx(0, Some(("disk / 93% full, 12G left".into(), Level::Warn, 1))), wide);
    boxed("after a failed command, 60 columns", &ctx(2, None), 60);
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

/// What each part of twig costs, from the daemon's answer to Cost, in two
/// boxes, side by side where they fit: the prompt (what a shell waits for)
/// and the daemon (what it does in the background).
fn cost_boxes(v: &serde_json::Value) {
    println!();
    let color = unsafe { libc::isatty(1) } == 1;
    let paint = |s: &str, st: &str| if color { Style::of(st).paint(s) } else { s.to_string() };
    let dim = |s: &str| paint(s, "dim");
    let ms = |us: u64| format!("{:.2} ms", us as f64 / 1000.0);
    let n = |x: &serde_json::Value| x.as_u64().unwrap_or(0);
    let home = paths::home().to_string_lossy().into_owned();

    // The prompt: each step, typical, slow and worst.
    let mut body = vec![dim(&format!("{:<12}{:>10} {:>10} {:>10}", "", "p50", "p95", "max"))];
    for s in v["steps"].as_array().into_iter().flatten() {
        let name = s["name"].as_str().unwrap_or("");
        let cells = format!("{:>10} {:>10} {:>10}", ms(n(&s["p50"])), ms(n(&s["p95"])), ms(n(&s["max"])));
        body.push(if name == "total" { paint(&format!("{name:<12}{cells}"), "bold") } else { format!("{}{cells}", dim(&format!("{name:<12}"))) });
    }
    body.push(String::new());
    body.push(dim(&format!("git is waited for up to {} ms,", n(&v["git_wait_ms"]))));
    body.push(dim("and only after a command"));
    let prompt = boxes::draw(&format!("prompt, you wait · last {}", n(&v["prompts"])), &body, None);

    // The daemon: each probe (how often, what a run takes, what that is an
    // hour, and whether the prompt shows what it finds), git status per
    // repository, and its own memory and CPU.
    let used: Vec<&str> = v["used"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).collect();
    let mut body = vec![dim(&format!("{:<12}{:>6} {:>6} {:>10} {:>10} {:>9}   {}", "probes", "every", "runs", "avg", "max", "per hour", "on the prompt"))];
    for p in v["probes"].as_array().into_iter().flatten() {
        let sp = &p["spent"];
        let every = p["every_s"].as_f64().unwrap_or(1.0).max(0.001);
        let per_hour = n(&sp["avg_us"]) as f64 * 3600.0 / every / 1000.0;
        let vars: Vec<&str> = p["vars"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).collect();
        let shown: Vec<String> = vars.iter().filter(|v| used.contains(v)).map(|v| format!("{{{v}}}")).collect();
        let value = p["value"].as_str().unwrap_or("");
        let where_ = if !shown.is_empty() {
            format!("{} {}", shown.join(" "), dim(if value.is_empty() { "(empty here)" } else { value }))
        } else if vars.contains(&"fqdn") {
            dim("for the colors in [hosts]")
        } else if !vars.is_empty() {
            dim(&format!("{{{}}} not shown{}", vars.join("} {"), if p["notices"].as_bool() == Some(true) { "; notices" } else { "" }))
        } else {
            dim("notices only")
        };
        let name = p["name"].as_str().unwrap_or("").trim_start_matches("probe:");
        let hot = if per_hour >= 100.0 { "yellow" } else { "none" };
        body.push(format!(
            "{name:<12}{:>6} {:>6} {:>10} {:>10} {}   {where_}",
            short(every as u64),
            n(&sp["runs"]),
            ms(n(&sp["avg_us"])),
            ms(n(&sp["max_us"])),
            paint(&format!("{:>9}", format!("{per_hour:.0} ms")), hot),
        ));
    }
    for s in v["skipped"].as_array().into_iter().flatten() {
        body.push(format!("{:<12}{}", s.as_str().unwrap_or(""), dim("not on this machine (hosts)")));
    }

    body.push(String::new());
    body.push(dim(&format!("{:<32}{:>6} {:>10} {:>10} {:>8}", "git status", "runs", "avg", "max", "last")));
    let repos = v["repos"].as_array().cloned().unwrap_or_default();
    for r in &repos {
        let sp = &r["spent"];
        body.push(format!(
            "{:<32}{:>6} {:>10} {:>10} {:>8}",
            tilde(r["root"].as_str().unwrap_or(""), &home),
            n(&sp["runs"]),
            ms(n(&sp["avg_us"])),
            ms(n(&sp["max_us"])),
            format!("{} ago", short(n(&r["checked_s"]))),
        ));
    }
    if repos.is_empty() {
        body.push(dim("no repository yet"));
    }

    let up = n(&v["uptime_s"]).max(1);
    let cpu = n(&v["cpu_ms"]);
    body.push(String::new());
    body.push(format!(
        "{} {:.1} MB · {} of CPU in {} ({:.3}% of one core)",
        dim("itself"),
        n(&v["rss_kb"]) as f64 / 1024.0,
        fmt_duration(cpu),
        short(up),
        cpu as f64 / 10.0 / up as f64
    ));
    let daemon = boxes::draw("daemon, in the background", &body, Some("a prompt only reads what it found last"));

    let term = crate::util::tty_columns("/dev/tty").unwrap_or(100) as usize;
    for l in boxes::beside(&prompt, &daemon, term) {
        println!("{l}");
    }
}
