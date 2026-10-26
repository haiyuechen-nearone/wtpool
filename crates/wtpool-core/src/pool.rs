//! The worktree pool of one repository and its commands.

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

pub(crate) use git::run_reported;

use crate::config::Config;
use crate::github::GhCli;

/// The worktree pool of one repository: a foreground worktree and background slots.
/// Pull requests are looked up through `prs`.
///
/// Commands that change the pool hold an exclusive lock on `wtpool.lock` in the
/// repository's common git directory, so parallel runs wait for each other.
#[derive(Debug, Clone)]
pub struct Pool<V = GhCli> {
    pub(crate) config: Config,
    pub(crate) prs: V,
}

impl Pool {
    /// A pool that looks pull requests up with the `gh` command line tool.
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self::with_view_pr(config, GhCli)
    }
}

impl<V> Pool<V> {
    /// A pool that looks pull requests up through `prs`.
    #[must_use]
    pub fn with_view_pr(config: Config, prs: V) -> Self {
        Self { config, prs }
    }
}
