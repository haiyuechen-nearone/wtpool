//! A reader of shell command lines: the path words, `cd` targets and summary of a command
//! ([`parse`]), its simple commands ([`simple_commands`]), the start of a simple command's
//! program after its prefixes ([`program_start`]), and its wait loops ([`waits::waits`]).

mod command;
mod lex;
mod program;
mod simple;
pub(crate) mod waits;

pub use command::{Command, parse};
pub(crate) use program::{SHELLS, program_start, summary_of};
pub(crate) use simple::simple_commands;

/// The deepest nesting of `sh -c` and substitutions read for path words. Deeper ones are left
/// out and the rest of the line still counts.
const MAX_PATH_DEPTH: u8 = 4;
/// Scripts and substitutions nested deeper than this make a line unreadable for its wait loops,
/// so that no line can exhaust the hook's stack.
const MAX_DEPTH: u8 = 32;

fn basename(word: &str) -> String {
    word.rsplit('/').next().unwrap_or(word).to_owned()
}

fn has_substitution(word: &str) -> bool {
    word.contains("$(") || word.contains('`') || word.contains("<(") || word.contains(">(")
}
