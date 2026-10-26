//! Heavy commands: the [`Pattern`]s that name them, the [`CommandWords`] of a command that a
//! pattern matches, the [`HeavyList`] of a machine and a repository, and the [`Suggestions`] a
//! repository's files make.

mod list;
mod pattern;
mod scripts;
mod suggest;
mod words;

pub use list::{HeavyList, HeavyPattern, PatternSource, Suggestion};
pub use pattern::{InvalidPattern, Pattern, PatternFault};
pub use suggest::{Suggestions, suggested_json};
pub use words::{CommandWords, ShellCommand, shell_commands};
