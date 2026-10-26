//! `wtpool ls`, `free`, `new`, `rm`, `path` and `setup`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::error::{Error, Refusal};
use crate::names::{BranchName, InvalidSlotName, SlotName};
use crate::report::{Discard, Report};
use crate::worktree::{self, Head};
use crate::worktree_state::Row;

use super::refusals::{self, WorktreeState};
use super::{AgentGuard, Pool, git, slots};

impl<F> Pool<F> {
    /// `wtpool ls`: every worktree that exists on disk, in `git worktree list` order.
    /// Warns about each worktree whose directory is gone.
    pub fn list(&self, report: &mut impl Report) -> Result<Vec<Row>, Error> {
        let mut rows = Vec::new();
        for worktree in self.worktrees(report)? {
            if worktree.is_gone() {
                report.warn(&self.gone_warning(&worktree));
                continue;
            }
            if !worktree.path().is_dir() {
                continue;
            }
            let role = self.config.role_of(&worktree);
            let state = worktree.facts(&self.git())?.state();
            let branch = match worktree.head() {
                Head::Branch(branch) => BranchName::try_new(branch).ok(),
                Head::Detached | Head::Unknown => None,
            };
            rows.push(Row {
                branch,
                role,
                state,
                worktree: worktree.name(),
            });
        }
        Ok(rows)
    }

    /// `wtpool free`: detaches the worktree named by its path, its slot name or the branch it
    /// holds. `agents` sets the rule for an agent that works or waits there.
    pub fn free(
        &self,
        arg: &str,
        agents: AgentGuard<'_>,
        report: &mut impl Report,
    ) -> Result<(), Error> {
        let lock = self.lock(report)?;
        let worktrees = self.worktrees(&mut Discard)?;
        let found = match self.at_path_or_slot(&worktrees, arg) {
            Some(worktree) => Some(worktree.clone()),
            None => match BranchName::try_new(arg) {
                Ok(branch) => self.any_holder_of(&branch)?,
                Err(_) => None,
            },
        };
        let worktree = found.ok_or_else(|| Error::NoMatch(arg.to_owned()))?;
        if self.is_foreground(worktree.path()) {
            return Err(Refusal::DetachForeground.into());
        }
        if worktree.is_gone() {
            return Err(self.gone_error(&worktree).into());
        }
        let state = WorktreeState::read(&worktree, &self.git())?;
        refusals::detach_refusal(&worktree.name(), &state).map_or(Ok(()), Err)?;
        self.refuse_busy_agents(agents, &lock.common_dir, &[&worktree])?;
        git::run_reported(&worktree.git(), ["switch", "--detach", "--quiet"], report)?.check()?;
        report.info(&format!("{} released", worktree.name()));
        Ok(())
    }

    /// `wtpool new`: adds a slot, named `name` or the next free `<prefix><n>`. Refuses a
    /// `name` without the slot prefix, as `wtpool` would never count it as a slot.
    /// Fails after adding it when `copy` or `on_new_slot` fails there.
    pub fn new_slot(
        &self,
        name: Option<SlotName>,
        report: &mut impl Report,
    ) -> Result<PathBuf, Error> {
        if let Some(name) = &name
            && !self.config.slot_prefix.manages(name.as_str())
        {
            let prefix = self.config.slot_prefix_source.describe(
                "WTPOOL_SLOT_PREFIX",
                "slot_prefix",
                &format!(" '{}'", self.config.slot_prefix.as_str()),
            );
            return Err(Error::InvalidSlotName {
                name: name.to_string(),
                reason: InvalidSlotName::MissingPrefix(prefix),
            });
        }
        let _lock = self.lock(report)?;
        let mut failures = Vec::new();
        let path = self.create_slot(name, &mut failures, report)?;
        report.info(&format!("created {}", path.display()));
        let count = self.managed_slots(report)?.len();
        let limit = self.config.slot_count;
        if count > limit.get() {
            let limit = self.config.slot_count_source.describe(
                "WTPOOL_SLOT_COUNT",
                "slot_count",
                &format!("={limit}"),
            );
            report.warn(&format!("the pool now has {count} slots, above {limit}"));
        }
        slots::slot_setup(failures)?;
        Ok(path.into_path_buf())
    }

    /// `wtpool rm`: removes a clean worktree other than the foreground, named by its path or
    /// slot name.
    pub fn remove(&self, arg: &str, report: &mut impl Report) -> Result<(), Error> {
        let _lock = self.lock(report)?;
        let worktrees = self.worktrees(&mut Discard)?;
        let worktree = match self.at_path_or_slot(&worktrees, arg) {
            Some(worktree) => worktree,
            None => {
                if !Path::new(arg).exists() {
                    SlotName::try_new(arg).map_err(|reason| Error::InvalidSlotName {
                        name: arg.to_owned(),
                        reason,
                    })?;
                }
                return Err(Error::NoMatch(arg.to_owned()));
            }
        };
        if self.is_foreground(worktree.path()) {
            return Err(Refusal::RemoveForeground.into());
        }
        let gone = worktree.is_gone();
        let state = WorktreeState::read(worktree, &self.git())?;
        refusals::removal_refusal(&worktree.name(), gone, &state).map_or(Ok(()), Err)?;
        let ran = self.git().run([
            OsStr::new("worktree"),
            OsStr::new("remove"),
            worktree.path().as_os_str(),
        ])?;
        ran.forward_stderr(report).check()?;
        report.info(&format!("removed {}", worktree.name()));
        Ok(())
    }

    /// `wtpool path`: the worktree holding `branch`. Fails when only a worktree whose
    /// directory is gone holds it.
    pub fn path_of(&self, branch: &str) -> Result<PathBuf, Error> {
        let not_checked_out = || Error::NotCheckedOut(branch.to_owned());
        let name = BranchName::try_new(branch).map_err(|_| not_checked_out())?;
        match self.any_holder_of(&name)? {
            Some(holder) if holder.is_gone() => Err(self.gone_error(&holder).into()),
            Some(holder) => Ok(holder.into_path().into_path_buf()),
            None => Err(not_checked_out()),
        }
    }

    /// `wtpool setup`: fills the pool up to the slot count, copies the `copy` files that
    /// existing slots lack, then lists it. Fails after all that when `copy` or
    /// `on_new_slot` failed in a slot.
    pub fn setup(&self, report: &mut impl Report) -> Result<Vec<Row>, Error> {
        let _lock = self.lock(report)?;
        // `list` below warns about slots whose directory is gone.
        let existing = self.managed_slots(&mut Discard)?;
        let mut failures = Vec::new();
        for _ in existing.len()..self.config.slot_count.get() {
            let path = self.create_slot(None, &mut failures, report)?;
            report.info(&format!("created {}", worktree::basename(&path)));
        }
        // A slot created above got its copies when it was created.
        for slot in existing {
            self.copy_into(slot.path(), &mut failures, report);
        }
        report.info("");
        let rows = self.list(report)?;
        slots::slot_setup(failures)?;
        Ok(rows)
    }
}
