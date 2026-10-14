//! What shells and commands say to the daemon: one JSON line each way, one
//! request per connection.

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// precmd: the last command has finished; draw the prompt.
    Prompt { pid: u32, status: i32, cwd: String, env: Vec<(String, String)> },
    /// postcmd: a command is about to run.
    /// `event` is the history number of the line; an empty line repeats
    /// the one before.
    Hook { pid: u32, cmd: String, cwd: String, #[serde(default)] event: Option<u64> },
    Notify { text: String, level: String, ttl: Option<u64>, shows: Option<u32>, pid: Option<u32>, key: Option<String> },
    Ack { id: Option<u64>, pid: Option<u32> },
    Notices,
    JobStart { cmd: String, cwd: String, shell: u32 },
    JobPid { id: u64, pid: u32 },
    JobEnd { id: u64, status: i32 },
    JobSeen { id: u64 },
    Jobs,
    /// Turn the timings line on or off for a shell.
    Timings { pid: u32 },
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
    let mut buf = Vec::new();
    s.read_to_end(&mut buf)?;
    let v = serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok((v, connect))
}
