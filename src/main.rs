// The C entry point, not Rust's: a prompt is a new process each time, and
// Rust's own start (a stack overflow handler, a check of the standard
// descriptors, a read of /proc/self/maps) is 50 µs it doesn't need. (The
// test harness brings its own main.)
#![cfg_attr(not(test), no_main)]

mod boxes;
mod client;
mod config;
mod daemon;
mod dirs;
mod git;
mod history;
mod jobs;
mod notices;
mod paths;
mod pick;
mod probes;
mod proto;
mod render;
mod setup;
mod tcsh;
mod template;
mod util;

const HELP: &str = "\
twig: a fast tcsh prompt, with a daemon that keeps it up to date.

  twig status           how it is: daemon, settings, shells, git, notices, jobs
  twig info [FILE]      all of it in detail: what is wrong with the settings,
                          what each part costs, the prompt the settings make
  twig timings          how long each prompt takes, above it; again to stop

  twig setup            put twig on this machine: settings, history, the service
  twig restart          the daemon, with or without systemd
  twig remove           take the service off this machine

Notices: messages on the prompt that go away by themselves.

  twig notify [--warn|--error] [--for 10m] [--shows N] [--here] TEXT
  twig notices          the ones showing now, and the last ones to go away
  twig notices --ignore PATTERN|ID   never show those (--unignore to undo)
  twig ack [ID]         dismiss one, or all in this shell

Jobs: commands that run in the background and report on the prompt.

  twig run COMMAND...   start one
  twig jobs             the recent ones
  twig log [ID] [-f]    a job's output (the newest one's by default)
  twig kill [ID] [-9]   stop one

History and directories:

  twig history [WORDS]  search every command from every shell (twig h --help)
  Ctrl-R                the same, to pick one onto the line
  z WORDS               go to the best directory with those words in its path
  twig dirs             the directories z knows

What the shell runs (set up by twig init tcsh):

  twig init tcsh        the aliases for ~/.cshrc
  twig prompt PID STATUS, twig pick PID
  twig daemon           the daemon itself, in the foreground
";

/// The version and the commit it was built from: `0.1.0-abc1234`, with
/// `-dirty` if the tree had changes not committed.
fn version() -> String {
    // Unset where git could not be asked, or vergen's stand-in.
    let Some(sha) = option_env!("VERGEN_GIT_SHA").filter(|sha| !sha.is_empty() && !sha.contains("VERGEN")) else {
        return env!("CARGO_PKG_VERSION").to_string();
    };
    let dirty = if option_env!("VERGEN_GIT_DIRTY") == Some("true") { "-dirty" } else { "" };
    format!("{}-{sha}{dirty}", env!("CARGO_PKG_VERSION"))
}

#[cfg_attr(not(test), no_mangle)]
pub extern "C" fn main(_argc: libc::c_int, _argv: *const *const libc::c_char) -> libc::c_int {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    let rest: Vec<String> = args.collect();
    // Without Rust's start, SIGPIPE is left as it was: `twig h | head` ends
    // quietly, as other commands do. The daemon sets it to be ignored.
    if cmd != "daemon" {
        unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    }
    let code = match cmd.as_str() {
        "prompt" => client::prompt(&rest),
        "hook" => client::hook(&rest),
        "init" => tcsh::init(&rest),
        "daemon" => daemon::run(),
        "run" => jobs::run(&rest),
        "__keeper" => jobs::keeper(&rest),
        "jobs" => jobs::list(&rest),
        "log" => jobs::log(&rest),
        "kill" => jobs::kill(&rest),
        "notify" => notices::notify_cmd(&rest),
        "ack" => notices::ack_cmd(&rest),
        "history" | "h" => history::cmd(&rest),
        "pick" => pick::run(&rest),
        "z" => dirs::z_cmd(&rest),
        "dirs" => dirs::list_cmd(&rest),
        // The older names lead to the two reports.
        "status" => setup::status(),
        "notices" => notices::list_cmd(&rest),
        "info" | "check" | "cost" | "debug" => setup::info(&rest),
        "timings" => client::timings(),
        "setup" => setup::setup(),
        "remove" => setup::remove(),
        "restart" => setup::restart(),
        "reload" | "stop" => client::simple(&cmd),
        "" | "help" | "-h" | "--help" => {
            print!("{HELP}");
            0
        }
        "version" | "-V" | "--version" => {
            println!("twig {}", version());
            0
        }
        other => {
            eprintln!("twig: no command \"{other}\"\n\n{HELP}");
            2
        }
    };
    std::process::exit(code);
}
