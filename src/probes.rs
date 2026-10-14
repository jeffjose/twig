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
        ("battery".to_string(), Kind::Battery, Duration::from_secs(30)),
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

fn battery(cfg: &Loaded, o: &mut Outcome) {
    let bat = fs::read_dir("/sys/class/power_supply").into_iter().flatten().flatten().find(|e| {
        e.file_name().to_string_lossy().starts_with("BAT")
    });
    let Some(bat) = bat else {
        o.fact("battery", None);
        return;
    };
    let cap: Option<u8> = read_trimmed(bat.path().join("capacity")).and_then(|c| c.parse().ok());
    let status = read_trimmed(bat.path().join("status")).unwrap_or_default();
    let Some(cap) = cap else { return };
    let value = match status.as_str() {
        "Charging" => Some(format!("{cap}%⚡")),
        "Discharging" => Some(format!("{cap}%")),
        _ => None,
    };
    o.fact("battery", value);
    let low = status == "Discharging" && cap <= cfg.raw.battery.low;
    let level = if cap <= 5 { Level::Error } else { Level::Warn };
    o.notice("battery", low.then(|| (level, format!("battery {cap}%"))));
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
