//! Creating slots and setting them up: the `copy` files and the `on_new_slot` command.

use std::ffi::OsStr;

use crate::error::Error;
use crate::github::ViewPr;
use crate::names::SlotName;
use crate::paths::{self, AbsolutePath};
use crate::report::{Discard, Report};
use crate::run;

use super::{Pool, copy};

impl<V: ViewPr> Pool<V> {
    /// The first `<prefix><n>` that is neither on disk nor a registered worktree
    /// whose directory is gone.
    fn next_slot_name(&self) -> Result<SlotName, Error> {
        let registered = self.worktrees(&mut Discard)?;
        let prefix = &self.config.slot_prefix;
        let taken = |name: &SlotName| {
            let path = self.config.slot_dir.join(name.as_str());
            path.exists() || registered.iter().any(|w| paths::same_path(w.path(), &path))
        };
        Ok((1..u32::MAX)
            .map(|n| prefix.slot(n))
            .find(|name| !taken(name))
            .unwrap_or_else(|| prefix.slot(u32::MAX)))
    }

    /// Adds the slot `name`, or the next free `<prefix><n>`, then copies the `copy`
    /// files into it and runs `on_new_slot` there. A failure in those two steps goes to
    /// `failures` and keeps the slot.
    pub(super) fn create_slot(
        &self,
        name: Option<SlotName>,
        failures: &mut Vec<Error>,
        report: &mut impl Report,
    ) -> Result<AbsolutePath, Error> {
        let name = match name {
            Some(name) => name,
            None => self.next_slot_name()?,
        };
        let path = self.config.slot_dir.join(name.as_str());
        if path.exists() {
            return Err(Error::PathExists(path.into()));
        }
        let ran = self.git().run([
            OsStr::new("worktree"),
            OsStr::new("add"),
            OsStr::new("--detach"),
            path.as_os_str(),
            OsStr::new("HEAD"),
        ])?;
        if !ran.success && !self.has_a_commit()? {
            // git's own `invalid reference: HEAD` names no fix.
            return Err(Error::NoCommits(self.config.foreground.to_path_buf()));
        }
        ran.forward_stderr(report);
        if !ran.success {
            return Err(Error::WorktreeAdd(path.into()));
        }
        self.copy_into(&path, failures, report);
        self.run_on_new_slot(&path, failures, report);
        Ok(path)
    }

    /// `true` when the foreground has a commit checked out for a new slot to start from. An
    /// unborn branch, as in a repository without commits, has none.
    fn has_a_commit(&self) -> Result<bool, Error> {
        Ok(self
            .git()
            .run(["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])?
            .success)
    }

    /// Copies each `copy` file that `slot` lacks from the main worktree, when git
    /// ignores it there. A path git does not ignore, or one that is not a regular
    /// file, is skipped with a warning, so the slot stays clean and no symlink is
    /// followed. A file the main worktree does not have is skipped quietly.
    pub(super) fn copy_into(
        &self,
        slot: &AbsolutePath,
        failures: &mut Vec<Error>,
        report: &mut impl Report,
    ) {
        let copy = &self.config.slot_setup.copy;
        if copy.is_empty() {
            return;
        }
        let Some(main) = &self.config.main_worktree else {
            failures.push(Error::NoMainWorktree(slot.to_path_buf()));
            return;
        };
        for path in copy {
            if let Err(error) = copy::copy_path(main, slot, path, report) {
                failures.push(error);
            }
        }
    }

    /// Runs `on_new_slot` in `slot` through the platform shell. Its standard output
    /// goes to standard error too, as `wtpool bg` prints the slot's path on standard
    /// output.
    fn run_on_new_slot(
        &self,
        slot: &AbsolutePath,
        failures: &mut Vec<Error>,
        report: &mut impl Report,
    ) {
        let Some(line) = &self.config.slot_setup.on_new_slot else {
            return;
        };
        match run::run(run::shell(line).current_dir(slot)) {
            Ok(ran) => {
                if !ran.stdout.is_empty() {
                    report.err(&ran.stdout);
                }
                ran.forward_stderr(report);
                if !ran.success {
                    failures.push(Error::OnNewSlot {
                        command: line.clone(),
                        slot: slot.to_path_buf(),
                        code: ran.code,
                    });
                }
            }
            Err(error) => failures.push(error),
        }
    }
}

/// Fails with every failure of `copy` and `on_new_slot`, once the command has done the
/// rest.
pub(super) fn slot_setup(failures: Vec<Error>) -> Result<(), Error> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::SlotSetup { failures })
    }
}
