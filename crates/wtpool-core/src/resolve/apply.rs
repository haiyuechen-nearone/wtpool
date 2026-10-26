//! Fetching, creating or syncing the branch of a [`CheckedPlan`], and setting its upstream.

use crate::error::{Error, GitError, Refusal};
use crate::forge::{ChangeHead, Fetch};
use crate::git::left_right;
use crate::names::{BranchName, ObjectId};
use crate::refs::SyncMode;
use crate::report::Report;
use crate::worktree::Worktree;

use super::divergence::{self, Holder, Staleness, Stay, SyncStep};
use super::guard::CheckedPlan;
use super::plan::{Step, Unresolved};
use super::{Resolver, run_reported};

/// Only `wtpool` writes this ref, and only under the pool lock.
const FETCHED: &str = "refs/wtpool-fetch/head";

/// A git config value.
#[derive(Debug, Clone)]
enum Configured {
    Set(String),
    Unset,
    /// git could not read the config.
    Unknown,
}

impl<F> Resolver<'_, F> {
    /// Fetches, creates or syncs the planned branch. The upstream is set last, so
    /// a refused sync leaves the branch's config as it was.
    pub(crate) fn apply(
        &self,
        checked: &CheckedPlan,
        report: &mut impl Report,
    ) -> Result<(), Unresolved> {
        let CheckedPlan { plan, sync } = checked;
        let branch = &plan.branch;
        match &plan.step {
            Step::Local => self.apply_local(branch, *sync, report),
            Step::LocalChange(change) => self.apply_local_change(branch, change, *sync, report),
            Step::FetchBranch => self.fetch_branch(branch, report),
            Step::FetchChange(change) => self.fetch_change(branch, change, report),
        }
    }

    fn apply_local(
        &self,
        branch: &BranchName,
        sync: SyncMode,
        report: &mut impl Report,
    ) -> Result<(), Unresolved> {
        report.note(&format!("{branch}: already local"));
        let remote = &self.config.remote;
        if sync == SyncMode::No {
            if let Some(tip) = self.commit_of(&self.remote_ref(branch))? {
                let label = format!("{remote}/{branch} (as of the last fetch)");
                self.warn_if_stale(branch, &tip, &label, report)?;
            }
        } else {
            match self.remote_has_branch(branch, report)? {
                Some(true) => self.fetch_and_sync(branch, &branch.branch_ref(), sync, report)?,
                Some(false) => report.warn(&format!(
                    "{remote} has no branch named '{branch}'; nothing to refresh"
                )),
                None => report.warn(&format!(
                    "no remote '{remote}' here; {branch} not refreshed"
                )),
            }
        }
        Ok(self.adopt_upstream(branch, report)?)
    }

    fn apply_local_change(
        &self,
        branch: &BranchName,
        change: &ChangeHead,
        sync: SyncMode,
        report: &mut impl Report,
    ) -> Result<(), Unresolved> {
        report.note(&format!("{branch}: already local"));
        if sync == SyncMode::No {
            let label = format!("the head of {}", change.name);
            self.warn_if_stale(branch, &change.head, &label, report)?;
        } else {
            let refspec = match change.fetch {
                Fetch::ChangeRef => change.change_ref.refspec.clone(),
                Fetch::Branch => branch.branch_ref(),
            };
            self.fetch_and_sync(branch, &refspec, sync, report)?;
        }
        Ok(self.adopt_upstream(branch, report)?)
    }

    fn fetch_branch(
        &self,
        branch: &BranchName,
        report: &mut impl Report,
    ) -> Result<(), Unresolved> {
        let remote = self.config.remote.as_str();
        report.note(&format!("fetching {branch} from {remote}"));
        let refspec = branch.branch_ref();
        let args = ["fetch", "--quiet", remote, refspec.as_str()];
        if !run_reported(&self.git, args, report)?.success {
            return Err(Unresolved::Fatal(
                GitError::FetchBranch(branch.to_string()).into(),
            ));
        }
        self.track_remote(branch, report)
    }

    fn fetch_change(
        &self,
        branch: &BranchName,
        change: &ChangeHead,
        report: &mut impl Report,
    ) -> Result<(), Unresolved> {
        let remote = self.config.remote.as_str();
        report.note(&format!("fetching {} -> {branch}", change.name));
        // Prefer the real branch, so <remote>/<branch> exists and the local branch can
        // track it. Without one, the change's own ref gives an untracked branch.
        let local_ref = branch.branch_ref();
        let direct = self
            .git
            .run(["fetch", "--quiet", remote, local_ref.as_str()])?;
        if direct.success {
            return self.track_remote(branch, report);
        }
        let refspec = format!("{}:{local_ref}", change.change_ref.refspec);
        let args = ["fetch", "--quiet", remote, refspec.as_str()];
        if !run_reported(&self.git, args, report)?.success {
            return Err(Unresolved::Fatal(
                GitError::FetchChange(change.name.clone()).into(),
            ));
        }
        report.warn(&change.change_ref.note);
        Ok(())
    }

    /// Fetches `src` and moves `branch` to it, as [`Resolver::sync_branch`] decides.
    fn fetch_and_sync(
        &self,
        branch: &BranchName,
        src: &str,
        sync: SyncMode,
        report: &mut impl Report,
    ) -> Result<(), Unresolved> {
        let fetched = self
            .fetch(src, report)?
            .ok_or(Unresolved::Fatal(GitError::Fetch.into()))?;
        self.sync_branch(branch, &fetched, sync, report)
    }

    fn track_remote(
        &self,
        branch: &BranchName,
        report: &mut impl Report,
    ) -> Result<(), Unresolved> {
        let upstream = self.remote_ref(branch);
        let args = ["branch", "--quiet", "--track", branch.as_str(), &upstream];
        let ran = run_reported(&self.git, args, report)?;
        if ran.success {
            Ok(())
        } else {
            Err(Unresolved::Fatal(
                GitError::CreateBranch(branch.to_string()).into(),
            ))
        }
    }

    /// The commit fetched as `src`, `None` when the fetch fails. Read from [`FETCHED`], not
    /// `FETCH_HEAD`, as any other fetch in the foreground rewrites it.
    fn fetch(&self, src: &str, report: &mut impl Report) -> Result<Option<ObjectId>, Error> {
        let refspec = format!("+{src}:{FETCHED}");
        let args = ["fetch", "--quiet", self.config.remote.as_str(), &refspec];
        let ran = run_reported(&self.git, args, report)?;
        if !ran.success {
            return Ok(None);
        }
        let fetched = self.commit_of(FETCHED)?;
        let _deleted = run_reported(&self.git, ["update-ref", "-d", FETCHED], report)?;
        Ok(fetched)
    }

    /// The commit `rev` names, `None` when there is no such commit.
    fn commit_of(&self, rev: &str) -> Result<Option<ObjectId>, Error> {
        let ran = self.git.run([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ])?;
        Ok(ran.line().and_then(ObjectId::new))
    }

    /// Gives a local branch its upstream when the remote has a branch of the same
    /// name and nothing is tracked yet, so `git pull` works where it is checked out.
    /// An upstream in the config is kept, also when its remote branch is gone, with
    /// a warning when it is not `<remote>/<branch>`.
    fn adopt_upstream(&self, branch: &BranchName, report: &mut impl Report) -> Result<(), Error> {
        if self.commit_of(&self.remote_ref(branch))?.is_none() {
            return Ok(());
        }
        let remote = self.config.remote.as_str();
        match self.branch_config(branch, "merge")? {
            Configured::Unknown => {
                report.warn(&format!(
                    "could not read the upstream of {branch}; left as it is"
                ));
            }
            Configured::Set(merge) => {
                let tracked = match self.branch_config(branch, "remote")? {
                    Configured::Set(tracked) => Some(tracked),
                    Configured::Unset | Configured::Unknown => None,
                };
                if tracked.as_deref() != Some(remote) || merge != branch.branch_ref() {
                    report.warn(&format!(
                        "{branch} tracks {}, not {remote}/{branch}; upstream left as it is",
                        upstream_label(tracked.as_deref(), &merge)
                    ));
                }
            }
            Configured::Unset => {
                let upstream = format!("--set-upstream-to={}", self.remote_ref(branch));
                let args = ["branch", "--quiet", upstream.as_str(), branch.as_str()];
                let ran = run_reported(&self.git, args, report)?;
                if ran.success {
                    report.note(&format!("{branch}: upstream set to {remote}/{branch}"));
                } else {
                    report.warn(&format!(
                        "could not set the upstream of {branch} to {remote}/{branch}"
                    ));
                }
            }
        }
        Ok(())
    }

    fn remote_ref(&self, branch: &BranchName) -> String {
        branch.remote_ref(&self.config.remote)
    }

    /// `branch.<branch>.<key>` from the git config. Read from the config, not by
    /// resolving `<branch>@{upstream}`, as that also fails when the upstream's remote
    /// branch is gone.
    fn branch_config(&self, branch: &BranchName, key: &str) -> Result<Configured, Error> {
        let ran = self
            .git
            .run(["config", "--get", &format!("branch.{branch}.{key}")])?;
        Ok(match (ran.code, ran.line()) {
            (Some(0), Some(value)) if !value.is_empty() => Configured::Set(value.to_owned()),
            (Some(1), _) => Configured::Unset,
            _ => Configured::Unknown,
        })
    }

    /// Moves a local branch to a fetched commit, as [`divergence::plan_sync`] chooses.
    fn sync_branch(
        &self,
        branch: &BranchName,
        target: &ObjectId,
        mode: SyncMode,
        report: &mut impl Report,
    ) -> Result<(), Unresolved> {
        let range = format!("{}...{target}", branch.branch_ref());
        let compared = left_right(&self.git, &range)?;
        let holder = self.sync_holder(branch)?;
        match divergence::plan_sync(compared, mode, holder) {
            SyncStep::FastForward { behind, holder } => {
                self.fast_forward(branch, target, holder.as_ref(), report)?;
                report.note(&format!("{branch}: fast-forwarded {behind} commit(s)"));
                Ok(())
            }
            SyncStep::ResetWithBackup { ahead, holder } => {
                self.reset_with_backup(branch, target, ahead, holder.as_ref(), report)
            }
            SyncStep::Stay(stay) => stay_in_place(branch, stay, report),
        }
    }

    /// The worktree that has `branch` checked out, as [`divergence::plan_sync`] weighs it.
    fn sync_holder(&self, branch: &BranchName) -> Result<Option<Holder<Worktree>>, Error> {
        let Some(worktree) = self.holder_of(branch)? else {
            return Ok(None);
        };
        Ok(Some(Holder {
            role: self.config.role_of(&worktree),
            dirty: worktree.uncommitted()?,
            worktree,
        }))
    }

    fn fast_forward(
        &self,
        branch: &BranchName,
        target: &ObjectId,
        holder: Option<&Worktree>,
        report: &mut impl Report,
    ) -> Result<(), Unresolved> {
        let local_ref = branch.branch_ref();
        let (git, args) = match holder {
            Some(holder) => (
                holder.git(),
                vec!["merge", "--ff-only", "--quiet", target.as_str()],
            ),
            None => (
                self.git.clone(),
                vec!["update-ref", &local_ref, target.as_str()],
            ),
        };
        let ran = run_reported(&git, args, report)?;
        if ran.success {
            Ok(())
        } else {
            Err(Unresolved::Quiet)
        }
    }

    /// Moves a diverged branch to `target` after saving its tip under `refs/wtpool-backup/`.
    fn reset_with_backup(
        &self,
        branch: &BranchName,
        target: &ObjectId,
        ahead: usize,
        holder: Option<&Worktree>,
        report: &mut impl Report,
    ) -> Result<(), Unresolved> {
        let local_ref = branch.branch_ref();
        let old = run_reported(&self.git, ["rev-parse", local_ref.as_str()], report)?;
        let old = old
            .line()
            .and_then(ObjectId::new)
            .ok_or(Unresolved::Quiet)?;
        let short = self.git.run(["rev-parse", "--short", old.as_str()])?;
        let short = short
            .line()
            .filter(|s| !s.is_empty())
            .ok_or(Unresolved::Quiet)?;
        let backup = format!("refs/wtpool-backup/{branch}/{short}");
        let args = ["update-ref", backup.as_str(), old.as_str()];
        if !run_reported(&self.git, args, report)?.success {
            return Err(Unresolved::Quiet);
        }
        let (git, args) = match holder {
            Some(holder) => (
                holder.git(),
                vec!["reset", "--hard", "--quiet", target.as_str()],
            ),
            None => (
                self.git.clone(),
                vec![
                    "update-ref",
                    "-m",
                    "wtpool: reset to the remote head",
                    &local_ref,
                    target.as_str(),
                    old.as_str(),
                ],
            ),
        };
        let ran = run_reported(&git, args, report)?;
        if !ran.success {
            return Err(Unresolved::Quiet);
        }
        report.note(&format!(
            "{branch}: reset to the remote head, {ahead} local only commit(s) left behind"
        ));
        report.note(&format!(
            "  old tip saved as {backup}; to undo, run git reset --hard {old} where {branch} is checked out"
        ));
        Ok(())
    }

    /// Warns, without fetching, when a local branch lags or has diverged from
    /// `target`, named `label`, as [`divergence::stale_warning`] words it.
    fn warn_if_stale(
        &self,
        branch: &BranchName,
        target: &ObjectId,
        label: &str,
        report: &mut impl Report,
    ) -> Result<(), Error> {
        let local_ref = branch.branch_ref();
        let local = run_reported(&self.git, ["rev-parse", &local_ref], report)?;
        let local = local.line().and_then(ObjectId::new);
        let Some(local) = local.filter(|local| local != target) else {
            // Nothing to compare: git cannot read the branch, or it is at `target` already.
            return Ok(());
        };
        let fetched = self.commit_of(target.as_str())?.is_some();
        let compared = if fetched {
            left_right(&self.git, &format!("{local}...{target}"))?
        } else {
            None
        };
        let facts = Staleness {
            local: &local,
            target,
            fetched,
            divergence: compared,
        };
        for warning in divergence::stale_warning(branch, label, &facts) {
            report.warn(&warning);
        }
        Ok(())
    }
}

/// Reports the reason `branch` stays where it is, an error unless it is up to date.
fn stay_in_place(
    branch: &BranchName,
    stay: Stay<Worktree>,
    report: &mut impl Report,
) -> Result<(), Unresolved> {
    match stay {
        Stay::Incomparable => {
            report.warn(&format!(
                "could not compare {branch} with the remote head; not refreshed"
            ));
            Err(Unresolved::Quiet)
        }
        Stay::UpToDate => {
            report.note(&format!("{branch}: already up to date"));
            Ok(())
        }
        Stay::Diverged { ahead, behind } => {
            report.warn(&format!(
                "{branch} has diverged from the remote head ({ahead} local only, {behind} remote only commit(s)); not refreshed"
            ));
            report.warn(
                "if the remote was rebased and the local only commits are old copies, rerun with --reset",
            );
            Err(Unresolved::Quiet)
        }
        Stay::HeldByUnmanaged(holder) => Err(Unresolved::Fatal(
            Refusal::HeldByUnmanaged {
                branch: branch.to_string(),
                worktree: holder.name(),
                path: holder.path().to_path_buf(),
            }
            .into(),
        )),
        Stay::HolderUnknown(holder) => Err(Unresolved::Fatal(
            Refusal::StatusUnknown(holder.name()).into(),
        )),
        Stay::HolderDirty(holder) => {
            report.warn(&format!(
                "{branch} is checked out dirty in {}; not refreshed",
                holder.name()
            ));
            Err(Unresolved::Quiet)
        }
    }
}

/// An upstream as `git status` names it, such as `fork/feature`.
fn upstream_label(remote: Option<&str>, merge: &str) -> String {
    match (remote, merge.strip_prefix("refs/heads/")) {
        (Some(remote), Some(name)) if remote != "." => format!("{remote}/{name}"),
        (_, Some(name)) => name.to_owned(),
        (_, None) => merge.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::remote_branch(Some("fork"), "refs/heads/feature", "fork/feature")]
    #[case::local_branch(Some("."), "refs/heads/main", "main")]
    #[case::no_remote(None, "refs/heads/feature", "feature")]
    #[case::not_a_branch(Some("origin"), "refs/tags/v1", "refs/tags/v1")]
    fn upstream_label__should_name_the_upstream_as_git_status_does(
        #[case] remote: Option<&str>,
        #[case] merge: &str,
        #[case] expected: &str,
    ) {
        // When
        let label = upstream_label(remote, merge);

        // Then
        assert_eq!(label, expected);
    }
}
