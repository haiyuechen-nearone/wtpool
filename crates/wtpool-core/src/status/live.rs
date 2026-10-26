//! The pool state `wtpool status --watch` keeps up to date: the reads each change asks for, the
//! git refresh after a call that edits files, and the timers of the actors.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::Sources;
use super::document::Document;
use super::state::PoolState;
use super::watch::{Pending, Track};
use crate::clock::ReadClock;
use crate::processes::ListProcesses;

/// The shortest time between two git refreshes of a worktree after edits.
const EDIT_REFRESH: Duration = Duration::from_secs(1);

/// A [`PoolState`] refreshed with the process table and clock of `sources`.
pub struct Live<'a, C, P> {
    state: PoolState,
    sources: &'a Sources<C, P>,
    /// The last git refresh of each worktree after an edit.
    edited: BTreeMap<PathBuf, Instant>,
    /// The worktrees whose refresh after an edit waits, each with the time it is due.
    waiting: BTreeMap<PathBuf, Instant>,
}

impl<'a, C, P> Live<'a, C, P> {
    pub fn new(state: PoolState, sources: &'a Sources<C, P>) -> Self {
        Self {
            state,
            sources,
            edited: BTreeMap::new(),
            waiting: BTreeMap::new(),
        }
    }

    /// Refreshes the git facts of `worktree` after an edit at `now`, or schedules the refresh
    /// [`EDIT_REFRESH`] after the last one.
    fn edit(&mut self, worktree: PathBuf, now: Instant) {
        let next = self
            .edited
            .get(&worktree)
            .and_then(|last| last.checked_add(EDIT_REFRESH));
        match next {
            Some(next) if now < next => {
                self.waiting.insert(worktree, next);
            }
            _ => {
                self.state.refresh_worktree(&worktree);
                self.waiting.remove(&worktree);
                self.edited.insert(worktree, now);
            }
        }
    }
}

impl<C: ReadClock, P: ListProcesses> Track for Live<'_, C, P> {
    /// A failed listing keeps the state as it was, and the next heartbeat tries again.
    fn refresh_all(&mut self) {
        let _ = self.state.refresh_all(&self.sources.processes);
    }

    fn refresh(&mut self, pending: &Pending, now: Instant) {
        if pending.all || (pending.worktrees && self.state.git_dirs_changed()) {
            self.refresh_all();
            return;
        }
        let mut unknown_git_dir = false;
        for git_dir in &pending.git_dirs {
            unknown_git_dir |= !self.state.refresh_git_dir(git_dir);
        }
        if unknown_git_dir {
            self.refresh_all();
            return;
        }
        if pending.refs {
            self.state.refresh_refs();
        }
        if pending.rescan_records {
            self.state.rescan_records();
        } else {
            for name in &pending.records {
                let edited = self.state.refresh_record(name, &self.sources.processes);
                if let Some(worktree) = edited {
                    self.edit(worktree, now);
                }
            }
        }
    }

    fn document(&self) -> Document {
        self.state.document(self.sources.clock.now())
    }

    /// A timer past the range of [`Instant`] never comes.
    fn deadline(&self, now: Instant) -> Option<Instant> {
        let clock = &self.sources.clock;
        let timer = self
            .state
            .next_change(clock.now())
            .and_then(|at| now.checked_add(clock.until(at)));
        self.waiting.values().copied().chain(timer).min()
    }

    fn tick(&mut self, now: Instant) {
        let due: Vec<PathBuf> = self
            .waiting
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(worktree, _)| worktree.clone())
            .collect();
        for worktree in due {
            self.edit(worktree, now);
        }
    }
}

#[cfg(test)]
mod tests;
