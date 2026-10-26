//! Comparing a local branch with a remote head, and the decisions the comparison drives: the
//! move `--refresh` or `--reset` makes, and the warning about a stale branch.

pub(crate) use crate::git::{Divergence, left_right};
use crate::names::{BranchName, ObjectId};
use crate::refs::SyncMode;
use crate::worktree_state::{Role, Uncommitted};

/// The worktree that has the branch checked out, as [`plan_sync`] weighs it. `worktree` is
/// handed back in the [`SyncStep`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Holder<T> {
    pub(super) worktree: T,
    pub(super) role: Role,
    pub(super) dirty: Uncommitted,
}

/// The move [`plan_sync`] chooses for a local branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SyncStep<T> {
    /// git could not count the commits between the branch and the fetched commit.
    Incomparable,
    UpToDate,
    /// Without [`SyncMode::Reset`], local only commits keep the branch where it is.
    Diverged {
        ahead: usize,
        behind: usize,
    },
    /// `wtpool` never moves the files of an unmanaged worktree.
    HeldByUnmanaged(T),
    /// `git status` failed in the holder, so it may hold uncommitted work.
    HolderUnknown(T),
    HolderDirty(T),
    FastForward {
        behind: usize,
        holder: Option<T>,
    },
    /// Moves a diverged branch, after a backup of its tip.
    ResetWithBackup {
        ahead: usize,
        holder: Option<T>,
    },
}

/// Chooses the move to a fetched commit for a local branch that compares with it as
/// `divergence`.
pub(super) fn plan_sync<T>(
    divergence: Option<Divergence>,
    mode: SyncMode,
    holder: Option<Holder<T>>,
) -> SyncStep<T> {
    let Some(Divergence { ahead, behind }) = divergence else {
        return SyncStep::Incomparable;
    };
    if behind == 0 {
        return SyncStep::UpToDate;
    }
    if ahead != 0 && mode != SyncMode::Reset {
        return SyncStep::Diverged { ahead, behind };
    }
    let holder = match holder {
        None => None,
        Some(Holder {
            worktree,
            role: Role::Unmanaged,
            ..
        }) => return SyncStep::HeldByUnmanaged(worktree),
        Some(Holder {
            worktree,
            dirty: Uncommitted::Unknown,
            ..
        }) => return SyncStep::HolderUnknown(worktree),
        Some(Holder {
            worktree,
            dirty: Uncommitted::Files(0),
            ..
        }) => Some(worktree),
        Some(Holder { worktree, .. }) => return SyncStep::HolderDirty(worktree),
    };
    if ahead == 0 {
        SyncStep::FastForward { behind, holder }
    } else {
        SyncStep::ResetWithBackup { ahead, holder }
    }
}

/// A local branch tip that differs from the commit it should match, as [`stale_warning`]
/// weighs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Staleness<'a> {
    pub(super) local: &'a ObjectId,
    pub(super) target: &'a ObjectId,
    /// The object store has `target`.
    pub(super) fetched: bool,
    /// `None` when git could not count, or `target` is not fetched.
    pub(super) divergence: Option<Divergence>,
}

/// The warnings about `branch` when it lags or has diverged from the commit named `label`.
/// Local only commits stay silent.
pub(super) fn stale_warning(
    branch: &BranchName,
    label: &str,
    facts: &Staleness<'_>,
) -> Vec<String> {
    if !facts.fetched {
        return vec![format!(
            "{branch} is at {}, but {label} is {}, not fetched yet; rerun with --refresh",
            facts.local.abbreviated(),
            facts.target.abbreviated()
        )];
    }
    let Some(Divergence { ahead, behind }) = facts.divergence else {
        return vec![format!("could not compare {branch} with {label}")];
    };
    if behind == 0 {
        Vec::new()
    } else if ahead == 0 {
        vec![format!(
            "{branch} is {behind} commit(s) behind {label}; rerun with --refresh to fast-forward it"
        )]
    } else {
        vec![
            format!(
                "{branch} has diverged from {label} ({ahead} local only, {behind} remote only commit(s)); the remote was probably rebased"
            ),
            format!(
                "if the local only commits are old copies, rerun with --reset to move {branch} to {label}"
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const LOCAL: &str = "1111111111111111111111111111111111111111";
    const TARGET: &str = "2222222222222222222222222222222222222222";

    fn counted(ahead: usize, behind: usize) -> Option<Divergence> {
        Some(Divergence { ahead, behind })
    }

    fn holder(role: Role, dirty: Uncommitted) -> Option<Holder<&'static str>> {
        Some(Holder {
            worktree: "app-wt1",
            role,
            dirty,
        })
    }

    const CLEAN: Uncommitted = Uncommitted::Files(0);

    #[rstest]
    #[case::incomparable(None, SyncMode::Reset, None, SyncStep::Incomparable)]
    #[case::up_to_date(counted(3, 0), SyncMode::Refresh, None, SyncStep::UpToDate)]
    #[case::diverged_on_refresh(
        counted(1, 2),
        SyncMode::Refresh,
        None,
        SyncStep::Diverged { ahead: 1, behind: 2 }
    )]
    #[case::behind_without_a_holder(
        counted(0, 2),
        SyncMode::Refresh,
        None,
        SyncStep::FastForward { behind: 2, holder: None }
    )]
    #[case::behind_in_a_clean_slot(
        counted(0, 2),
        SyncMode::Reset,
        holder(Role::Slot, CLEAN),
        SyncStep::FastForward { behind: 2, holder: Some("app-wt1") }
    )]
    #[case::diverged_on_reset(
        counted(1, 2),
        SyncMode::Reset,
        holder(Role::Foreground, CLEAN),
        SyncStep::ResetWithBackup { ahead: 1, holder: Some("app-wt1") }
    )]
    #[case::dirty_holder(
        counted(0, 2),
        SyncMode::Refresh,
        holder(Role::Slot, Uncommitted::Files(1)),
        SyncStep::HolderDirty("app-wt1")
    )]
    #[case::holder_git_cannot_read(
        counted(1, 2),
        SyncMode::Reset,
        holder(Role::Slot, Uncommitted::Unknown),
        SyncStep::HolderUnknown("app-wt1")
    )]
    #[case::clean_unmanaged_holder(
        counted(1, 2),
        SyncMode::Reset,
        holder(Role::Unmanaged, CLEAN),
        SyncStep::HeldByUnmanaged("app-wt1")
    )]
    #[case::up_to_date_in_an_unmanaged_holder(
        counted(0, 0),
        SyncMode::Reset,
        holder(Role::Unmanaged, CLEAN),
        SyncStep::UpToDate
    )]
    fn plan_sync__should_move_only_a_branch_it_can_move_safely(
        #[case] divergence: Option<Divergence>,
        #[case] mode: SyncMode,
        #[case] holder: Option<Holder<&'static str>>,
        #[case] expected: SyncStep<&'static str>,
    ) {
        // When
        let step = plan_sync(divergence, mode, holder);

        // Then
        assert_eq!(step, expected);
    }

    #[rstest]
    #[case::not_fetched(
        false,
        None,
        &["feature is at 111111111, but PR #42 is 222222222, not fetched yet; rerun with --refresh"]
    )]
    #[case::incomparable(true, None, &["could not compare feature with PR #42"])]
    #[case::only_local_commits(true, counted(2, 0), &[])]
    #[case::behind(
        true,
        counted(0, 3),
        &["feature is 3 commit(s) behind PR #42; rerun with --refresh to fast-forward it"]
    )]
    #[case::diverged(
        true,
        counted(1, 2),
        &[
            "feature has diverged from PR #42 (1 local only, 2 remote only commit(s)); the remote was probably rebased",
            "if the local only commits are old copies, rerun with --reset to move feature to PR #42",
        ]
    )]
    fn stale_warning__should_warn_when_the_branch_lags_or_has_diverged(
        #[case] fetched: bool,
        #[case] divergence: Option<Divergence>,
        #[case] expected: &[&str],
    ) {
        // Given
        let branch = BranchName::try_new("feature").unwrap();
        let (local, target) = (
            ObjectId::new(LOCAL).unwrap(),
            ObjectId::new(TARGET).unwrap(),
        );
        let facts = Staleness {
            local: &local,
            target: &target,
            fetched,
            divergence,
        };

        // When
        let warnings = stale_warning(&branch, "PR #42", &facts);

        // Then
        assert_eq!(warnings, expected);
    }
}
