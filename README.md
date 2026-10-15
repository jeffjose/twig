# twig

A tcsh prompt that is fast because a daemon does the slow work in the
background. A prompt costs the shell about 0.8 ms per command, and most
of that is tcsh starting a process at all (see Speed, below).

The prompt: time, machine, address, directory, and git.

![The prompt](docs/prompt.png)

Over ssh, the host says so.

![The prompt in an ssh session](docs/ssh.png)

How long a slow command took.

![A command that took 3 seconds](docs/duration.png)

A command that failed.

![A command that failed with code 2](docs/failed.png)

A notice, appearing while the prompt waits. No key was pressed.

![A notice on the prompt](docs/notice.png)

A job in the background, running.

![A job running](docs/job-running.png)

The same job, done.

![The job done](docs/job-done.png)

The battery, charging.

![The battery charging](docs/battery-charging.png)

The battery, low.

![The battery low, with how long is left](docs/battery-low.png)

Ctrl-R: every command from every shell.

![The history search](docs/ctrl-r.png)

`twig timings`: what each prompt waited for.

![The timings box above a prompt](docs/timings.png)

`twig status`: how twig is.

![twig status](docs/status.png)

`twig notices`: the notices now, and the ones gone.

![twig notices](docs/notices.png)

`twig info`: what each part costs.

![twig info](docs/info.png)

## Install

```sh
cargo install --path .
twig setup
```

Then at the end of `~/.cshrc`:

```tcsh
if ( -X twig ) eval "`twig init tcsh`"
```

## Commands

```
twig status            how it is
twig info [FILE]       what is wrong with the settings, what each part costs
twig timings           a box above each prompt; again to stop

twig notify [--warn|--error] [--for 10m] TEXT
twig notices           now, and the last 50 gone
twig notices --ignore PATTERN|ID      never show those; --unignore to undo
twig ack [ID]          dismiss

twig run COMMAND       in the background
twig jobs · twig log [ID] · twig kill [ID]

twig h [WORDS]         search the history; Ctrl-R to pick one
z WORDS                go to a directory you use

twig setup · twig restart · twig remove
```

## Settings

`~/.config/twig/config.toml`. Every setting, with what it does, is in
[config.toml](config.toml). Changes show at the next prompt.

## Speed

What a command costs the shell, measured on an i7-12700K (medians; `twig
timings` shows the daemon's part above each prompt):

| | per command |
|---|---|
| tcsh forking and running a process, at all (`/bin/true`) | 590 µs |
| postcmd: `history 1 >>! file`, a builtin | 25 µs |
| precmd: `twig prompt`, start to finish | 745 µs |
| of which, asking the daemon and getting the prompt back | 130 µs |
| after a command in a repository: the fresh git status it waits for | +1.6 ms here |

Before each command, tcsh appends the line to a file; after it, `twig prompt`
reads the file (its birth is when the command started) and asks the daemon
for the prompt, sending only a hash of the environment unless it changed.
The daemon answers with what it has, waiting up to `git.wait` for a fresh
git status after a command in a repository (`git status` itself is most of
that; `wait = "0"` shows the last one at once and redraws the line when the
new one comes). The binary is linked statically and skips Rust's own
start-up, so that it is ready to run 280 µs after being called.

MIT
