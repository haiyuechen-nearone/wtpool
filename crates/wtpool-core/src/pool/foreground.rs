//! `wtpool fg`.

use std::path::PathBuf;

use crate::error::Error;
use crate::forge::Forge;
use crate::names::BranchName;
use crate::refs::SyncMode;
use crate::report::Report;
use crate::resolve::{Unresolved, run_reported};
use crate::worktree::{self, Worktree};

use super::refusals::{self, WorktreeState};
use super::{AgentGuard, Pool};

const FOREGROUND_NOT_CHANGED: &str = "foreground not changed";

impl<F> Pool<F> {
    /// `wtpool fg`: checks the branch a `<ref>` names out in the foreground, releasing it from the
    /// slot that holds it, and returns the foreground's path. Progress goes to standard error, so
    /// standard output carries the path alone.
    ///
    /// # Errors
    ///
    /// Before any fetch or change, refuses work it could lose in the foreground or in the slot
    /// that holds the branch, and a branch an unmanaged worktree holds. Fails when git cannot
    /// tell. `agents` sets the rule for an agent that works or waits in those two.
    pub fn foreground(
        &self,
        reference: &str,
        sync: SyncMode,
        agents: AgentGuard<'_>,
        report: &mut impl Report,
    ) -> Result<PathBuf, Error>
    where
        F: Forge,
    {
        let lock = self.lock(report)?;
        let state = WorktreeState::read(&lock.foreground, &self.git())?;
        refusals::foreground_refusal(reference, &lock.foreground.name(), &state)?;
        let unresolved = |u: Unresolved| u.into_error(reference, FOREGROUND_NOT_CHANGED);
        let not_changed = |e: Error| unresolved(Unresolved::Fatal(e));
        let resolver = self.resolver();
        let plan = resolver.plan(reference, report).map_err(unresolved)?;
        let plan = resolver
            .refuse_unsafe_checkout(plan, sync, |worktree| self.gone_error(worktree))
            .map_err(not_changed)?;
        let released = self
            .refuse_unreleasable(plan.branch())
            .map_err(not_changed)?;
        let moved: Vec<&Worktree> = std::iter::once(&lock.foreground).chain(&released).collect();
        self.refuse_busy_agents(agents, &lock.common_dir, &moved)
            .map_err(not_changed)?;
        resolver.apply(&plan, report).map_err(unresolved)?;
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
                    run_reported(&holder.git(), ["switch", "--detach", "--quiet"], report)?
                        .check()?;
                }
                run_reported(&self.git(), ["switch", "--quiet", branch.as_str()], report)?
                    .check()?;
                report.note(&format!(
                    "{branch} -> {}",
                    worktree::basename(&self.config.foreground.value)
                ));
            }
        }
        let log = self.git().run(["log", "-1", "--format=  %h %s"])?;
        // A note, not raw output: the subject comes from whoever wrote the commit.
        log.stdout
            .split_terminator('\n')
            .for_each(|line| report.note(line));
        log.forward_stderr(report).check()?;
        Ok(self.config.foreground.value.to_path_buf())
    }

    /// Refuses, before `fg` changes anything, a branch whose holder it would have to release but
    /// must not. Gives the holder it would release.
    fn refuse_unreleasable(&self, branch: &BranchName) -> Result<Option<Worktree>, Error> {
        match self.holder_of(branch)? {
            Some(holder) if !self.is_foreground(holder.path()) => {
                self.refuse_release(branch, &holder)?;
                Ok(Some(holder))
            }
            _ => Ok(None),
        }
    }

    /// Refuses to release `branch` from `holder`, a worktree other than the foreground, as
    /// [`refusals::release_refusal`] decides.
    fn refuse_release(&self, branch: &BranchName, holder: &Worktree) -> Result<(), Error> {
        let state = WorktreeState::read(holder, &self.git())?;
        let role = self.config.role_of(holder);
        Ok(refusals::release_refusal(
            branch,
            holder.path(),
            role,
            &state,
        )?)
    }
}
