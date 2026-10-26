//! [`Forge`]: a git hosting service, such as GitHub, as `fg`, `bg` and `wtpool status` use it: the
//! changes a `<ref>` can name, such as pull requests, their heads, and the names of repositories.

use std::path::Path;

use crate::names::{BranchName, ObjectId, RemoteName};
use crate::report::Report;
use crate::resolve::Unresolved;

/// A git hosting service whose changes `fg` and `bg` take as a `<ref>`.
pub trait Forge {
    /// A change as a `<ref>` names it.
    type Change;

    /// The change `reference` names, `None` when it names none, so it is a branch name.
    fn change(&self, reference: &str) -> Option<Self::Change>;

    /// The `<ref>` forms [`Forge::change`] reads, as an error lists them before a branch name,
    /// such as `a PR number, a github.com PR URL`.
    fn change_forms(&self) -> &'static str;

    /// The head of `change` in the repository of `remote`. Without one to use, reports why and
    /// returns [`Unresolved::Quiet`].
    fn head(
        &self,
        change: Self::Change,
        remote: Remote<'_>,
        report: &mut dyn Report,
    ) -> Result<ChangeHead, Unresolved>;

    /// The repository `url` names on this forge, `None` for any other URL.
    fn repository(&self, url: &str) -> Option<String>;
}

/// The remote [`Forge::head`] looks a change up in.
#[derive(Debug, Clone, Copy)]
pub struct Remote<'a> {
    pub name: &'a RemoteName,
    /// `None` when the repository has no remote of that name.
    pub url: Option<&'a str>,
    /// The foreground worktree, whose repository has the remote.
    pub foreground: &'a Path,
}

/// The head of a change and where the remote keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeHead {
    /// The change as messages name it, such as `PR #42`.
    pub name: String,
    /// The local branch the change is checked out on.
    pub branch: BranchName,
    pub head: ObjectId,
    /// What a refresh of the local branch fetches.
    pub fetch: Fetch,
    /// A new local branch comes from here when the remote has no branch of its name.
    pub change_ref: ChangeRef,
}

/// What a refresh of the local branch of a change fetches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetch {
    /// The remote's branch of the same name.
    Branch,
    /// [`ChangeHead::change_ref`], as the head is on a branch of another repository.
    ChangeRef,
}

/// A ref of the remote that holds the head of a change, such as `refs/pull/42/head`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeRef {
    pub refspec: String,
    /// The warning for a local branch made from this ref, which therefore tracks nothing.
    pub note: String,
}
