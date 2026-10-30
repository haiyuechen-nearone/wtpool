//! The program of a simple command: where it starts after its prefixes, the script it runs
//! when it is a shell, and its summary.

use super::{basename, has_substitution};

/// Programs whose next word joins the summary.
const WITH_SUBCOMMAND: [&str; 8] = [
    "git", "cargo", "gh", "nix", "npm", "wtpool", "docker", "kubectl",
];

/// Programs that start a coding agent: a call that runs one moves its caller nowhere, as that
/// agent works where it is told while its caller stays where it is.
pub(crate) const AGENT_PROGRAMS: [&str; 18] = [
    "claude",
    "codex",
    "copilot",
    "cursor-agent",
    "gemini",
    "qwen",
    "opencode",
    "goose",
    "amp",
    "cline",
    "droid",
    "kiro-cli",
    "junie",
    "devin",
    "auggie",
    "crush",
    "cn",
    "aider",
];

/// Shells whose script, after an option word that holds `c`, is a command line of its own.
pub(crate) const SHELLS: [&str; 4] = ["sh", "bash", "zsh", "dash"];

/// Options of a shell whose value is the next word.
const SHELL_VALUED: [&str; 6] = ["-o", "+o", "-O", "+O", "--rcfile", "--init-file"];

/// Reserved words that may stand before the program of a simple command.
const LEADING_RESERVED: [&str; 8] = ["then", "do", "else", "elif", "if", "while", "until", "!"];

/// Where the program of a simple command starts, after its prefixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgramStart {
    /// The index of the program's word, the length of the words when none follows.
    pub at: usize,
    /// One of the prefixes is `wtpool job` with its options up to `--`.
    pub wrapped: bool,
    /// One of the prefixes is `timeout` or `gtimeout` with a duration above 0.
    pub timed: bool,
    /// The index of the word where `wtpool job -- ` goes to wrap the command: after the
    /// reserved words, assignments and shell builtins that lead it.
    pub wrap_at: usize,
}

/// The [`ProgramStart`] of the simple command `words`. Its prefixes are `VAR=value`, `env`,
/// `timeout N`, `gtimeout N`, `time`, `nice`, `exec`, `command`, `builtin`, `nix develop … -c`
/// and `wtpool job … --`, after the reserved words that may lead a command.
pub(crate) fn program_start(words: &[String]) -> ProgramStart {
    let reserved = words
        .iter()
        .take_while(|word| LEADING_RESERVED.contains(&word.as_str()))
        .count();
    let mut start = ProgramStart {
        at: reserved,
        wrapped: false,
        timed: false,
        wrap_at: reserved,
    };
    let mut in_shell = true;
    while let Some(prefix) = prefix_at(words, start.at) {
        start.at = prefix.next;
        start.wrapped |= prefix.wrapped;
        start.timed |= prefix.timed;
        in_shell &= prefix.in_shell;
        if in_shell {
            start.wrap_at = start.at;
        }
    }
    start
}

/// A program that runs the words after its options as a command.
struct Runner {
    names: &'static [&'static str],
    /// Its options whose value is the next word.
    valued: &'static [&'static str],
    /// A duration follows its options.
    timed: bool,
    /// The shell runs it itself, so `wtpool job` cannot.
    builtin: bool,
}

const RUNNERS: [Runner; 3] = [
    Runner {
        names: &["time", "nice"],
        valued: &["-n"],
        timed: false,
        builtin: false,
    },
    Runner {
        names: &["timeout", "gtimeout"],
        valued: &["-k", "-s", "--kill-after", "--signal"],
        timed: true,
        builtin: false,
    },
    Runner {
        names: &["exec"],
        valued: &["-a"],
        timed: false,
        builtin: true,
    },
];

/// One prefix before the program of a simple command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Prefix {
    /// The index of the word after it.
    next: usize,
    /// It is `wtpool job` with its options up to `--`.
    wrapped: bool,
    /// It is `timeout` or `gtimeout` with a duration above 0.
    timed: bool,
    /// It is an assignment or a shell builtin, so a wrapper goes after it.
    in_shell: bool,
}

/// The prefix that starts at word `at` of `words`, `None` when that word is the program, or
/// there is none.
fn prefix_at(words: &[String], at: usize) -> Option<Prefix> {
    let word = words.get(at)?;
    let next = |from: usize| words.get(from).map(String::as_str);
    let mut prefix = Prefix {
        next: at.saturating_add(1),
        wrapped: false,
        timed: false,
        in_shell: false,
    };
    if is_assignment(word) {
        prefix.in_shell = true;
        return Some(prefix);
    }
    let name = basename(word);
    if let Some(runner) = RUNNERS
        .iter()
        .find(|runner| runner.names.contains(&name.as_str()))
    {
        while let Some(option) = next(prefix.next).filter(|w| w.starts_with('-')) {
            let width = if runner.valued.contains(&option) {
                2
            } else {
                1
            };
            prefix.next = prefix.next.saturating_add(width);
        }
        if runner.timed
            && let Some(duration) = next(prefix.next)
        {
            prefix.timed = is_positive(duration);
            prefix.next = prefix.next.saturating_add(1);
        }
        prefix.in_shell = runner.builtin;
        return Some(prefix);
    }
    match name.as_str() {
        "env" => {
            while next(prefix.next).is_some_and(|w| w.starts_with('-') || is_assignment(w)) {
                prefix.next = prefix.next.saturating_add(1);
            }
        }
        "command" => {
            while let Some(option) = next(prefix.next).filter(|w| w.starts_with('-')) {
                // `-v` and `-V` print the program's kind instead of running it.
                if option != "--" && option.contains(['v', 'V']) {
                    return None;
                }
                prefix.next = prefix.next.saturating_add(1);
            }
            prefix.in_shell = true;
        }
        "builtin" => prefix.in_shell = true,
        "nix" if next(at.saturating_add(1)) == Some("develop") => {
            let flag = (at.saturating_add(2)..words.len())
                .find(|&i| matches!(next(i), Some("-c" | "--command")))?;
            prefix.next = flag.saturating_add(1);
        }
        "wtpool" if next(at.saturating_add(1)) == Some("job") => {
            let dashes = (at.saturating_add(2)..words.len()).find(|&i| next(i) == Some("--"))?;
            prefix.next = dashes.saturating_add(1);
            prefix.wrapped = true;
        }
        _ => return None,
    }
    Some(prefix)
}

/// `NAME=value` or `NAME+=value`.
fn is_assignment(word: &str) -> bool {
    match word.split_once('=') {
        Some((name, _)) => {
            let name = name.strip_suffix('+').unwrap_or(name);
            !name.is_empty()
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && !name.starts_with(|c: char| c.is_ascii_digit())
        }
        None => false,
    }
}

/// `true` for a duration as `timeout` reads it, a number with an optional unit `s`, `m`, `h` or
/// `d`, above 0.
fn is_positive(duration: &str) -> bool {
    let number = duration
        .strip_suffix(['s', 'm', 'h', 'd'])
        .unwrap_or(duration);
    number
        .parse::<f64>()
        .is_ok_and(|value| value.is_finite() && value > 0.0)
}

/// The index in `args`, the arguments of a shell, of the script it runs: the first word that is
/// no option after an option word that holds `c`.
pub(super) fn shell_script(args: &[String]) -> Option<usize> {
    let mut command = false;
    let mut index: usize = 0;
    while let Some(arg) = args.get(index) {
        let valued = SHELL_VALUED.contains(&arg.as_str());
        if !valued && !arg.starts_with(['-', '+']) {
            return command.then_some(index);
        }
        if valued {
            index = index.saturating_add(1);
        } else if arg.starts_with('-') && !arg.starts_with("--") {
            command |= arg.contains('c');
        }
        index = index.saturating_add(1);
    }
    None
}

/// The summary of the simple command `words`: its program's basename plus, for a listed program,
/// its next word. `None` when no program follows the prefixes, or the program word is a piece of
/// a construct the lexer missed.
pub(crate) fn summary_of(words: &[String]) -> Option<String> {
    let start = program_start(words).at;
    let program = words.get(start).filter(|word| is_program(word))?;
    let next = words.get(start.saturating_add(1)).map(String::as_str);
    Some(summary(&basename(program), next))
}

/// `program` plus, for a listed program, `next` when it matches `^[a-z][a-z0-9-]{0,23}$`.
pub(super) fn summary(program: &str, next: Option<&str>) -> String {
    let sub = next.filter(|word| {
        WITH_SUBCOMMAND.contains(&program)
            && (1..=24).contains(&word.len())
            && word.starts_with(|c: char| c.is_ascii_lowercase())
            && word
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    });
    match sub {
        Some(sub) => format!("{program} {sub}"),
        None => program.to_owned(),
    }
}

/// `false` for a word that starts with `-` or still holds a substitution or `)`: such a word is
/// a piece of a construct the lexer missed, so it names no program.
pub(super) fn is_program(word: &str) -> bool {
    !word.starts_with('-') && !has_substitution(word) && !word.contains(')')
}

#[cfg(test)]
mod tests;
