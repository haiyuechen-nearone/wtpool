//! `wtpool fg`.

use std::path::PathBuf;

use crate::error::Error;
use crate::github::ViewPr;
use crate::names::BranchName;
use crate::refs::SyncMode;
use crate::report::Report;
use crate::resolve::Unresolved;
use crate::worktree::{self, Worktree};

use super::refusals::{self, WorktreeState};
use super::{Pool, git};

const FOREGROUND_NOT_CHANGED: &str = "foreground not changed";

impl<V: ViewPr> Pool<V> {
    /// `wtpool fg`: checks a branch or PR out in the foreground, releasing it from the slot that
    /// holds it, and returns the foreground's path. Progress goes to standard error, so standard
    /// output carries the path alone.
    ///
    /// # Errors
    ///
    /// Before any fetch or change, refuses work it could lose in the foreground or in the slot
    /// that holds the branch, and a branch an unmanaged worktree holds. Fails when git cannot
    /// tell.
    pub fn foreground(
        &self,
        reference: &str,
        sync: SyncMode,
        report: &mut impl Report,
    ) -> Result<PathBuf, Error> {
        let lock = self.lock(report)?;
        let state = WorktreeState::read(&lock.foreground, &self.git())?;
        refusals::foreground_refusal(reference, &lock.foreground.name(), &state)
            .map_or(Ok(()), Err)?;
        let unresolved = |u: Unresolved| u.into_error(reference, FOREGROUND_NOT_CHANGED);
        let not_changed = |e: Error| unresolved(Unresolved::Fatal(e));
        let plan = self.plan(reference, report).map_err(unresolved)?;
        let plan = self
            .refuse_unsafe_checkout(plan, sync)
            .map_err(not_changed)?;
        self.refuse_unreleasable(plan.branch())
            .map_err(not_changed)?;
        self.apply(&plan, report).map_err(unresolved)?;
        let branch = plan.branch();
        match self.holder_of(branch)? {
            Some(holder) if self.is_foreground(holder.path()) => {
                report.note(&format!("{branch} is already in the foreground"));
            }
            holder => {
                if let Some(holder) = holder {
                    // Again, as git run outside `wtpool` may have changed the holder during the fetch.
                    self.refuse_release(branch, &holder).map_err(not_changed)?;
                    report.note(&format!("releasing {branch} from {}", holder.name()));
                    git::run_reported(&holder.git(), ["switch", "--detach", "--quiet"], report)?
                        .check()?;
                }
                git::run_reported(&self.git(), ["switch", "--quiet", branch.as_str()], report)?
                    .check()?;
                report.note(&format!(
                    "{branch} -> {}",
                    worktree::basename(&self.config.foreground)
                ));
            }
        }
        let log = self.git().run(["log", "-1", "--format=  %h %s"])?;
        // A note, not raw output: the subject comes from whoever wrote the commit.
        log.stdout
            .split_terminator('\n')
            .for_each(|line| report.note(line));
        log.forward_stderr(report).check()?;
        Ok(self.config.foreground.to_path_buf())
    }

    /// Refuses, before `fg` changes anything, a branch whose holder it would have to release but
    /// must not.
    fn refuse_unreleasable(&self, branch: &BranchName) -> Result<(), Error> {
        match self.holder_of(branch)? {
            Some(holder) if !self.is_foreground(holder.path()) => {
                self.refuse_release(branch, &holder)
            }
            _ => Ok(()),
        }
    }

    /// Refuses to release `branch` from `holder`, a worktree other than the foreground, as
    /// [`refusals::release_refusal`] decides.
    fn refuse_release(&self, branch: &BranchName, holder: &Worktree) -> Result<(), Error> {
        let state = WorktreeState::read(holder, &self.git())?;
        let role = self.config.role_of(holder);
        refusals::release_refusal(branch, holder.path(), role, &state).map_or(Ok(()), Err)
    }
}
