//! Resolving a `<ref>` to the local branch it names ([`Resolver::plan`], with lookups only), and
//! fetching, creating or syncing that branch once the plan is checked ([`Resolver::apply`]).

mod apply;
mod divergence;
mod git;
mod guard;
mod plan;

pub(crate) use git::run_reported;
pub use plan::Unresolved;

use crate::config::Config;
use crate::error::Error;
use crate::git::Git;
use crate::names::BranchName;
use crate::report::Discard;
use crate::worktree::Worktree;

/// The repository of a pool's foreground, and the forge whose changes a `<ref>` may name.
#[derive(Debug)]
pub(crate) struct Resolver<'a, F> {
    /// git in the foreground worktree.
    git: Git,
    config: &'a Config,
    forge: &'a F,
}

impl<'a, F> Resolver<'a, F> {
    pub(crate) fn new(config: &'a Config, forge: &'a F) -> Self {
        Self {
            git: Git::at(&config.foreground.value),
            config,
            forge,
        }
    }

    pub(crate) fn worktrees(&self) -> Result<Vec<Worktree>, Error> {
        Worktree::list(&self.config.foreground.value, &mut Discard)
    }

    /// The worktree whose directory exists and whose `HEAD` is `branch`.
    pub(crate) fn holder_of(&self, branch: &BranchName) -> Result<Option<Worktree>, Error> {
        Ok(self
            .worktrees()?
            .into_iter()
            .find(|worktree| worktree.holds_branch(branch) && !worktree.is_gone()))
    }
}
