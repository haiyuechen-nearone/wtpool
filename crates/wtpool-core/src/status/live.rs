//! The pool state `wtpool status --watch` keeps up to date: what each change asks to read again,
//! the git refresh after an edit tool or Bash call (W4), and the timers of the actors (W6).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::Sources;
use super::document::Document;
use super::state::PoolState;
use super::watch::{Pending, Track};

/// The git facts of a worktree refresh after edits at most this often (W4).
const EDIT_REFRESH: Duration = Duration::from_secs(1);

/// A [`PoolState`] refreshed with the process table and clock of `sources`.
pub struct Live<'a> {
    state: PoolState,
    sources: &'a Sources,
    /// When each worktree's git last refreshed after an edit.
    edited: BTreeMap<PathBuf, Instant>,
    /// Worktrees whose refresh after an edit waits until the time given.
    waiting: BTreeMap<PathBuf, Instant>,
}

impl<'a> Live<'a> {
    pub fn new(state: PoolState, sources: &'a Sources) -> Self {
        Self {
            state,
            sources,
            edited: BTreeMap::new(),
            waiting: BTreeMap::new(),
        }
    }

    /// Refreshes the git facts of `worktree` after an edit, or schedules it a second after
    /// the last such refresh.
    fn edit(&mut self, worktree: PathBuf) {
        let now = Instant::now();
        match self.edited.get(&worktree) {
            Some(last) if now < *last + EDIT_REFRESH => {
                self.waiting.insert(worktree, *last + EDIT_REFRESH);
            }
            _ => {
                self.state.refresh_worktree(&worktree);
                self.waiting.remove(&worktree);
                self.edited.insert(worktree, now);
            }
        }
    }
}

impl Track for Live<'_> {
    /// A failed listing keeps what was known, and the next heartbeat tries again.
    fn refresh_all(&mut self) {
        let _ = self.state.refresh_all(self.sources.processes.as_ref());
    }

    fn refresh(&mut self, pending: &Pending) {
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
                let edited = self
                    .state
                    .refresh_record(name, self.sources.processes.as_ref());
                if let Some(worktree) = edited {
                    self.edit(worktree);
                }
            }
        }
    }

    fn document(&self) -> Document {
        self.state.document(self.sources.clock.now())
    }

    fn deadline(&self) -> Option<Instant> {
        let clock = &self.sources.clock;
        let timer = self
            .state
            .next_change(clock.now())
            .map(|at| Instant::now() + clock.until(at));
        self.waiting.values().copied().chain(timer).min()
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let due: Vec<PathBuf> = self
            .waiting
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(worktree, _)| worktree.clone())
            .collect();
        for worktree in due {
            self.edit(worktree);
        }
    }
}

#[cfg(test)]
mod tests;
