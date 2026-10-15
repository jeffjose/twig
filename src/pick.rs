//! The history search behind Ctrl-R.
//!
//! tcsh can't take text into the line being edited from a program, so the
//! key is bound to a macro: it runs `_twig_pick`, which runs this and then
//! loads the chosen command into tcsh's history with `history -L`; the
//! macro's last key, Ctrl-P, then brings it up on the line, to edit or run.

use crate::history::{self, Entry};
use crate::paths;
use crate::template::Style;
use crate::util::{char_width, fmt_ago, now_ms};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;

#[derive(Clone, Copy, PartialEq)]
enum Scope {
    All,
    Here,
    Shell,
}

impl Scope {
    fn next(self) -> Scope {
        match self {
            Scope::All => Scope::Here,
            Scope::Here => Scope::Shell,
            Scope::Shell => Scope::All,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Scope::All => "all",
            Scope::Here => "here",
            Scope::Shell => "this shell",
        }
    }
}

struct Tty {
    f: File,
    saved: libc::termios,
}

impl Tty {
    fn open() -> std::io::Result<Tty> {
        let f = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
        let mut t: libc::termios = unsafe { std::mem::zeroed() };
        unsafe { libc::tcgetattr(f.as_raw_fd(), &mut t) };
        let saved = t;
        unsafe {
            libc::cfmakeraw(&mut t);
            t.c_oflag |= libc::OPOST;
            libc::tcsetattr(f.as_raw_fd(), libc::TCSANOW, &t);
        }
        let mut tty = Tty { f, saved };
        tty.put("\x1b[?1049h\x1b[H\x1b[2J");
        Ok(tty)
    }

    fn put(&mut self, s: &str) {
        let _ = self.f.write_all(s.as_bytes());
    }

    fn size(&self) -> (usize, usize) {
        let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
        unsafe { libc::ioctl(self.f.as_raw_fd(), libc::TIOCGWINSZ, &mut ws) };
        (ws.ws_row.max(5) as usize, ws.ws_col.max(20) as usize)
    }

    /// A byte, or None if nothing comes within `ms` (for telling Escape
    /// alone from the start of an arrow key).
    fn byte(&mut self, ms: i32) -> Option<u8> {
        let mut p = libc::pollfd { fd: self.f.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        if unsafe { libc::poll(&mut p, 1, ms) } <= 0 {
            return None;
        }
        let mut b = [0u8; 1];
        (self.f.read(&mut b).ok()? == 1).then_some(b[0])
    }
}

impl Drop for Tty {
    fn drop(&mut self) {
        self.put("\x1b[?1049l");
        unsafe { libc::tcsetattr(self.f.as_raw_fd(), libc::TCSANOW, &self.saved) };
    }
}

enum Key {
    Char(char),
    Up,
    Down,
    PageUp,
    PageDown,
    Enter,
    Back,
    ClearLine,
    DeleteWord,
    Tab,
    Cancel,
    Other,
}

fn key(t: &mut Tty) -> Key {
    let Some(b) = t.byte(-1) else { return Key::Cancel };
    match b {
        0x1b => match t.byte(30) {
            None => Key::Cancel,
            Some(b'[') | Some(b'O') => match t.byte(30) {
                Some(b'A') => Key::Up,
                Some(b'B') => Key::Down,
                Some(b'5') => {
                    t.byte(30);
                    Key::PageUp
                }
                Some(b'6') => {
                    t.byte(30);
                    Key::PageDown
                }
                _ => Key::Other,
            },
            Some(_) => Key::Other,
        },
        b'\r' | b'\n' => Key::Enter,
        0x7f | 0x08 => Key::Back,
        0x15 => Key::ClearLine,
        0x17 => Key::DeleteWord,
        0x09 => Key::Tab,
        0x03 | 0x07 | 0x04 => Key::Cancel,
        0x10 | 0x12 => Key::Up,
        0x0e | 0x13 => Key::Down,
        b if b < 0x20 => Key::Other,
        b => {
            // UTF-8: gather the rest of the character.
            let n = if b >= 0xf0 { 3 } else if b >= 0xe0 { 2 } else if b >= 0xc0 { 1 } else { 0 };
            let mut bytes = vec![b];
            for _ in 0..n {
                bytes.extend(t.byte(30));
            }
            String::from_utf8(bytes).ok().and_then(|s| s.chars().next()).map_or(Key::Other, Key::Char)
        }
    }
}

fn cut(s: &str, width: usize) -> String {
    let mut w = 0;
    let mut out = String::new();
    for c in s.chars() {
        let cw = char_width(c);
        if w + cw > width {
            out.pop();
            out.push('…');
            return out;
        }
        w += cw;
        out.push(if c.is_control() { ' ' } else { c });
    }
    out
}

/// `twig pick PID`
pub fn run(args: &[String]) -> i32 {
    let pid: u32 = args.first().and_then(|a| a.parse().ok()).unwrap_or_else(|| unsafe { libc::getppid() } as u32);
    let out = paths::runtime_dir_made().join(format!("pick.{pid}"));
    let entries = history::load_all();
    let cwd = std::env::current_dir().ok().and_then(|p| p.to_str().map(String::from)).unwrap_or_default();
    let chosen = match Tty::open() {
        Ok(mut tty) => ui(&mut tty, &entries, &cwd, pid),
        Err(e) => {
            eprintln!("twig pick: /dev/tty: {e}");
            return 1;
        }
    };
    // Cancelled: the shell's previous command, so the Ctrl-P that follows
    // brings up what the up arrow would have.
    let line = chosen.or_else(|| {
        entries.iter().rev().find(|e| e.p == pid).or(entries.last()).map(|e| e.c.clone())
    });
    let text = line.map(|c| format!("#+{}\n{c}\n", now_ms() / 1000)).unwrap_or_default();
    match std::fs::write(&out, text) {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

fn ui(tty: &mut Tty, entries: &[Entry], cwd: &str, pid: u32) -> Option<String> {
    let mut query = String::new();
    let mut scope = Scope::All;
    let mut sel = 0usize;
    let mut top = 0usize;
    let now = now_ms();
    let dim = Style::of("dim");
    let red = Style::of("red");
    let hi = Style::of("bold bg:236");
    // Each command in lower case, once: every keystroke searches them all.
    let lower: Vec<String> = entries.iter().map(|e| e.c.to_lowercase()).collect();
    loop {
        let terms: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();
        let f = history::Filter {
            terms,
            here: (scope == Scope::Here).then(|| cwd.to_string()),
            failed: false,
            ok: false,
            host: None,
            shell: (scope == Scope::Shell).then_some(pid),
        };
        let list = history::unique_newest(entries.iter().zip(&lower).filter(|(e, l)| f.matches(e, l)).map(|(e, _)| e));
        let (rows, cols) = tty.size();
        let height = rows - 2;
        sel = sel.min(list.len().saturating_sub(1));
        if sel < top {
            top = sel;
        }
        if sel >= top + height {
            top = sel + 1 - height;
        }

        // The newest at the bottom, just above the query.
        let mut screen = String::from("\x1b[H");
        for row in 0..height {
            let i = top + (height - 1 - row);
            screen.push_str("\x1b[2K");
            if let Some(e) = list.get(i) {
                let age = format!("{:>4} ", fmt_ago(e.t, now));
                let mark = if e.s.is_some_and(|s| s != 0) { red.paint("✘") } else { " ".into() };
                let text = cut(&e.c, cols.saturating_sub(8));
                let line = format!("{} {mark} {text}", dim.paint(&age));
                if i == sel {
                    screen.push_str(&format!("{}{age} {mark}{} {text}\x1b[K\x1b[0m", hi.ansi(), hi.ansi()));
                } else {
                    screen.push_str(&line);
                }
            }
            screen.push_str("\r\n");
        }
        let info = format!("  {}/{}  {}   tab: all, here, this shell   esc: cancel", list.len(), entries.len(), scope.name());
        screen.push_str(&format!("\x1b[2K{}\r\n\x1b[2K{} {}", dim.paint(&cut(&info, cols - 1)), Style::of("cyan,bold").paint(">"), query));
        tty.put(&screen);

        match key(tty) {
            Key::Char(c) => {
                query.push(c);
                sel = 0;
                top = 0;
            }
            Key::Back => {
                query.pop();
                sel = 0;
            }
            Key::ClearLine => {
                query.clear();
                sel = 0;
            }
            Key::DeleteWord => {
                let t = query.trim_end().len();
                query.truncate(query[..t].rfind(' ').map_or(0, |i| i + 1));
                sel = 0;
            }
            Key::Up => sel = (sel + 1).min(list.len().saturating_sub(1)),
            Key::Down => sel = sel.saturating_sub(1),
            Key::PageUp => sel = (sel + height).min(list.len().saturating_sub(1)),
            Key::PageDown => sel = sel.saturating_sub(height),
            Key::Tab => {
                scope = scope.next();
                sel = 0;
                top = 0;
            }
            Key::Enter => return list.get(sel).map(|e| e.c.clone()),
            Key::Cancel => return None,
            Key::Other => {}
        }
    }
}
