//! `wtpool bg`.

use std::path::PathBuf;

use crate::error::Error;
use crate::forge::Forge;
use crate::git::Git;
use crate::names::BranchName;
use crate::refs::SyncMode;
use crate::report::{Discard, Report};
use crate::resolve::{Unresolved, run_reported};
use crate::worktree::{self, Worktree};

use super::{Pool, parking, refusals, slots};

const NO_SLOT_CHANGED: &str = "no slot changed";

/// Where `bg` puts a branch.
#[derive(Debug, Clone)]
enum Parking {
    /// A slot already holds the branch.
    Held(Worktree),
    Free(Worktree),
    NewSlot,
}

impl<F> Pool<F> {
    /// `wtpool bg`: parks the branch a `<ref>` names in a free slot and returns the slot's path,
    /// or the path of the slot that already holds it. Creates a slot only while the pool is below
    /// its slot count. Progress goes to standard error, so standard output carries the path alone.
    ///
    /// # Errors
    ///
    /// Before any fetch or change, refuses a full pool, and a branch that the foreground or an
    /// unmanaged worktree holds, as the caller would then work where the user does. When `copy`
    /// or `on_new_slot` fails in a slot it created, it parks the branch there all the same, then
    /// fails.
    pub fn background(
        &self,
        reference: &str,
        sync: SyncMode,
        report: &mut impl Report,
    ) -> Result<PathBuf, Error>
    where
        F: Forge,
    {
        let _lock = self.lock(report)?;
        let unresolved = |u: Unresolved| u.into_error(reference, NO_SLOT_CHANGED);
        let resolver = self.resolver();
        let plan = resolver.plan(reference, report).map_err(unresolved)?;
        let plan = resolver
            .refuse_unsafe_checkout(plan, sync, |worktree| self.gone_error(worktree))
            .map_err(|e| unresolved(Unresolved::Fatal(e)))?;
        // Checked before the fetch, so a pool without room changes nothing.
        self.parking(plan.branch(), report)?;
        resolver.apply(&plan, report).map_err(unresolved)?;
        let branch = plan.branch();
        // Chosen again, as git run outside `wtpool` may have taken the slot during the fetch.
        let parking = self.parking(branch, &mut Discard)?;
        let mut failures = Vec::new();
        let slot = match parking {
            Parking::Held(holder) => {
                report.note(&format!("{branch} is already in {}", holder.name()));
                return Ok(holder.into_path().into_path_buf());
            }
            Parking::Free(slot) => slot.into_path(),
            Parking::NewSlot => {
                report.note("no free slot, creating one");
                self.create_slot(None, &mut failures, report)?
            }
        };
        let switch = ["switch", "--quiet", branch.as_str()];
        run_reported(&Git::at(&slot), switch, report)?.check()?;
        report.note(&format!("{branch} -> {}", worktree::basename(&slot)));
        slots::slot_setup(failures)?;
        Ok(slot.into_path_buf())
    }

    /// Where `bg` puts `branch`: the slot that holds it, else the slot
    /// [`parking::choose_parking`] picks. Refuses a holder outside the slots.
    fn parking(&self, branch: &BranchName, report: &mut impl Report) -> Result<Parking, Error> {
        if let Some(holder) = self.holder_of(branch)? {
            let role = self.config.role_of(&holder);
            refusals::outside_slots_refusal(branch, holder.path(), role)?;
            return Ok(Parking::Held(holder));
        }
        let facts = self.slot_facts(report)?;
        Ok(
            match parking::choose_parking(facts, self.config.slot_count.value)? {
                Some(slot) => Parking::Free(slot),
                None => Parking::NewSlot,
            },
        )
    }
}
