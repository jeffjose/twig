//! What tcsh needs: prompts written in its escapes, and the aliases that
//! connect a shell to twig.

use crate::config;
use crate::paths;
use std::env;

/// Turn rendered text (with ANSI escapes and newlines) into a value for
/// tcsh's `prompt`: escapes inside %{ %} so tcsh knows they take no room,
/// % and ! escaped, newlines as \n.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut chars = text.chars().peekable();
    let mut after_escape = false;
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => {
                let mut seq = String::from('\x1b');
                if chars.peek() == Some(&'[') {
                    seq.push(chars.next().unwrap());
                    for c in chars.by_ref() {
                        seq.push(c);
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                // Join neighboring escapes into one %{ %}.
                if after_escape && out.ends_with("%}") {
                    out.truncate(out.len() - 2);
                    out.push_str(&seq);
                    out.push_str("%}");
                } else {
                    out.push_str("%{");
                    out.push_str(&seq);
                    out.push_str("%}");
                }
                after_escape = true;
                continue;
            }
            '\n' => {
                // tcsh misreads %}\n; a space between them is invisible.
                if after_escape {
                    out.push(' ');
                }
                out.push_str("\\n");
            }
            '%' => out.push_str("%%"),
            '!' => out.push_str("\\!"),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
        after_escape = false;
    }
    out
}

/// `twig init tcsh`: commands for .cshrc, on one line so that
/// eval "`twig init tcsh`" works.
pub fn init(args: &[String]) -> i32 {
    if args.first().map(String::as_str) != Some("tcsh") {
        eprintln!("usage: twig init tcsh\n\nPut this at the end of ~/.cshrc:\n\n    if ( -X twig ) eval \"`twig init tcsh`\"");
        return 2;
    }
    let exe = env::current_exe().ok().and_then(|p| p.to_str().map(String::from)).unwrap_or_else(|| "twig".into());
    let cfg = config::read(&paths::config_file()).map(|l| l.raw).unwrap_or_default();
    let rt = paths::runtime_dir();
    let rt = rt.display();
    let hist = paths::tcsh_history();
    let key = &cfg.history.key;
    let lines = [
        format!("set _twig_rt = \"{rt}\""),
        // The twig this shell started with, found once, so that no prompt goes
        // through a mise shim (about 18 ms each); if it goes away (an upgrade),
        // twig from PATH. -x is a builtin test, so the check is free.
        format!("set _twig = \"{exe}\""),
        format!("alias precmd 'if (! -x \"$_twig\") set _twig = twig; set _tw = (\"`$_twig prompt $$ $status`\"); set prompt = \"$_tw[1]\"; set rprompt = \"$_tw[2]\"'"),
        // The history builtin gives the line about to run, with its event
        // number: an empty line runs postcmd too, and repeats the number.
        // ($_ is the previous command here, and is evaluated again when
        // quoted into a command.)
        format!("alias postcmd 'history 1 >! \"$_twig_rt/cmd.$$\"; $_twig hook $$'"),
        format!("alias _twig_pick '$_twig pick $$ && history -L \"$_twig_rt/pick.$$\"'"),
        format!("alias z 'set _twz = \"`$_twig z \\!*`\"; if (\"$_twz\" != \"\") cd \"$_twz\"'"),
        format!("bindkey -s '{key}' '^A^K_twig_pick^M^P'"),
        "unset savehist".into(),
        format!("set history = {}", cfg.history.size),
        format!("if (-r \"{}\") history -L \"{}\"", hist.display(), hist.display()),
    ];
    println!("{}", lines.join("; "));
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes() {
        assert_eq!(escape("\x1b[32mok\x1b[0m"), "%{\x1b[32m%}ok%{\x1b[0m%}");
        assert_eq!(escape("\x1b[0m\x1b[1m!"), "%{\x1b[0m\x1b[1m%}\\!");
        assert_eq!(escape("a\x1b[0m\n$ "), "a%{\x1b[0m%} \\n$ ");
        assert_eq!(escape("90% done"), "90%% done");
        assert_eq!(escape("a\\b"), "a\\\\b");
    }
}
