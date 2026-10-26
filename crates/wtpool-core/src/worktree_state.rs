//! The role of each worktree and the work it holds, as `wtpool ls` lists them.

use crate::names::BranchName;

/// One line of `wtpool ls`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub worktree: String,
    /// `None` when the worktree is detached, git recorded no branch for it, or its
    /// branch is not a valid [`BranchName`].
    pub branch: Option<BranchName>,
    pub role: Role,
    pub state: State,
}

/// The relation of a worktree to the pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Foreground,
    Slot,
    Unmanaged,
}

/// The work in a worktree that `wtpool` must not lose or interrupt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct State {
    pub dirty: Uncommitted,
    pub upstream: Upstream,
    pub unreferenced: Unreferenced,
    pub operation: Operation,
}

/// The git facts of one worktree, read once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorktreeFacts {
    /// `None` without an upstream.
    pub upstream: Option<Tracking>,
    pub dirty: Uncommitted,
    pub unreferenced: Unreferenced,
    pub operation: Operation,
}

impl WorktreeFacts {
    /// The facts `wtpool ls` shows. A branch without an upstream counts as level with it.
    pub fn state(&self) -> State {
        State {
            dirty: self.dirty,
            upstream: self
                .upstream
                .as_ref()
                .map_or_else(Upstream::default, |tracking| tracking.counts),
            unreferenced: self.unreferenced,
            operation: self.operation,
        }
    }
}

/// The upstream of a branch, as `<branch>@{upstream}` names it, and the branch's ahead and behind
/// counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tracking {
    pub name: String,
    pub counts: Upstream,
}

/// The uncommitted files of a worktree, as `git status --porcelain` counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uncommitted {
    Files(usize),
    /// `git status` failed, so the worktree may hold uncommitted work.
    Unknown,
}

impl Default for Uncommitted {
    fn default() -> Self {
        Self::Files(0)
    }
}

/// The commits ahead of and behind the upstream. A branch without an upstream
/// counts as level with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upstream {
    Counted {
        ahead: usize,
        behind: usize,
    },
    /// `git rev-list` failed, so the branch may lag or have diverged.
    Unknown,
}

impl Default for Upstream {
    fn default() -> Self {
        Self::Counted {
            ahead: 0,
            behind: 0,
        }
    }
}

/// The commits on `HEAD` that no local branch, remote tracking ref, tag or `refs/wtpool-backup/`
/// ref contains. Removing the worktree deletes its `HEAD` reflog, the last record of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unreferenced {
    Commits(usize),
    /// git could not count them, so the worktree may hold such commits.
    Unknown,
}

impl Default for Unreferenced {
    fn default() -> Self {
        Self::Commits(0)
    }
}

/// A git operation that stopped half way in a worktree and waits for the user.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Operation {
    #[default]
    Idle,
    Rebase,
    Am,
    Merge,
    CherryPick,
    Revert,
    Bisect,
    /// git could not tell, so an operation may be in progress.
    Unknown,
}

impl Operation {
    /// The `wtpool ls` label, `None` when no operation is in progress.
    #[must_use]
    pub fn label(self) -> Option<&'static str> {
        match self {
            Self::Idle => None,
            Self::Rebase => Some("rebase"),
            Self::Am => Some("am"),
            Self::Merge => Some("merge"),
            Self::CherryPick => Some("cherry-pick"),
            Self::Revert => Some("revert"),
            Self::Bisect => Some("bisect"),
            Self::Unknown => Some("operation:?"),
        }
    }
}
