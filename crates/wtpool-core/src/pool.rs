//! The worktree pool of one repository and its commands.

mod agents;
mod background;
mod copy;
mod foreground;
mod git;
mod lock;
mod lookups;
mod maintain;
mod parking;
mod refusals;
mod slots;

pub use agents::AgentGuard;
pub(crate) use git::run_reported;

use crate::config::Config;

/// The worktree pool of one repository: a foreground worktree and background slots.
/// The changes a `<ref>` names, such as pull requests, are looked up on `forge`.
///
/// Commands that change the pool hold an exclusive lock on `wtpool.lock` in the
/// repository's common git directory, so parallel runs wait for each other.
#[derive(Debug, Clone)]
pub struct Pool<F> {
    pub(crate) config: Config,
    pub(crate) forge: F,
}

impl<F> Pool<F> {
    /// A pool that looks changes up on `forge`, a [`Forge`](crate::Forge) for `fg` and `bg`.
    #[must_use]
    pub fn new(config: Config, forge: F) -> Self {
        Self { config, forge }
    }
}
