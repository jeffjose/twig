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
  twig prompt PID STATUS, twig hook PID COMMAND, twig pick PID
  twig daemon           the daemon itself, in the foreground
";

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    let rest: Vec<String> = args.collect();
    if cmd != "daemon" {
        // `twig h | head` ends quietly, as other commands do.
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
            println!("twig {}", env!("CARGO_PKG_VERSION"));
            0
        }
        other => {
            eprintln!("twig: no command \"{other}\"\n\n{HELP}");
            2
        }
    };
    std::process::exit(code);
}
