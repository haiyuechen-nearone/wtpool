//! The pool state `wtpool status --watch` keeps up to date: the reads each change asks for, the
//! git refresh after a call that edits files, and the timers of the actors.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::Sources;
use super::document::Document;
use super::state::PoolState;
use super::watch::{FolderChange, JobFolders, Pending, Track};
use crate::clock::ReadClock;
use crate::processes::ListProcesses;

/// The shortest time between two git refreshes of a worktree after edits.
const EDIT_REFRESH: Duration = Duration::from_secs(1);

/// The time between two reads of the process table while heavy processes outside a job are
/// flagged: no file changes when such a process starts.
const PROCESS_SAMPLE: Duration = Duration::from_secs(1);

/// A [`PoolState`] refreshed with the process table and clock of `sources`.
pub struct Live<'a, C, P> {
    state: PoolState,
    sources: &'a Sources<C, P>,
    /// The last git refresh of each worktree after an edit.
    edited: BTreeMap<PathBuf, Instant>,
    /// The worktrees whose refresh after an edit waits, each with the time it is due.
    waiting: BTreeMap<PathBuf, Instant>,
    folders: Option<JobFolders>,
    /// The last read of the process table on [`PROCESS_SAMPLE`]'s timer.
    sampled: Option<Instant>,
}

impl<'a, C, P> Live<'a, C, P> {
    pub fn new(state: PoolState, sources: &'a Sources<C, P>) -> Self {
        Self {
            state,
            sources,
            edited: BTreeMap::new(),
            waiting: BTreeMap::new(),
            folders: None,
            sampled: None,
        }
    }

    /// Follows the job folders too, and each heartbeat watches those made since.
    #[must_use]
    pub fn with_folders(self, folders: JobFolders) -> Self {
        Self {
            folders: Some(folders),
            ..self
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

    /// The time the process table is next read on its own, at once before its first read. `None`
    /// while nothing outside a job is flagged, or past the range of [`Instant`].
    fn next_sample(&self, now: Instant) -> Option<Instant> {
        if !self.state.flags_outside() {
            return None;
        }
        self.sampled
            .map_or(Some(now), |last| last.checked_add(PROCESS_SAMPLE))
    }
}

impl<C: ReadClock, P: ListProcesses> Track for Live<'_, C, P> {
    /// A failed listing keeps the state as it was, and the next heartbeat tries again.
    fn refresh_all(&mut self) {
        if let Some(folders) = &mut self.folders {
            folders.watch_new();
        }
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
        let maybe_made = pending.machine_config || pending.jobs_changed();
        if let Some(folders) = self
            .folders
            .as_mut()
            .filter(|folders| maybe_made && folders.waits())
        {
            folders.watch_new();
        }
        if pending.machine_config {
            self.state.refresh_machine_config();
        }
        if pending.repository_config {
            self.state.refresh_repository_config();
        }
        if pending.jobs_changed() {
            self.state
                .refresh_jobs(&job_names(pending), &self.sources.processes);
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
        let sample = self.next_sample(now);
        self.waiting
            .values()
            .copied()
            .chain(timer)
            .chain(sample)
            .min()
    }

    fn classify(&self, path: &Path) -> Vec<FolderChange> {
        self.folders
            .as_ref()
            .map_or_else(Vec::new, |folders| folders.classify(path))
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
        if self.next_sample(now).is_some_and(|at| at <= now) {
            self.state.refresh_processes(&self.sources.processes);
            self.sampled = Some(now);
        }
    }
}

/// The names [`PoolState::refresh_jobs`] reads again: the ids of the changed jobs, or else an
/// empty name, read as every job.
fn job_names(pending: &Pending) -> BTreeSet<OsString> {
    if pending.all_jobs {
        return BTreeSet::from([OsString::new()]);
    }
    pending
        .jobs
        .iter()
        .map(|id| OsString::from(id.to_string()))
        .collect()
}

#[cfg(test)]
mod sample_tests;
#[cfg(test)]
mod tests;
