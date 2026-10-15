//! Where twig keeps things. Every location follows the XDG variables, so a
//! test can point all of them somewhere else.

use std::env;
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;

pub fn home() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match env::var_os(var) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home().join(fallback),
    }
}

pub fn config_file() -> PathBuf {
    if let Some(p) = env::var_os("TWIG_CONFIG") {
        return PathBuf::from(p);
    }
    xdg("XDG_CONFIG_HOME", ".config").join("twig/config.toml")
}

/// The socket, lock and other per-boot files. A shell started without
/// XDG_RUNTIME_DIR (su, an old screen) still finds the daemon that systemd
/// started, because /run/user/UID is tried before /tmp.
///
/// Only the path: a prompt asks for it, and the directory is made where
/// twig starts (`runtime_dir_made`).
pub fn runtime_dir() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    let base = match env::var_os("XDG_RUNTIME_DIR") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let run = PathBuf::from(format!("/run/user/{uid}"));
            if run.is_dir() {
                run
            } else {
                PathBuf::from(format!("/tmp/twig-{uid}"))
            }
        }
    };
    if base.ends_with(format!("twig-{uid}")) { base } else { base.join("twig") }
}

/// The runtime directory, made if it wasn't there.
pub fn runtime_dir_made() -> PathBuf {
    let dir = runtime_dir();
    make_private(&dir);
    dir
}

pub fn socket() -> PathBuf {
    runtime_dir().join("sock")
}

pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("twig")
}

pub fn state_dir() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("twig")
}

pub fn history_dir() -> PathBuf {
    data_dir().join("history")
}

/// One history file per machine, so that copying the folder between machines
/// merges their histories without either one writing to the other's file.
pub fn history_file(host: &str) -> PathBuf {
    history_dir().join(format!("{host}.jsonl"))
}

/// The history in tcsh's own format, for `history -L` when a shell starts.
pub fn tcsh_history() -> PathBuf {
    runtime_dir().join("history.tcsh")
}

pub fn dirs_file(host: &str) -> PathBuf {
    data_dir().join(format!("dirs/{host}.json"))
}

pub fn jobs_dir() -> PathBuf {
    state_dir().join("jobs")
}

pub fn make_private(dir: &std::path::Path) {
    let _ = fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
}

/// The short host name, as the kernel has it.
pub fn hostname() -> String {
    let mut buf = [0u8; 256];
    let ok = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0;
    let name = if ok {
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).into_owned()
    } else {
        String::new()
    };
    let short = name.split('.').next().unwrap_or("").to_string();
    if short.is_empty() { "localhost".into() } else { short }
}
