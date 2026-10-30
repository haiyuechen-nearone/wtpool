//! The sessions another agent started, found through the parents of the process table, and the
//! sessions a `wtpool` command counts as its own.

use std::collections::{BTreeMap, BTreeSet};

use super::records::{ActorRecord, RecordState};
use crate::processes::{Liveness, RunningProcess};

/// The levels above a process that a launcher, or the process of a named session, is looked for.
pub const MAX_LEVELS: usize = 16;

/// The command that runs `wtpool`: its ancestors, the parent first, and the session stem its
/// environment names.
#[derive(Debug, Clone, Copy, Default)]
pub struct Caller<'a> {
    pub ancestors: &'a [u32],
    pub named_session: Option<&'a str>,
}

/// The live records by the pid of their harness process, and the parents of the process table.
pub struct HarnessTree<'a> {
    parents: BTreeMap<u32, u32>,
    on: BTreeMap<u32, Vec<&'a ActorRecord>>,
}

impl<'a> HarnessTree<'a> {
    /// The tree of `processes` and of those of `records` that are neither ended nor of a gone
    /// process, so a pid that a dead record names counts as no harness.
    #[must_use]
    pub fn new(processes: &[RunningProcess], records: &[&'a ActorRecord]) -> Self {
        let parents = processes
            .iter()
            .filter_map(|process| Some((process.pid, process.parent?)))
            .collect();
        let mut on: BTreeMap<u32, Vec<&ActorRecord>> = BTreeMap::new();
        for record in records {
            if let Some(process) = record.process
                && record.state != RecordState::Ended
                && Liveness::UnlessGone.runs(record.process, processes)
            {
                on.entry(process.pid).or_default().push(record);
            }
        }
        Self { parents, on }
    }

    /// The pids of the harness processes of the tree's records.
    #[must_use]
    pub fn harness_pids(&self) -> BTreeSet<u32> {
        self.on.keys().copied().collect()
    }

    /// The launcher of each live session of `shown`, by the session's key: the key of the main
    /// session shown on the first harness process above its own, within [`MAX_LEVELS`] and below
    /// pid 1.
    /// A harness process that runs many sessions launches none, as it cannot tell which of them
    /// did.
    #[must_use]
    pub fn launchers(&self, shown: &[&ActorRecord]) -> BTreeMap<String, String> {
        shown
            .iter()
            .filter(|record| !record.key.is_subagent())
            .filter_map(|record| {
                let pid = record.process?.pid;
                self.on
                    .get(&pid)?
                    .iter()
                    .find(|live| live.key == record.key)?;
                let launcher = self.launcher(pid, shown)?;
                Some((record.key.to_string(), launcher))
            })
            .collect()
    }

    fn launcher(&self, pid: u32, shown: &[&ActorRecord]) -> Option<String> {
        let mut at = pid;
        for _ in 0..MAX_LEVELS {
            let parent = *self.parents.get(&at).filter(|&&parent| parent > 1)?;
            if let Some(records) = self.on.get(&parent) {
                if records.iter().any(|record| record.shared) {
                    return None;
                }
                return main_session_of(records, shown);
            }
            at = parent;
        }
        None
    }

    /// The sessions `caller` counts as its own, whose actors its check leaves out: when the
    /// nearest of its ancestors that a live record names is not shared, the sessions on it and
    /// the ones they launched; and the session `caller` names, when a shared process among its
    /// first [`MAX_LEVELS`] ancestors runs it.
    #[must_use]
    pub fn left_out(
        &self,
        caller: &Caller<'_>,
        launchers: &BTreeMap<String, String>,
    ) -> BTreeSet<String> {
        let mut sessions: BTreeSet<String> = caller
            .ancestors
            .iter()
            .find_map(|pid| self.on.get(pid))
            .filter(|records| records.iter().all(|record| !record.shared))
            .into_iter()
            .flatten()
            .map(|record| record.key.session().to_owned())
            .collect();
        let launched: Vec<String> = launchers
            .iter()
            .filter(|(_, launcher)| sessions.contains(*launcher))
            .map(|(session, _)| session.clone())
            .collect();
        sessions.extend(launched);
        if let Some(named) = caller
            .named_session
            .and_then(|stem| self.named(caller, stem))
        {
            sessions.insert(named);
        }
        sessions
    }

    fn named(&self, caller: &Caller<'_>, stem: &str) -> Option<String> {
        caller
            .ancestors
            .iter()
            .take(MAX_LEVELS)
            .filter_map(|pid| self.on.get(pid))
            .flatten()
            .find(|record| {
                record.shared && record.key.as_str() == stem && !record.key.is_subagent()
            })
            .map(|record| record.key.to_string())
    }
}

/// The key of the newest main session of `records` that `shown` holds.
fn main_session_of(records: &[&ActorRecord], shown: &[&ActorRecord]) -> Option<String> {
    records
        .iter()
        .filter(|record| {
            !record.key.is_subagent() && shown.iter().any(|other| other.key == record.key)
        })
        .max_by(|a, b| {
            a.updated
                .at()
                .cmp(&b.updated.at())
                .then_with(|| a.writer_started.cmp(&b.writer_started))
                .then_with(|| b.key.cmp(&a.key))
        })
        .map(|record| record.key.to_string())
}

#[cfg(test)]
mod tests;
