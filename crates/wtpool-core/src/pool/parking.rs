//! The slot `bg` parks a branch in: the facts it reads of each slot, and the choice.

use crate::config::SlotCount;
use crate::error::{Error, Refusal};
use crate::report::Report;
use crate::worktree::{Head, Worktree};

use super::Pool;
use super::refusals::{self, WorktreeState};

/// A managed slot whose directory exists, as [`choose_parking`] weighs it. `slot` is handed back
/// when it is chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SlotFacts<T> {
    pub(super) slot: T,
    pub(super) name: String,
    pub(super) head: SlotHead,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SlotHead {
    Branch(String),
    Detached(WorktreeState),
    /// git recorded neither a branch nor a detached `HEAD`.
    Unknown,
}

impl<F> Pool<F> {
    /// The facts of each managed slot whose directory exists. Warns about the others.
    pub(super) fn slot_facts(
        &self,
        report: &mut impl Report,
    ) -> Result<Vec<SlotFacts<Worktree>>, Error> {
        let repo = self.git();
        let mut facts = Vec::new();
        for slot in self.managed_slots(report)? {
            let head = match slot.head() {
                Head::Detached => SlotHead::Detached(WorktreeState::read(&slot, &repo)?),
                Head::Branch(branch) => SlotHead::Branch(branch.clone()),
                Head::Unknown => SlotHead::Unknown,
            };
            facts.push(SlotFacts {
                name: slot.name(),
                head,
                slot,
            });
        }
        Ok(facts)
    }
}

/// The first slot that is detached and holds no work, `None` when there is none but room for
/// another slot below `count`. Fails when the pool is full, with one line per slot on the reason
/// it is busy: the branch it holds, or the work that keeps a detached slot from being reused.
pub(super) fn choose_parking<T>(
    slots: Vec<SlotFacts<T>>,
    count: SlotCount,
) -> Result<Option<T>, Error> {
    let full = slots.len() >= count.get();
    let mut blocked = Vec::new();
    for SlotFacts { slot, name, head } in slots {
        match head {
            SlotHead::Detached(state) => match refusals::refusal(&name, &state) {
                None => return Ok(Some(slot)),
                Some(refusal) => blocked.push(refusal),
            },
            SlotHead::Branch(branch) => {
                blocked.push(Refusal::SlotHoldsBranch { slot: name, branch });
            }
            SlotHead::Unknown => blocked.push(Refusal::HeadUnknown(name)),
        }
    }
    if full {
        return Err(Refusal::PoolFull {
            limit: count.get(),
            blocked,
        }
        .into());
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::worktree_state::{Operation, Uncommitted, Unreferenced};

    fn detached(dirty: Uncommitted) -> SlotHead {
        SlotHead::Detached(WorktreeState {
            dirty,
            operation: Operation::Idle,
            unreferenced: Unreferenced::Commits(0),
            head_commit: None,
        })
    }

    fn slots(heads: Vec<SlotHead>) -> Vec<SlotFacts<u32>> {
        (1..)
            .zip(heads)
            .map(|(n, head)| SlotFacts {
                slot: n,
                name: format!("app-wt{n}"),
                head,
            })
            .collect()
    }

    const CLEAN: Uncommitted = Uncommitted::Files(0);

    #[rstest]
    #[case::first_free_slot(
        vec![SlotHead::Branch("a".to_owned()), detached(CLEAN), detached(CLEAN)],
        3,
        Ok(Some(2))
    )]
    #[case::free_slot_above_the_count(
        vec![SlotHead::Branch("a".to_owned()), detached(CLEAN)],
        1,
        Ok(Some(2))
    )]
    #[case::room_for_a_new_slot(vec![detached(Uncommitted::Files(1))], 2, Ok(None))]
    #[case::empty_pool(vec![], 1, Ok(None))]
    #[case::full_pool(
        vec![SlotHead::Branch("a".to_owned()), detached(Uncommitted::Files(1)), SlotHead::Unknown],
        3,
        Err(vec![
            "app-wt1 holds branch a".to_owned(),
            "app-wt2 has 1 uncommitted file(s)".to_owned(),
            "could not tell what app-wt3 has checked out; not reusing it".to_owned(),
            "no free slot and the pool is at its 3 slot limit; release one with 'wtpool free <slot>' (see 'wtpool ls')".to_owned(),
        ])
    )]
    fn choose_parking__should_take_the_first_free_slot_then_room_for_a_new_one(
        #[case] heads: Vec<SlotHead>,
        #[case] count: usize,
        #[case] expected: Result<Option<u32>, Vec<String>>,
    ) {
        // Given
        let count = SlotCount::try_new(count).unwrap();

        // When
        let chosen = choose_parking(slots(heads), count);

        // Then
        assert_eq!(chosen.map_err(|error| error.messages()), expected);
    }
}
