//! The state of a worktree that `wtpool` must not lose or interrupt, and the refusals it leads
//! to before a worktree is handed out, released, detached or removed.

use std::path::Path;

use crate::error::{Error, Refusal};
use crate::git::Git;
use crate::names::{BranchName, ObjectId};
use crate::worktree::{self, Worktree};
use crate::worktree_state::{Operation, Role, Uncommitted, Unreferenced};

/// The work in a worktree, read once before a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorktreeState {
    pub(super) dirty: Uncommitted,
    pub(super) operation: Operation,
    pub(super) unreferenced: Unreferenced,
    pub(super) head_commit: Option<ObjectId>,
}

impl WorktreeState {
    pub(super) fn read(worktree: &Worktree, repo: &Git) -> Result<Self, Error> {
        Ok(Self {
            dirty: worktree.uncommitted()?,
            operation: worktree.operation()?,
            unreferenced: worktree.unreferenced(repo)?,
            head_commit: worktree.head_commit().cloned(),
        })
    }
}

/// Refuses the worktree `name` for any work at risk in it, such as in a detached slot that `bg`
/// would reuse.
pub(super) fn refusal(name: &str, state: &WorktreeState) -> Result<(), Refusal> {
    detach_refusal(name, state)?;
    refuse_unreferenced(name, state.unreferenced, state.head_commit.as_ref())
}

/// Refuses the worktree `name` that `free` would detach. Its commits on no branch stay in its
/// `HEAD` reflog, so they do not count.
pub(super) fn detach_refusal(name: &str, state: &WorktreeState) -> Result<(), Refusal> {
    refuse_dirty(name, state.dirty, |count| Refusal::WorktreeDirty {
        worktree: name.to_owned(),
        count,
    })?;
    refuse_operation(name, state.operation)
}

/// Refuses the worktree `name` that `rm` would remove. A worktree whose directory is `gone` holds
/// no files to lose, but its `HEAD` reflog may be the last record of commits on no branch.
pub(super) fn removal_refusal(
    name: &str,
    gone: bool,
    state: &WorktreeState,
) -> Result<(), Refusal> {
    if gone {
        refuse_unreferenced(name, state.unreferenced, state.head_commit.as_ref())
    } else {
        refusal(name, state)
    }
}

/// Refuses the foreground `name` that `fg` would move to the branch of `reference`.
pub(super) fn foreground_refusal(
    reference: &str,
    name: &str,
    state: &WorktreeState,
) -> Result<(), Refusal> {
    refuse_dirty(name, state.dirty, |count| Refusal::ForegroundDirty {
        count,
        reference: reference.to_owned(),
    })?;
    refuse_operation(name, state.operation)?;
    refuse_unreferenced(name, state.unreferenced, state.head_commit.as_ref())
}

/// Refuses the worktree at `holder`, other than the foreground, that `fg` would release `branch`
/// from. `wtpool` never detaches an unmanaged worktree.
pub(super) fn release_refusal(
    branch: &BranchName,
    holder: &Path,
    role: Role,
    state: &WorktreeState,
) -> Result<(), Refusal> {
    if role == Role::Unmanaged {
        return Err(held_by_unmanaged(branch, holder));
    }
    let name = worktree::basename(holder);
    refuse_dirty(&name, state.dirty, |count| Refusal::HolderDirty {
        branch: branch.to_string(),
        holder: name.clone(),
        count,
    })?;
    refuse_operation(&name, state.operation)
}

/// Refuses the worktree at `holder` that `bg` would hand out for `branch`: only a slot may be
/// handed out, as the caller would otherwise work where the user does.
pub(super) fn outside_slots_refusal(
    branch: &BranchName,
    holder: &Path,
    role: Role,
) -> Result<(), Refusal> {
    match role {
        Role::Slot => Ok(()),
        Role::Foreground => Err(Refusal::HeldByForeground {
            branch: branch.to_string(),
            path: holder.to_path_buf(),
        }),
        Role::Unmanaged => Err(held_by_unmanaged(branch, holder)),
    }
}

fn held_by_unmanaged(branch: &BranchName, holder: &Path) -> Refusal {
    Refusal::HeldByUnmanaged {
        branch: branch.to_string(),
        worktree: worktree::basename(holder),
        path: holder.to_path_buf(),
    }
}

/// Refuses uncommitted files in the worktree `name` as `files` words their count, and a status
/// git cannot read.
fn refuse_dirty(
    name: &str,
    dirty: Uncommitted,
    files: impl FnOnce(usize) -> Refusal,
) -> Result<(), Refusal> {
    match dirty {
        Uncommitted::Files(0) => Ok(()),
        Uncommitted::Files(count) => Err(files(count)),
        Uncommitted::Unknown => Err(Refusal::StatusUnknown(name.to_owned())),
    }
}

fn refuse_operation(name: &str, operation: Operation) -> Result<(), Refusal> {
    if operation == Operation::Unknown {
        return Err(Refusal::OperationUnknown(name.to_owned()));
    }
    match operation.label() {
        None => Ok(()),
        Some(label) => Err(Refusal::OperationInProgress {
            worktree: name.to_owned(),
            operation: label,
        }),
    }
}

/// Refuses commits that only the `HEAD` reflog of `name` records, `head` naming the newest.
fn refuse_unreferenced(
    name: &str,
    unreferenced: Unreferenced,
    head: Option<&ObjectId>,
) -> Result<(), Refusal> {
    match (unreferenced, head) {
        (Unreferenced::Commits(0), _) => Ok(()),
        (Unreferenced::Commits(count), Some(head)) => Err(Refusal::Unreferenced {
            worktree: name.to_owned(),
            count,
            head: head.to_string(),
        }),
        (Unreferenced::Commits(_), None) | (Unreferenced::Unknown, _) => {
            Err(Refusal::UnreferencedUnknown(name.to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const HEAD: &str = "1111111111111111111111111111111111111111";
    const CLEAN: Uncommitted = Uncommitted::Files(0);
    const NONE: Unreferenced = Unreferenced::Commits(0);

    fn state(
        dirty: Uncommitted,
        operation: Operation,
        unreferenced: Unreferenced,
    ) -> WorktreeState {
        WorktreeState {
            dirty,
            operation,
            unreferenced,
            head_commit: ObjectId::new(HEAD),
        }
    }

    fn message(refused: Result<(), Refusal>) -> Option<String> {
        refused.err().map(|refusal| refusal.to_string())
    }

    #[rstest]
    #[case::clean(state(CLEAN, Operation::Idle, NONE), None)]
    #[case::uncommitted_files(
        state(Uncommitted::Files(2), Operation::Rebase, Unreferenced::Unknown),
        Some("app-wt1 has 2 uncommitted file(s)")
    )]
    #[case::unreadable_status(
        state(Uncommitted::Unknown, Operation::Idle, NONE),
        Some("could not read the status of app-wt1; not touching it")
    )]
    #[case::rebase_in_progress(
        state(CLEAN, Operation::Rebase, Unreferenced::Commits(1)),
        Some("app-wt1 is in the middle of a rebase; finish or abort it first")
    )]
    #[case::unreadable_operation_state(
        state(CLEAN, Operation::Unknown, NONE),
        Some("could not tell whether an operation is in progress in app-wt1; not touching it")
    )]
    #[case::commits_on_no_branch(
        state(CLEAN, Operation::Idle, Unreferenced::Commits(1)),
        Some(
            "app-wt1 holds 1 commit(s) on no branch; to keep them, run: git branch <name> 1111111111111111111111111111111111111111"
        )
    )]
    #[case::uncounted_commits(
        state(CLEAN, Operation::Idle, Unreferenced::Unknown),
        Some("could not tell whether app-wt1 holds commits on no branch; not touching it")
    )]
    fn refusal__should_refuse_the_first_work_at_risk(
        #[case] state: WorktreeState,
        #[case] expected: Option<&str>,
    ) {
        // When
        let refused = refusal("app-wt1", &state);

        // Then
        assert_eq!(message(refused).as_deref(), expected);
    }

    #[test]
    fn refusal__should_refuse_commits_it_cannot_name() {
        // Given
        let state = WorktreeState {
            head_commit: None,
            ..state(CLEAN, Operation::Idle, Unreferenced::Commits(1))
        };

        // When
        let refused = refusal("app-wt1", &state);

        // Then
        assert_eq!(
            message(refused).as_deref(),
            Some("could not tell whether app-wt1 holds commits on no branch; not touching it")
        );
    }

    #[rstest]
    #[case::commits_on_no_branch(state(CLEAN, Operation::Idle, Unreferenced::Commits(1)), None)]
    #[case::merge_in_progress(
        state(CLEAN, Operation::Merge, NONE),
        Some("app-wt1 is in the middle of a merge; finish or abort it first")
    )]
    fn detach_refusal__should_let_commits_on_no_branch_pass(
        #[case] state: WorktreeState,
        #[case] expected: Option<&str>,
    ) {
        // When
        let refused = detach_refusal("app-wt1", &state);

        // Then
        assert_eq!(message(refused).as_deref(), expected);
    }

    #[rstest]
    #[case::live_worktree_with_files(
        false,
        state(Uncommitted::Files(1), Operation::Idle, NONE),
        Some("app-wt1 has 1 uncommitted file(s)")
    )]
    #[case::gone_worktree_git_cannot_read(
        true,
        state(Uncommitted::Unknown, Operation::Unknown, NONE),
        None
    )]
    #[case::gone_worktree_with_commits_on_no_branch(
        true,
        state(Uncommitted::Unknown, Operation::Unknown, Unreferenced::Commits(2)),
        Some(
            "app-wt1 holds 2 commit(s) on no branch; to keep them, run: git branch <name> 1111111111111111111111111111111111111111"
        )
    )]
    fn removal_refusal__should_weigh_only_commits_on_no_branch_once_the_directory_is_gone(
        #[case] gone: bool,
        #[case] state: WorktreeState,
        #[case] expected: Option<&str>,
    ) {
        // When
        let refused = removal_refusal("app-wt1", gone, &state);

        // Then
        assert_eq!(message(refused).as_deref(), expected);
    }

    #[rstest]
    #[case::clean(state(CLEAN, Operation::Idle, NONE), None)]
    #[case::uncommitted_file(
        state(Uncommitted::Files(1), Operation::Idle, NONE),
        Some("foreground has 1 uncommitted file(s); commit, stash, or use: wtpool bg feature")
    )]
    #[case::unreadable_status(
        state(Uncommitted::Unknown, Operation::Idle, NONE),
        Some("could not read the status of app; not touching it")
    )]
    #[case::bisect_in_progress(
        state(CLEAN, Operation::Bisect, NONE),
        Some("app is in the middle of a bisect; finish or abort it first")
    )]
    #[case::commits_on_no_branch(
        state(CLEAN, Operation::Idle, Unreferenced::Commits(3)),
        Some(
            "app holds 3 commit(s) on no branch; to keep them, run: git branch <name> 1111111111111111111111111111111111111111"
        )
    )]
    fn foreground_refusal__should_keep_the_foreground_where_it_holds_work(
        #[case] state: WorktreeState,
        #[case] expected: Option<&str>,
    ) {
        // When
        let refused = foreground_refusal("feature", "app", &state);

        // Then
        assert_eq!(message(refused).as_deref(), expected);
    }

    #[rstest]
    #[case::clean_slot(Role::Slot, state(CLEAN, Operation::Idle, NONE), None)]
    #[case::slot_with_files(
        Role::Slot,
        state(Uncommitted::Files(1), Operation::Idle, NONE),
        Some(
            "feature is checked out in app-wt1 with 1 uncommitted file(s); commit or stash them there first"
        )
    )]
    #[case::slot_git_cannot_read(
        Role::Slot,
        state(Uncommitted::Unknown, Operation::Idle, NONE),
        Some("could not read the status of app-wt1; not touching it")
    )]
    #[case::slot_merging(
        Role::Slot,
        state(CLEAN, Operation::Merge, NONE),
        Some("app-wt1 is in the middle of a merge; finish or abort it first")
    )]
    #[case::clean_unmanaged_worktree(
        Role::Unmanaged,
        state(CLEAN, Operation::Idle, NONE),
        Some(
            "feature is checked out in the unmanaged worktree app-wt1 (/w/app-wt1), which is not a slot; work on it there, or first release it with 'wtpool free feature'"
        )
    )]
    #[case::unmanaged_worktree_with_files(
        Role::Unmanaged,
        state(Uncommitted::Files(1), Operation::Idle, NONE),
        Some(
            "feature is checked out in the unmanaged worktree app-wt1 (/w/app-wt1), which is not a slot; work on it there, or first release it with 'wtpool free feature'"
        )
    )]
    fn release_refusal__should_release_only_a_slot_that_holds_no_work(
        #[case] role: Role,
        #[case] state: WorktreeState,
        #[case] expected: Option<&str>,
    ) {
        // Given
        let branch = BranchName::try_new("feature").unwrap();

        // When
        let refused = release_refusal(&branch, Path::new("/w/app-wt1"), role, &state);

        // Then
        assert_eq!(message(refused).as_deref(), expected);
    }

    #[rstest]
    #[case::slot(Role::Slot, None)]
    #[case::foreground(
        Role::Foreground,
        Some(
            "feature is checked out in the foreground (/w/review); work on it there, or first move the foreground to another branch with 'wtpool fg <other-branch>'"
        )
    )]
    #[case::unmanaged_worktree(
        Role::Unmanaged,
        Some(
            "feature is checked out in the unmanaged worktree review (/w/review), which is not a slot; work on it there, or first release it with 'wtpool free feature'"
        )
    )]
    fn outside_slots_refusal__should_hand_out_only_a_slot(
        #[case] role: Role,
        #[case] expected: Option<&str>,
    ) {
        // Given
        let branch = BranchName::try_new("feature").unwrap();

        // When
        let refused = outside_slots_refusal(&branch, Path::new("/w/review"), role);

        // Then
        assert_eq!(message(refused).as_deref(), expected);
    }
}
