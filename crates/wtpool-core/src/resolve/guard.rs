//! The guard a [`Plan`] passes before it is applied.

use crate::error::{Error, Refusal};
use crate::names::BranchName;
use crate::pool::Pool;
use crate::refs::SyncMode;
use crate::report::Discard;
use crate::worktree::Worktree;
use crate::worktree_state::Operation;

use super::plan::Plan;

/// A [`Plan`] that [`Pool::refuse_unsafe_checkout`] found safe to apply in `sync` mode. Only that
/// guard makes one, so [`Pool::apply`] cannot run unguarded.
#[derive(Debug, Clone)]
pub(crate) struct CheckedPlan {
    pub(super) plan: Plan,
    pub(super) sync: SyncMode,
}

impl CheckedPlan {
    pub(crate) fn branch(&self) -> &BranchName {
        &self.plan.branch
    }
}

/// The facts [`checkout_risk`] weighs for one worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CheckoutFacts {
    /// The directory is gone, so its rebase and bisect files are not read.
    Gone,
    /// git could not name the branches a stopped rebase or bisect uses.
    Unreadable,
    Live {
        /// The branches a stopped rebase or bisect returns to or moves.
        operation_branches: Vec<String>,
        operation: Operation,
    },
}

/// The reason a worktree keeps `wtpool` from taking or moving a branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckoutRisk {
    /// A worktree whose directory is gone has the branch as `HEAD`, so its files are unknown.
    Gone,
    OperationUnknown,
    InProgress(&'static str),
}

impl<F> Pool<F> {
    /// Refuses, before `fg` or `bg` changes anything, a branch that git counts as checked out
    /// where `wtpool` cannot safely take or move it, as [`checkout_risk`] decides for each
    /// worktree.
    pub(crate) fn refuse_unsafe_checkout(
        &self,
        plan: Plan,
        sync: SyncMode,
    ) -> Result<CheckedPlan, Error> {
        let branch = &plan.branch;
        for worktree in self.worktrees(&mut Discard)? {
            let facts = checkout_facts(&worktree)?;
            let Some(risk) = checkout_risk(branch, worktree.holds_branch(branch), sync, &facts)
            else {
                continue;
            };
            return Err(match risk {
                CheckoutRisk::Gone => self.gone_error(&worktree),
                CheckoutRisk::OperationUnknown => Refusal::OperationUnknown(worktree.name()),
                CheckoutRisk::InProgress(operation) => Refusal::OperationInProgress {
                    worktree: worktree.name(),
                    operation,
                },
            }
            .into());
        }
        Ok(CheckedPlan { plan, sync })
    }
}

fn checkout_facts(worktree: &Worktree) -> Result<CheckoutFacts, Error> {
    if worktree.is_gone() {
        return Ok(CheckoutFacts::Gone);
    }
    Ok(match worktree.operation_branches()? {
        None => CheckoutFacts::Unreadable,
        Some(operation_branches) => CheckoutFacts::Live {
            operation_branches,
            operation: worktree.operation()?,
        },
    })
}

/// The risk a worktree poses to taking or moving `branch`, its `HEAD` when `held`:
/// - Its directory is gone while it holds the branch.
/// - A stopped rebase or bisect in it names the branch while its `HEAD` is not it, so git
///   would refuse the later switch.
/// - With a `sync` other than [`SyncMode::No`], it holds the branch in the middle of an
///   operation, or git cannot read it.
///
/// Without `sync`, a holder is left to the caller: `bg` returns it and `fg` checks it before
/// releasing it.
fn checkout_risk(
    branch: &BranchName,
    held: bool,
    sync: SyncMode,
    facts: &CheckoutFacts,
) -> Option<CheckoutRisk> {
    let syncing = sync != SyncMode::No;
    let (operation_branches, operation) = match facts {
        CheckoutFacts::Gone => return held.then_some(CheckoutRisk::Gone),
        CheckoutFacts::Unreadable => return syncing.then_some(CheckoutRisk::OperationUnknown),
        CheckoutFacts::Live {
            operation_branches,
            operation,
        } => (operation_branches, *operation),
    };
    let named = operation_branches
        .iter()
        .any(|name| name == branch.as_str());
    if !(held || named) || (held && !syncing) {
        return None;
    }
    match operation {
        // A rebase or bisect names the branch, yet no operation marker was found.
        Operation::Idle => named.then_some(CheckoutRisk::OperationUnknown),
        Operation::Unknown => Some(CheckoutRisk::OperationUnknown),
        operation => operation.label().map(CheckoutRisk::InProgress),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn live(operation_branches: &[&str], operation: Operation) -> CheckoutFacts {
        CheckoutFacts::Live {
            operation_branches: operation_branches
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            operation,
        }
    }

    #[rstest]
    #[case::gone_holder(true, SyncMode::No, CheckoutFacts::Gone, Some(CheckoutRisk::Gone))]
    #[case::gone_worktree_of_another_branch(false, SyncMode::Reset, CheckoutFacts::Gone, None)]
    #[case::unreadable_worktree_without_sync(false, SyncMode::No, CheckoutFacts::Unreadable, None)]
    #[case::unreadable_worktree_with_sync(
        false,
        SyncMode::Refresh,
        CheckoutFacts::Unreadable,
        Some(CheckoutRisk::OperationUnknown)
    )]
    #[case::idle_holder_with_sync(true, SyncMode::Reset, live(&[], Operation::Idle), None)]
    #[case::rebasing_holder_without_sync(true, SyncMode::No, live(&[], Operation::Rebase), None)]
    #[case::rebasing_holder_with_sync(
        true,
        SyncMode::Refresh,
        live(&[], Operation::Rebase),
        Some(CheckoutRisk::InProgress("rebase"))
    )]
    #[case::holder_git_cannot_read_with_sync(
        true,
        SyncMode::Reset,
        live(&[], Operation::Unknown),
        Some(CheckoutRisk::OperationUnknown)
    )]
    #[case::bisect_that_names_the_branch(
        false,
        SyncMode::No,
        live(&["feature"], Operation::Bisect),
        Some(CheckoutRisk::InProgress("bisect"))
    )]
    #[case::stale_file_that_names_the_branch(
        false,
        SyncMode::No,
        live(&["feature"], Operation::Idle),
        Some(CheckoutRisk::OperationUnknown)
    )]
    #[case::rebase_of_another_branch(
        false,
        SyncMode::Reset,
        live(&["other"], Operation::Rebase),
        None
    )]
    fn checkout_risk__should_refuse_only_a_worktree_that_uses_the_branch(
        #[case] held: bool,
        #[case] sync: SyncMode,
        #[case] facts: CheckoutFacts,
        #[case] expected: Option<CheckoutRisk>,
    ) {
        // Given
        let branch = BranchName::try_new("feature").unwrap();

        // When
        let risk = checkout_risk(&branch, held, sync, &facts);

        // Then
        assert_eq!(risk, expected);
    }
}
