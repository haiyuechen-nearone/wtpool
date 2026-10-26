//! The git facts of one worktree, read through read only git, and the rules that turn them
//! into `free`, `reasons` and `activity`.

use super::document::{Activity, GitStatus, OperationName, Reason};
use crate::git::Git;
use crate::resolve::{self, Divergence};
use crate::worktree::{Head, Worktree};
use crate::worktree_state::{Operation, Role, Uncommitted, Unreferenced, Upstream};

/// The git facts of a worktree whose folder exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub local: LocalFacts,
    pub refs: RefFacts,
}

/// The facts that only the worktree's own `HEAD`, index and operation files change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalFacts {
    pub dirty: Uncommitted,
    pub operation: Operation,
}

/// The facts that a ref anywhere in the repository may change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefFacts {
    pub unreferenced: Unreferenced,
    pub upstream: Option<Tracking>,
}

/// The upstream of a branch and the branch's ahead and behind counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tracking {
    pub name: String,
    pub counts: Upstream,
}

impl Facts {
    /// Every fact of `worktree`. Each git command runs in the worktree itself.
    pub fn read(worktree: &Worktree) -> Self {
        Self {
            local: LocalFacts::read(worktree),
            refs: RefFacts::read(worktree),
        }
    }
}

impl LocalFacts {
    pub fn read(worktree: &Worktree) -> Self {
        Self {
            dirty: worktree.uncommitted().unwrap_or(Uncommitted::Unknown),
            operation: worktree.operation().unwrap_or(Operation::Unknown),
        }
    }
}

impl RefFacts {
    pub fn read(worktree: &Worktree) -> Self {
        let git = worktree.git();
        Self {
            unreferenced: worktree.unreferenced(&git).unwrap_or(Unreferenced::Unknown),
            upstream: match worktree.head() {
                Head::Branch(_) => tracking(&git),
                Head::Detached | Head::Unknown => None,
            },
        }
    }
}

/// The upstream of the branch checked out in `git`'s worktree, `None` without one.
fn tracking(git: &Git) -> Option<Tracking> {
    let ran = git.run(["rev-parse", "--abbrev-ref", "@{upstream}"]).ok()?;
    let name = ran.line().filter(|name| !name.is_empty())?.to_owned();
    let counts = match resolve::left_right(git, &format!("HEAD...{name}")) {
        Ok(Some(Divergence { ahead, behind })) => Upstream::Counted { ahead, behind },
        Ok(None) | Err(_) => Upstream::Unknown,
    };
    Some(Tracking { name, counts })
}

/// The `git` object of the document. A worktree without facts has a missing folder.
#[must_use]
pub fn git_status(head: &Head, facts: Option<&Facts>) -> GitStatus {
    let Some(Facts { local, refs }) = facts else {
        return GitStatus {
            unknown: true,
            ..GitStatus::default()
        };
    };
    let dirty = match local.dirty {
        Uncommitted::Files(count) => Some(count),
        Uncommitted::Unknown => None,
    };
    let unreferenced = match refs.unreferenced {
        Unreferenced::Commits(count) => Some(count),
        Unreferenced::Unknown => None,
    };
    let counts = refs.upstream.as_ref().map(|tracking| tracking.counts);
    let (ahead, behind) = match counts {
        Some(Upstream::Counted { ahead, behind }) => (Some(ahead), Some(behind)),
        Some(Upstream::Unknown) | None => (None, None),
    };
    let unknown = dirty.is_none()
        || unreferenced.is_none()
        || local.operation == Operation::Unknown
        || counts == Some(Upstream::Unknown)
        || *head == Head::Unknown;
    GitStatus {
        dirty: dirty.map(count),
        ahead: ahead.map(count),
        behind: behind.map(count),
        unreferenced: unreferenced.map(count),
        operation: operation_name(local.operation),
        unknown,
    }
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn operation_name(operation: Operation) -> Option<OperationName> {
    match operation {
        Operation::Idle | Operation::Unknown => None,
        Operation::Rebase => Some(OperationName::Rebase),
        Operation::Am => Some(OperationName::Am),
        Operation::Merge => Some(OperationName::Merge),
        Operation::CherryPick => Some(OperationName::CherryPick),
        Operation::Revert => Some(OperationName::Revert),
        Operation::Bisect => Some(OperationName::Bisect),
    }
}

/// The causes that keep `wtpool bg` from taking this worktree, in [`Reason`] order. Empty means
/// free.
#[must_use]
pub fn reasons(role: Role, missing: bool, head: &Head, git: &GitStatus) -> Vec<Reason> {
    let mut reasons = Vec::new();
    match role {
        Role::Foreground => reasons.push(Reason::Foreground),
        Role::Unmanaged => reasons.push(Reason::Unmanaged),
        Role::Slot if matches!(head, Head::Branch(_)) => reasons.push(Reason::Branch),
        Role::Slot => {}
    }
    if missing {
        reasons.push(Reason::Missing);
        return reasons;
    }
    if git.dirty.is_some_and(|count| count > 0) {
        reasons.push(Reason::Dirty);
    }
    if git.unreferenced.is_some_and(|count| count > 0) {
        reasons.push(Reason::Unreferenced);
    }
    if git.operation.is_some() {
        reasons.push(Reason::Operation);
    }
    if git.unknown {
        reasons.push(Reason::Unknown);
    }
    reasons
}

/// The states of a worktree's actors and builds that [`activity`] reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Busy {
    pub waiting: bool,
    pub working: bool,
    pub building: bool,
    /// An idle or quiet actor. A done one counts as nothing.
    pub idle: bool,
}

/// The first activity that applies, else the availability.
#[must_use]
pub fn activity(
    role: Role,
    missing: bool,
    head: &Head,
    git: &GitStatus,
    free: bool,
    busy: Busy,
) -> Activity {
    if missing {
        Activity::Missing
    } else if busy.waiting {
        Activity::Waiting
    } else if busy.working {
        Activity::Working
    } else if busy.building {
        Activity::Building
    } else if busy.idle {
        Activity::Idle
    } else if role == Role::Foreground {
        Activity::Foreground
    } else if free {
        Activity::Free
    } else if role == Role::Slot && *head == Head::Detached {
        Activity::NotReusable
    } else if git.unknown {
        Activity::Unknown
    } else {
        Activity::Unused
    }
}

#[cfg(test)]
mod tests;
