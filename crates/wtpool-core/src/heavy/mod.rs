//! Heavy commands: the [`Pattern`]s that name them, the [`CommandWords`] of a command that a
//! pattern matches, the [`HeavyList`] of a machine and a repository, the [`RepositoryPatterns`]
//! of a repository's `wtpool.toml`, and the [`Suggestions`] a repository's files make.

mod list;
mod pattern;
mod repository;
mod scripts;
mod suggest;
mod words;

pub use list::{HeavyList, HeavyPattern, PatternSource};
pub use pattern::{InvalidPattern, Pattern, PatternFault};
pub use repository::{RepositoryHeavy, RepositoryPatterns};
pub use suggest::{Suggestion, SuggestionFault, Suggestions};
pub use words::{CommandWords, ShellCommand, shell_commands};
