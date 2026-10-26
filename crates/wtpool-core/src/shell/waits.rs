//! The wait loops of a shell command line: `while` and `until` loops that run `sleep`, read
//! through subshells, groups, compound commands, pipelines, substitutions and the scripts of
//! the shells of [`SHELLS`].

use super::lex;
use super::program::{SHELLS, program_start, shell_script};
use super::simple::{Boundary, Simple, simples};
use super::{MAX_DEPTH, basename};

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

impl Reader {
    fn script(&mut self, text: &str, bounded: bool, depth: u8) -> Option<()> {
        if depth > MAX_DEPTH {
            return None;
        }
        let mut frames = Vec::new();
        for (simple, end) in simples(lex::tokenize_whole(text)?) {
            self.end(end, simple, &mut frames, bounded, depth)?;
        }
        frames.is_empty().then_some(())
    }

    /// Reads `simple`, ended by `end`.
    fn end(
        &mut self,
        end: Boundary,
        simple: Simple,
        frames: &mut Vec<Frame>,
        bounded: bool,
        depth: u8,
    ) -> Option<()> {
        let patterns = matches!(frames.last(), Some(Frame::Case { patterns: true }));
        match end {
            Boundary::Close if patterns => {
                frames.pop();
                frames.push(Frame::Case { patterns: false });
            }
            Boundary::Close => {
                self.simple(simple, frames, bounded, depth)?;
                match frames.pop()? {
                    Frame::Case { patterns: true } => frames.push(Frame::Case { patterns: false }),
                    Frame::Subshell => {}
                    _ => return None,
                }
            }
            Boundary::Open | Boundary::Separator if patterns => {
                if simple.words.first().is_some_and(|word| word.text == "esac") {
                    frames.pop();
                }
            }
            Boundary::Open => {
                self.simple(simple, frames, bounded, depth)?;
                frames.push(Frame::Subshell);
            }
            Boundary::ItemEnd => {
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
        for substitution in &simple.substitutions {
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
        let start = program_start(words);
        let Some(program) = words.get(start.at) else {
            return Some(());
        };
        let args = words.get(start.at.saturating_add(1)..).unwrap_or_default();
        let name = basename(program);
        if name == "sleep" {
            self.open.iter_mut().for_each(|open| open.sleeps = true);
        } else if (name == "pgrep" || name == "pkill") && matches_full(args) {
            self.open
                .iter_mut()
                .for_each(|open| open.matches_itself = true);
        } else if SHELLS.contains(&name.as_str())
            && let Some(script) = shell_script(args).and_then(|at| args.get(at))
        {
            self.script(script, bounded || start.timed, depth.saturating_add(1))?;
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

/// `true` when the arguments of `pgrep` or `pkill` hold `--full` or a short option word with `f`.
fn matches_full(args: &[String]) -> bool {
    args.iter().take_while(|arg| *arg != "--").any(|arg| {
        arg == "--full" || (arg.starts_with('-') && !arg.starts_with("--") && arg.contains('f'))
    })
}

#[cfg(test)]
mod tests;
