//! What shells and commands say to the daemon: one JSON line each way, one
//! request per connection.

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

/// The command line that just ran, as postcmd wrote it to a file when it
/// started: the line, when it started (ms since the epoch, the file's
/// time), and its history number, which an empty line repeats.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Ran {
    pub cmd: String,
    pub start: u64,
    pub event: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// precmd: the last command has finished; draw the prompt. The
    /// environment (10 KB or so) comes only when the daemon hasn't seen
    /// this one: `envh` is its hash, and the daemon answers {"env": "?"}
    /// when it needs the whole of it.
    Prompt {
        pid: u32,
        status: i32,
        cwd: String,
        #[serde(default)]
        envh: u64,
        #[serde(default)]
        env: Option<Vec<(String, String)>>,
        #[serde(default)]
        ran: Option<Ran>,
    },
    /// postcmd of shells set up by an older twig: a command is about to
    /// run. (Now the prompt brings the command, with `ran`.)
    Hook { pid: u32, cmd: String, cwd: String, #[serde(default)] event: Option<u64> },
    Notify { text: String, level: String, ttl: Option<u64>, shows: Option<u32>, pid: Option<u32>, key: Option<String> },
    Ack { id: Option<u64>, pid: Option<u32> },
    Notices,
    /// Mute notices that match a pattern, or (undo) stop muting it.
    Mute { pattern: String, undo: bool },
    JobStart { cmd: String, cwd: String, shell: u32 },
    JobPid { id: u64, pid: u32 },
    JobEnd { id: u64, status: i32 },
    JobSeen { id: u64 },
    Jobs,
    /// Turn the timings line on or off for a shell.
    Timings { pid: u32 },
    /// What everything costs: prompts, probes, git, the daemon.
    Cost,
    Status,
    Reload,
    Stop,
}

pub fn send(req: &Request, timeout: Duration) -> io::Result<serde_json::Value> {
    send_timed(req, timeout).map(|(v, _)| v)
}

/// As send, and how long connecting took.
pub fn send_timed(req: &Request, timeout: Duration) -> io::Result<(serde_json::Value, Duration)> {
    let t = std::time::Instant::now();
    let mut s = UnixStream::connect(crate::paths::socket())?;
    let connect = t.elapsed();
    s.set_read_timeout(Some(timeout))?;
    s.set_write_timeout(Some(timeout))?;
    let mut line = serde_json::to_vec(req)?;
    line.push(b'\n');
    s.write_all(&line)?;
    s.shutdown(std::net::Shutdown::Write)?;
    // Room for a prompt's answer at once, rather than growing to it.
    let mut buf = Vec::with_capacity(1024);
    s.read_to_end(&mut buf)?;
    let v = serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok((v, connect))
}
