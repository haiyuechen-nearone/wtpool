//! The wait loops of a shell command line: `while` and `until` loops that run `sleep`, read
//! through subshells, groups, compound commands, pipelines, substitutions and the scripts of
//! `sh -c`, `bash -c` and `zsh -c`.

use std::mem;

use super::lex::{self, Token, Word};
use super::{basename, prefix_at};

/// Shells whose script, after an option word that holds `c`, is read as a command line.
const SHELLS: [&str; 3] = ["sh", "bash", "zsh"];
const TIMEOUTS: [&str; 2] = ["timeout", "gtimeout"];
/// Options of `timeout` whose value is the next word.
const TIMEOUT_VALUED: [&str; 4] = ["-k", "-s", "--kill-after", "--signal"];
/// Options of a shell whose value is the next word.
const SHELL_VALUED: [&str; 4] = ["-o", "+o", "-O", "+O"];
/// Scripts and substitutions nested deeper than this make a line unreadable, so that no line
/// can exhaust the hook's stack.
const MAX_DEPTH: u8 = 32;

/// A `while` or `until` loop whose condition or body runs `sleep`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wait {
    /// It lies in the script of a shell that `timeout` or `gtimeout` starts with a duration
    /// above 0.
    pub bounded: bool,
    /// Its condition or body runs `pgrep` or `pkill` that matches whole command lines.
    pub matches_itself: bool,
}

/// The waits of `command`, inner loops first. `None` for a line that leaves a quote, an escape,
/// a substitution or a compound command open, closes one that is not open, or nests scripts
/// deeper than [`MAX_DEPTH`].
pub(crate) fn waits(command: &str) -> Option<Vec<Wait>> {
    let mut reader = Reader::default();
    reader.script(command, false, 0)?;
    Some(reader.waits)
}

#[derive(Default)]
struct Reader {
    /// The loops open around the command being read, outermost first, across scripts.
    open: Vec<Loop>,
    waits: Vec<Wait>,
}

#[derive(Debug, Clone, Copy)]
struct Loop {
    bounded: bool,
    sleeps: bool,
    matches_itself: bool,
}

/// A compound command open in one script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    /// A `while` or `until` loop: the last of [`Reader::open`].
    Loop,
    /// A `for` or `select` loop.
    For,
    /// A `case`, whose next words are patterns while `patterns`.
    Case {
        patterns: bool,
    },
    Subshell,
}

/// The words of one simple command and the targets of its redirects.
#[derive(Default)]
struct Simple {
    words: Vec<Word>,
    targets: Vec<Word>,
}

impl Reader {
    fn script(&mut self, text: &str, bounded: bool, depth: u8) -> Option<()> {
        if depth > MAX_DEPTH {
            return None;
        }
        let mut frames = Vec::new();
        let mut simple = Simple::default();
        let mut pending_redirect = false;
        for token in lex::tokenize_whole(text)? {
            match token {
                Token::Word(word) if mem::take(&mut pending_redirect) => simple.targets.push(word),
                Token::Word(word) => simple.words.push(word),
                Token::Redirect => pending_redirect = true,
                other => {
                    pending_redirect = false;
                    let simple = mem::take(&mut simple);
                    self.end(&other, simple, &mut frames, bounded, depth)?;
                }
            }
        }
        self.end(&Token::Separator, simple, &mut frames, bounded, depth)?;
        frames.is_empty().then_some(())
    }

    /// Reads `simple`, which `token` ends.
    fn end(
        &mut self,
        token: &Token,
        simple: Simple,
        frames: &mut Vec<Frame>,
        bounded: bool,
        depth: u8,
    ) -> Option<()> {
        let patterns = matches!(frames.last(), Some(Frame::Case { patterns: true }));
        match token {
            Token::Close if patterns => {
                frames.pop();
                frames.push(Frame::Case { patterns: false });
            }
            Token::Close => {
                self.simple(simple, frames, bounded, depth)?;
                match frames.pop()? {
                    Frame::Case { patterns: true } => frames.push(Frame::Case { patterns: false }),
                    Frame::Subshell => {}
                    _ => return None,
                }
            }
            Token::Open | Token::Separator if patterns => {
                if simple.words.first().is_some_and(|word| word.text == "esac") {
                    frames.pop();
                }
            }
            Token::Open => {
                self.simple(simple, frames, bounded, depth)?;
                frames.push(Frame::Subshell);
            }
            Token::ItemEnd => {
                self.simple(simple, frames, bounded, depth)?;
                match frames.last_mut()? {
                    Frame::Case { patterns } => *patterns = true,
                    _ => return None,
                }
            }
            _ => self.simple(simple, frames, bounded, depth)?,
        }
        Some(())
    }

    /// Reads one simple command: its leading reserved words, its substitutions, and its program.
    fn simple(
        &mut self,
        simple: Simple,
        frames: &mut Vec<Frame>,
        bounded: bool,
        depth: u8,
    ) -> Option<()> {
        let mut at = 0;
        while let Some(word) = simple.words.get(at) {
            match word.text.as_str() {
                "while" | "until" => {
                    self.open.push(Loop {
                        bounded,
                        sleeps: false,
                        matches_itself: false,
                    });
                    frames.push(Frame::Loop);
                }
                "for" | "select" | "case" => {
                    let frame = if word.text == "case" {
                        Frame::Case { patterns: true }
                    } else {
                        Frame::For
                    };
                    frames.push(frame);
                    at = simple.words.len();
                    break;
                }
                "done" => match frames.pop()? {
                    Frame::Loop => self.close_loop()?,
                    Frame::For => {}
                    _ => return None,
                },
                "esac" => match frames.pop()? {
                    Frame::Case { .. } => {}
                    _ => return None,
                },
                "do" | "if" | "then" | "elif" | "else" | "fi" | "!" => {}
                _ => break,
            }
            at = at.saturating_add(1);
        }
        let inner = simple.words.iter().chain(&simple.targets);
        for substitution in inner.flat_map(|word| &word.substitutions) {
            self.script(substitution, bounded, depth.saturating_add(1))?;
        }
        let words: Vec<String> = simple
            .words
            .into_iter()
            .skip(at)
            .map(|word| word.text)
            .collect();
        self.program(&words, bounded, depth)
    }

    /// Reads the program of the simple command `words`, its reserved words removed.
    fn program(&mut self, words: &[String], bounded: bool, depth: u8) -> Option<()> {
        let (at, timed) = program_of(words);
        let Some(program) = words.get(at) else {
            return Some(());
        };
        let args = words.get(at.saturating_add(1)..).unwrap_or_default();
        let name = basename(program);
        if name == "sleep" {
            self.open.iter_mut().for_each(|open| open.sleeps = true);
        } else if (name == "pgrep" || name == "pkill") && matches_full(args) {
            self.open
                .iter_mut()
                .for_each(|open| open.matches_itself = true);
        } else if SHELLS.contains(&name.as_str())
            && let Some(script) = shell_script(args)
        {
            self.script(script, bounded || timed, depth.saturating_add(1))?;
        }
        Some(())
    }

    fn close_loop(&mut self) -> Option<()> {
        let closed = self.open.pop()?;
        if closed.sleeps {
            self.waits.push(Wait {
                bounded: closed.bounded,
                matches_itself: closed.matches_itself,
            });
        }
        Some(())
    }
}

/// The index of the program of `words` after its prefixes, and `true` when one of them is a
/// `timeout` or `gtimeout` with a duration above 0.
fn program_of(words: &[String]) -> (usize, bool) {
    let mut at = 0;
    let mut timed = false;
    loop {
        let timeout = words
            .get(at)
            .is_some_and(|word| TIMEOUTS.contains(&basename(word).as_str()));
        let step = if timeout {
            timeout_end(words, at)
        } else {
            prefix_at(words, at).map(|prefix| (prefix.next, false))
        };
        let Some((next, positive)) = step else {
            return (at, timed);
        };
        at = next;
        timed |= positive;
    }
}

/// The index after the `timeout` at word `at` of `words`, its options and its duration, and
/// `true` when that duration is above 0. `None` without a duration.
fn timeout_end(words: &[String], at: usize) -> Option<(usize, bool)> {
    let mut end = at.saturating_add(1);
    while let Some(word) = words.get(end) {
        if TIMEOUT_VALUED.contains(&word.as_str()) {
            end = end.saturating_add(2);
        } else if word.starts_with('-') {
            end = end.saturating_add(1);
        } else {
            break;
        }
    }
    let duration = words.get(end)?;
    Some((end.saturating_add(1), is_positive(duration)))
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

/// `true` when the arguments of `pgrep` or `pkill` hold `--full` or a short option word with `f`.
fn matches_full(args: &[String]) -> bool {
    args.iter().take_while(|arg| *arg != "--").any(|arg| {
        arg == "--full" || (arg.starts_with('-') && !arg.starts_with("--") && arg.contains('f'))
    })
}

/// The script a shell with the arguments `args` runs: the first word that is no option after an
/// option word that holds `c`.
fn shell_script(args: &[String]) -> Option<&str> {
    let mut command = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if SHELL_VALUED.contains(&arg.as_str()) {
            args.next();
        } else if arg.starts_with('-') && !arg.starts_with("--") {
            command |= arg.contains('c');
        } else if !arg.starts_with(['-', '+']) {
            return command.then_some(arg.as_str());
        }
    }
    None
}

#[cfg(test)]
mod tests;
