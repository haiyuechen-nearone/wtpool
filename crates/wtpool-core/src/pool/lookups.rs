//! Finding worktrees of the pool: by path or slot name, by the branch they hold, and the words
//! for one whose directory is gone.

use std::path::Path;

use crate::error::Error;
use crate::github::ViewPr;
use crate::names::{BranchName, SlotName};
use crate::paths;
use crate::report::{Discard, Report};
use crate::run::Git;
use crate::worktree::{Head, Worktree};

use super::Pool;

impl<V: ViewPr> Pool<V> {
    pub(crate) fn git(&self) -> Git {
        Git::at(&self.config.foreground)
    }

    pub(super) fn is_foreground(&self, path: &Path) -> bool {
        paths::same_path(path, &self.config.foreground)
    }

    pub(crate) fn worktrees(&self, report: &mut impl Report) -> Result<Vec<Worktree>, Error> {
        Worktree::list(&self.config.foreground, report)
    }

    /// The listed worktree at path `arg`, or the slot named `arg`. Any other
    /// directory, such as one inside a worktree, matches nothing.
    pub(super) fn at_path_or_slot<'a>(
        &self,
        worktrees: &'a [Worktree],
        arg: &str,
    ) -> Option<&'a Worktree> {
        let at = |path: &Path| worktrees.iter().find(|w| paths::same_path(w.path(), path));
        if let Some(worktree) = at(Path::new(arg)) {
            return Some(worktree);
        }
        let name = SlotName::try_new(arg).ok()?;
        at(&self.config.slot_dir.join(name.as_str()))
    }

    /// The managed slots whose directory exists. Warns about the others.
    pub(super) fn managed_slots(&self, report: &mut impl Report) -> Result<Vec<Worktree>, Error> {
        let mut slots = Vec::new();
        for worktree in self.worktrees(report)? {
            if self.is_foreground(worktree.path())
                || !self.config.slot_prefix.manages(&worktree.name())
            {
                continue;
            }
            if worktree.is_gone() {
                report.warn(&self.gone_warning(&worktree));
            } else {
                slots.push(worktree);
            }
        }
        Ok(slots)
    }

    pub(crate) fn gone_error(&self, worktree: &Worktree) -> Error {
        let remove = self.remove_arg(worktree);
        match worktree.head() {
            Head::Branch(branch) => Error::HolderGone {
                holder: worktree.name(),
                branch: branch.clone(),
                remove,
            },
            Head::Detached | Head::Unknown => Error::WorktreeGone {
                worktree: worktree.name(),
                remove,
            },
        }
    }

    pub(super) fn gone_warning(&self, worktree: &Worktree) -> String {
        Error::WorktreeGone {
            worktree: worktree.name(),
            remove: self.remove_arg(worktree),
        }
        .to_string()
    }

    /// The argument of `wtpool rm` for `worktree`: its slot name when that finds it,
    /// its path otherwise.
    fn remove_arg(&self, worktree: &Worktree) -> String {
        let name = worktree.name();
        let by_name = SlotName::try_new(&name).is_ok_and(|slot| {
            paths::same_path(&self.config.slot_dir.join(slot.as_str()), worktree.path())
        });
        if by_name {
            name
        } else {
            worktree.path().display().to_string()
        }
    }

    /// The worktree whose directory exists and whose `HEAD` is `branch`.
    pub(crate) fn holder_of(&self, branch: &BranchName) -> Result<Option<Worktree>, Error> {
        Ok(self
            .any_holder_of(branch)?
            .filter(|holder| !holder.is_gone()))
    }

    /// The worktree whose `HEAD` is `branch`, preferring one whose directory exists.
    pub(super) fn any_holder_of(&self, branch: &BranchName) -> Result<Option<Worktree>, Error> {
        let (live, gone): (Vec<Worktree>, Vec<Worktree>) = self
            .worktrees(&mut Discard)?
            .into_iter()
            .filter(|worktree| worktree.holds_branch(branch))
            .partition(|worktree| !worktree.is_gone());
        Ok(live.into_iter().next().or_else(|| gone.into_iter().next()))
    }
}
