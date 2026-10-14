//! Probes: what the daemon looks at now and then. Each gives values for the
//! prompt (facts) and notices that come and go with what they report.

use crate::config::{Loaded, Probe};
use crate::notices::{Level, New};
use crate::util::{fmt_duration, read_trimmed, run_with_limit};
use std::ffi::CString;
use std::fs;
use std::net::UdpSocket;
use std::process::Command;
use std::time::Duration;

#[derive(Default, Debug)]
pub struct Outcome {
    /// name -> value; None removes it.
    pub facts: Vec<(String, Option<String>)>,
    /// key -> notice; None means its condition is over.
    pub notices: Vec<(String, Option<New>)>,
}

impl Outcome {
    fn fact(&mut self, name: &str, value: Option<String>) {
        self.facts.push((name.into(), value));
    }

    fn notice(&mut self, key: &str, n: Option<(Level, String)>) {
        let n = n.map(|(level, text)| New { key: Some(key.into()), text, level, ttl_ms: None, shows: None, pid: None });
        self.notices.push((key.into(), n));
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Ip,
    Fqdn,
    Disk,
    Battery,
    Load,
    Units,
    Reboot,
    Custom(usize),
}

/// The probes that apply on this machine with these settings, and how often
/// each runs.
pub fn schedule(cfg: &Loaded, fqdn: &str, host: &str) -> Vec<(String, Kind, Duration)> {
    let mut v = vec![
        ("ip".to_string(), Kind::Ip, Duration::from_secs(10)),
        ("fqdn".to_string(), Kind::Fqdn, Duration::from_secs(600)),
        ("disk".to_string(), Kind::Disk, cfg.disk_every),
        ("battery".to_string(), Kind::Battery, Duration::from_secs(15)),
        ("load".to_string(), Kind::Load, Duration::from_secs(10)),
        ("reboot".to_string(), Kind::Reboot, Duration::from_secs(300)),
    ];
    if let Some(every) = cfg.units_every {
        v.push(("units".into(), Kind::Units, every));
    }
    for (i, (p, every, _)) in cfg.probes.iter().enumerate() {
        if cfg.probe_applies(p, fqdn, host) {
            v.push((format!("probe:{}", p.name), Kind::Custom(i), *every));
        }
    }
    v
}

pub fn run(kind: &Kind, cfg: &Loaded) -> Outcome {
    let mut o = Outcome::default();
    match kind {
        Kind::Ip => o.fact("ip", ip()),
        Kind::Fqdn => o.fact("fqdn", fqdn()),
        Kind::Disk => disk(cfg, &mut o),
        Kind::Battery => battery(cfg, &mut o),
        Kind::Load => o.fact("load", read_trimmed("/proc/loadavg").and_then(|l| l.split_whitespace().next().map(String::from))),
        Kind::Units => units(&mut o),
        Kind::Reboot => {
            let needed = fs::metadata("/var/run/reboot-required").is_ok();
            o.notice("reboot", needed.then(|| (Level::Info, "restart needed for updates".to_string())));
        }
        Kind::Custom(i) => {
            if let Some((p, _, timeout)) = cfg.probes.get(*i) {
                custom(p, *timeout, &mut o);
            }
        }
    }
    o
}

/// The address this machine uses to reach the internet. Connecting a UDP
/// socket sends nothing; it only picks the route.
fn ip() -> Option<String> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:53").ok()?;
    Some(s.local_addr().ok()?.ip().to_string())
}

fn fqdn() -> Option<String> {
    let ran = run_with_limit(Command::new("hostname").arg("-f"), Duration::from_secs(5)).ok()?;
    let name = ran.stdout.trim().to_string();
    (ran.status == Some(0) && !name.is_empty()).then_some(name)
}

fn disk(cfg: &Loaded, o: &mut Outcome) {
    let home = crate::paths::home();
    let mut seen = Vec::new();
    for p in &cfg.raw.disk.paths {
        let path = if p == "~" { home.clone() } else if let Some(r) = p.strip_prefix("~/") { home.join(r) } else { p.into() };
        let key = format!("disk:{p}");
        // Paths on a filesystem already looked at (~ on the same disk as /).
        let dev = std::os::unix::fs::MetadataExt::dev(&match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        });
        if seen.contains(&dev) {
            o.notice(&key, None);
            continue;
        }
        seen.push(dev);
        let Ok(c) = CString::new(path.to_string_lossy().as_bytes()) else { continue };
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 || st.f_blocks == 0 {
            continue;
        }
        let total = st.f_blocks as f64 * st.f_frsize as f64;
        let avail = st.f_bavail as f64 * st.f_frsize as f64;
        let used_pct = (100.0 * (1.0 - avail / total)).round() as u8;
        let n = (used_pct >= cfg.raw.disk.warn).then(|| {
            let level = if used_pct >= 98 { Level::Error } else { Level::Warn };
            (level, format!("disk {p} {used_pct}% full, {} left", size(avail)))
        });
        o.notice(&key, n);
    }
}

fn size(bytes: f64) -> String {
    let units = ["B", "K", "M", "G", "T"];
    let mut v = bytes;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if v < 10.0 && i > 0 { format!("{v:.1}{}", units[i]) } else { format!("{v:.0}{}", units[i]) }
}

/// A laptop's battery, as /sys/class/power_supply has it.
#[derive(Debug, PartialEq)]
pub struct Battery {
    pub percent: u8,
    pub state: Power,
    /// Watts going in (charging) or out (on battery); None when the
    /// battery doesn't say.
    pub watts: Option<f64>,
    /// Seconds until empty (on battery) or full (charging).
    pub secs_left: Option<u64>,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Power {
    Charging,
    Discharging,
    /// On power and not charging: full, or held below full.
    Full,
}

/// The machine's battery: BAT0 and the like. Batteries of devices (a
/// mouse, a keyboard) are left out.
fn find_battery(root: &std::path::Path) -> Option<std::path::PathBuf> {
    fs::read_dir(root).ok()?.flatten().map(|e| e.path()).find(|p| {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let kind = read_trimmed(p.join("type")).unwrap_or_default();
        let device = read_trimmed(p.join("scope")).is_some_and(|s| s == "Device");
        name.starts_with("BAT") || (kind == "Battery" && !device)
    })
}

/// Read a battery's directory. Power is power_now, or current × voltage;
/// time left comes from the energy (or charge) left and that rate.
pub fn read_battery(dir: &std::path::Path) -> Option<Battery> {
    let num = |f: &str| read_trimmed(dir.join(f)).and_then(|v| v.parse::<f64>().ok());
    let percent = num("capacity")? as u8;
    let state = match read_trimmed(dir.join("status")).unwrap_or_default().as_str() {
        "Charging" => Power::Charging,
        "Discharging" => Power::Discharging,
        _ => Power::Full,
    };
    // In µW, µA, µV, µWh, µAh.
    let watts = num("power_now").or_else(|| Some(num("current_now")? * num("voltage_now")? / 1e6)).map(|uw| uw.abs() / 1e6);
    let watts = watts.filter(|w| *w > 0.05);
    let (now, full, rate) = match (num("energy_now"), num("energy_full")) {
        (Some(n), Some(f)) => (n, f, num("power_now")),
        _ => (num("charge_now").unwrap_or(0.0), num("charge_full").unwrap_or(0.0), num("current_now")),
    };
    let rate = rate.map(f64::abs).filter(|r| *r > 0.0);
    let secs_left = match (state, rate) {
        (Power::Discharging, Some(r)) => Some((now / r * 3600.0) as u64),
        (Power::Charging, Some(r)) if full > now => Some(((full - now) / r * 3600.0) as u64),
        _ => None,
    };
    Some(Battery { percent, state, watts: if state == Power::Full { None } else { watts }, secs_left })
}

fn watts_text(w: f64) -> String {
    if w < 10.0 { format!("{w:.1}W") } else { format!("{w:.0}W") }
}

fn time_text(secs: u64) -> String {
    let m = secs / 60;
    if m < 60 { format!("{m}m") } else { format!("{}h{:02}m", m / 60, m % 60) }
}

fn battery(cfg: &Loaded, o: &mut Outcome) {
    let root = std::env::var_os("TWIG_POWER_SUPPLY").map(std::path::PathBuf::from).unwrap_or_else(|| "/sys/class/power_supply".into());
    let Some(b) = find_battery(&root).and_then(|d| read_battery(&d)) else {
        for f in ["battery", "battery_pct", "battery_watts", "battery_time", "battery_state"] {
            o.fact(f, None);
        }
        o.notice("battery", None);
        return;
    };
    let low = b.state == Power::Discharging && b.percent <= cfg.raw.battery.low;
    let state = match b.state {
        Power::Charging => "charging",
        Power::Discharging if low => "low",
        Power::Discharging => "discharging",
        Power::Full => "full",
    };
    // Always there, and small: 85% +12W charging, 85% -8.4W on battery.
    let sign = if b.state == Power::Charging { "+" } else { "-" };
    let watts = b.watts.map(|w| format!("{sign}{}", watts_text(w)));
    let compact = match &watts {
        Some(w) => format!("{}% {w}", b.percent),
        None => format!("{}%", b.percent),
    };
    o.fact("battery", Some(compact));
    o.fact("battery_pct", Some(format!("{}%", b.percent)));
    o.fact("battery_watts", watts);
    o.fact("battery_time", b.secs_left.map(time_text));
    o.fact("battery_state", Some(state.into()));
    // And loud when it runs low.
    let n = low.then(|| {
        let left = b.secs_left.map(|s| format!(", {} left", time_text(s))).unwrap_or_default();
        if b.percent <= 5 {
            (Level::Error, format!("battery {}%{left}; plug in", b.percent))
        } else {
            (Level::Warn, format!("battery {}%{left}", b.percent))
        }
    });
    o.notice("battery", n);
}

fn units(o: &mut Outcome) {
    let mut failed = Vec::new();
    for user in [false, true] {
        let mut c = Command::new("systemctl");
        if user {
            c.arg("--user");
        }
        c.args(["list-units", "--failed", "--plain", "--no-legend", "--no-pager"]);
        if let Ok(r) = run_with_limit(&mut c, Duration::from_secs(10)) {
            failed.extend(r.stdout.lines().filter_map(|l| l.split_whitespace().next()).map(|u| u.trim_end_matches(".service").to_string()));
        }
    }
    let n = (!failed.is_empty()).then(|| {
        let shown: Vec<&str> = failed.iter().take(3).map(String::as_str).collect();
        let more = if failed.len() > 3 { format!(" +{}", failed.len() - 3) } else { String::new() };
        (Level::Warn, format!("failed: {}{more}", shown.join(", ")))
    });
    o.notice("units", n);
}

fn custom(p: &Probe, timeout: Duration, o: &mut Outcome) {
    let key = format!("probe:{}", p.name);
    let ran = run_with_limit(Command::new("sh").arg("-c").arg(&p.run), timeout);
    let (ok, value) = match ran {
        Ok(r) if !r.timed_out => (r.status == Some(0), r.stdout.lines().next().map(crate::util::clean)),
        Ok(_) => (false, Some(format!("timed out after {}", fmt_duration(timeout.as_millis() as u64)))),
        Err(e) => (false, Some(e.to_string())),
    };
    let value = value.filter(|v| !v.is_empty());
    o.fact(&p.name, if ok { value.clone() } else { None });
    let n = match (&p.notice, ok) {
        (Some(text), false) => Some((Level::parse(&p.level).unwrap_or(Level::Warn), text.replace("{value}", value.as_deref().unwrap_or("")))),
        _ => None,
    };
    o.notice(&key, n);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(size(512.0), "512B");
        assert_eq!(size(1536.0), "1.5K");
        assert_eq!(size(50.0 * 1024.0 * 1024.0 * 1024.0), "50G");
    }

    /// A battery directory as the kernel has it, with these files.
    fn fake(files: &[(&str, &str)]) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!("twig-bat-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        fs::create_dir_all(&d).unwrap();
        for (f, v) in files {
            fs::write(d.join(f), format!("{v}\n")).unwrap();
        }
        d
    }

    #[test]
    fn battery_with_power_and_energy() {
        // 40 Wh of 50, 8 W out: 5 hours left.
        let d = fake(&[("capacity", "80"), ("status", "Discharging"), ("power_now", "8000000"), ("energy_now", "40000000"), ("energy_full", "50000000")]);
        let b = read_battery(&d).unwrap();
        assert_eq!((b.percent, b.state, b.watts, b.secs_left), (80, Power::Discharging, Some(8.0), Some(5 * 3600)));
        // 10 Wh to go at 20 W in: half an hour.
        let d = fake(&[("capacity", "80"), ("status", "Charging"), ("power_now", "20000000"), ("energy_now", "40000000"), ("energy_full", "50000000")]);
        assert_eq!(read_battery(&d).unwrap().secs_left, Some(1800));
    }

    #[test]
    fn battery_with_current_and_charge() {
        // 2 A at 12 V is 24 W; 3 Ah left at 2 A is 1.5 hours.
        let d = fake(&[("capacity", "60"), ("status", "Discharging"), ("current_now", "2000000"), ("voltage_now", "12000000"), ("charge_now", "3000000"), ("charge_full", "5000000")]);
        let b = read_battery(&d).unwrap();
        assert_eq!((b.watts, b.secs_left), (Some(24.0), Some(5400)));
    }

    #[test]
    fn full_on_power_has_no_flow() {
        let d = fake(&[("capacity", "100"), ("status", "Not charging"), ("power_now", "0")]);
        let b = read_battery(&d).unwrap();
        assert_eq!((b.state, b.watts, b.secs_left), (Power::Full, None, None));
    }

    #[test]
    fn a_mouse_is_not_the_battery() {
        let root = fake(&[]);
        for (dir, kind, scope) in [("hidpp_battery_0", "Battery", "Device"), ("AC", "Mains", "")] {
            fs::create_dir_all(root.join(dir)).unwrap();
            fs::write(root.join(dir).join("type"), kind).unwrap();
            fs::write(root.join(dir).join("scope"), scope).unwrap();
        }
        assert_eq!(find_battery(&root), None);
        fs::create_dir_all(root.join("BAT0")).unwrap();
        assert_eq!(find_battery(&root), Some(root.join("BAT0")));
    }

    #[test]
    fn custom_probe_value_and_notice() {
        let p = |run: &str| Probe {
            name: "x".into(),
            run: run.into(),
            every: "1m".into(),
            hosts: vec![],
            notice: Some("broken: {value}".into()),
            level: "error".into(),
            timeout: "5s".into(),
        };
        let mut o = Outcome::default();
        custom(&p("echo hello; echo more"), Duration::from_secs(5), &mut o);
        assert_eq!(o.facts, vec![("x".to_string(), Some("hello".to_string()))]);
        assert!(o.notices[0].1.is_none());
        let mut o = Outcome::default();
        custom(&p("echo nope; exit 3"), Duration::from_secs(5), &mut o);
        assert_eq!(o.facts[0].1, None);
        let n = o.notices[0].1.as_ref().unwrap();
        assert_eq!((n.text.as_str(), n.level), ("broken: nope", Level::Error));
        let mut o = Outcome::default();
        custom(&p("sleep 5"), Duration::from_millis(100), &mut o);
        assert!(o.notices[0].1.as_ref().unwrap().text.contains("timed out"));
    }
}
