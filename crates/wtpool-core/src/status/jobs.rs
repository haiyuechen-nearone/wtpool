//! The heavy jobs in `wtpool status`: `machine.jobs`, the worktree and actor each job and heavy
//! process belongs to, and their effect on the builds and actors of the pool.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::agents::ActorJob;
use super::document::{HeavyEntry, JobEntry, JobSources, Jobs, SuggestionEntry};
use super::records::ActorRecord;
use crate::activity::CallKind;
use crate::clock::Timestamp;
use crate::error::Error;
use crate::heavy::{HeavyList, Pattern, RepositoryPatterns, Suggestion, Suggestions};
use crate::jobs::{
    Enforce, HeavyTable, JobId, JobPaths, JobRecord, JobState, LIMIT_BOUNDS, MachineJobs,
};
use crate::paths::comparable;
use crate::processes::{Liveness, RunningProcess};

/// This user's jobs and the heavy patterns of the repository, as last read.
#[derive(Debug, Clone)]
pub struct JobsState {
    /// `None` without a folder for this user's job settings.
    pub machine: Option<MachineJobs>,
    pub repository: RepositoryPatterns,
    pub suggested: Vec<Suggestion>,
    /// The folder whose names suggest patterns: the main worktree, else the foreground.
    pub markers: PathBuf,
}

impl JobsState {
    /// The jobs in `paths`, on a machine of `cores` cores, the heavy patterns of the repository
    /// whose common git directory is `common`, and the suggestions of the names in `markers`.
    pub fn read(
        paths: Option<JobPaths>,
        cores: usize,
        common: &Path,
        markers: PathBuf,
    ) -> Result<Self, Error> {
        Ok(Self {
            machine: paths.map(|paths| MachineJobs::read(paths, cores)),
            repository: RepositoryPatterns::read(common)?,
            suggested: Suggestions::read(&markers).found,
            markers,
        })
    }

    /// Reads the machine config, the jobs, the repository's patterns and its suggestions
    /// again.
    pub fn refresh_all(&mut self, common: &Path) {
        self.refresh_repository_config(common);
        self.suggested = Suggestions::read(&self.markers).found;
        if let Some(machine) = &mut self.machine {
            machine.read_config();
            machine.read_jobs();
        }
    }

    pub fn refresh_machine_config(&mut self) {
        if let Some(machine) = &mut self.machine {
            machine.read_config();
        }
    }

    /// Reads the repository's `wtpool.toml` in `common` again. A file that cannot be read keeps
    /// the patterns known before.
    pub fn refresh_repository_config(&mut self, common: &Path) {
        if let Ok(patterns) = RepositoryPatterns::read(common) {
            self.repository = patterns;
        }
    }

    /// Reads again the jobs whose files are called `stems`, every job for a name that is no
    /// job's.
    pub fn refresh_jobs(&mut self, stems: &BTreeSet<OsString>) {
        let Some(machine) = &mut self.machine else {
            return;
        };
        let ids: Option<Vec<JobId>> = stems
            .iter()
            .map(|stem| stem.to_str().and_then(JobId::parse))
            .collect();
        match ids {
            Some(ids) => ids.iter().for_each(|id| machine.read_job(id)),
            None => machine.read_jobs(),
        }
    }

    /// `true` while the machine config flags heavy processes outside a job.
    #[must_use]
    pub fn flags_outside(&self) -> bool {
        self.machine
            .as_ref()
            .is_some_and(|machine| machine.config().enforce != Enforce::Off)
    }

    /// The machine's patterns, then the repository's.
    #[must_use]
    pub fn heavy(&self) -> HeavyList {
        let machine = self
            .machine
            .as_ref()
            .map_or(&[][..], |jobs| jobs.config().heavy.as_slice());
        HeavyList::new(
            machine,
            &self.repository.heavy,
            &self.repository.build_processes,
        )
    }

    /// The suggestions that `heavy` lacks. They name builds while the repository lists no
    /// pattern of its own.
    #[must_use]
    pub fn build_suggestions(&self, heavy: &HeavyList) -> HeavyList {
        if !self.repository.is_empty() {
            return HeavyList::default();
        }
        let patterns: Vec<Pattern> = self
            .suggested
            .iter()
            .filter(|suggestion| !heavy.contains(&suggestion.pattern))
            .map(|suggestion| suggestion.pattern.clone())
            .collect();
        HeavyList::new(&patterns, &[], &[])
    }

    /// The live jobs, each in its own order.
    #[must_use]
    pub fn records(&self) -> Vec<&JobRecord> {
        self.machine
            .as_ref()
            .map(MachineJobs::records)
            .unwrap_or_default()
    }
}

/// The worktrees of the pool: each one's path as git lists it, and with symlinks resolved.
pub struct PoolFolders(pub Vec<(String, PathBuf)>);

impl PoolFolders {
    /// The path of the worktree that holds `folder`, the deepest one when worktrees nest.
    #[must_use]
    pub fn worktree_of(&self, folder: &Path) -> Option<String> {
        let folder = comparable(folder);
        self.0
            .iter()
            .filter(|(_, root)| folder.starts_with(root))
            .max_by_key(|(_, root)| root.components().count())
            .map(|(path, _)| path.clone())
    }
}

/// The actors of the pool that show, by the harness process their records name.
pub struct Harnesses<'a> {
    records: &'a [&'a ActorRecord],
}

impl<'a> Harnesses<'a> {
    /// `records`: those of the pool that show, neither ended, dead nor replaced.
    #[must_use]
    pub fn new(records: &'a [&'a ActorRecord]) -> Self {
        Self { records }
    }

    /// The key of the actor the process `pid`, named `summary`, belongs to: of the actors whose
    /// harness process is the nearest of its ancestors, the one with a shell call in flight on
    /// `summary`, the newest first, else the session.
    #[must_use]
    pub fn actor_of(&self, pid: u32, summary: &str, table: &HeavyTable<'_>) -> Option<String> {
        let by_pid: BTreeMap<u32, &RunningProcess> = table
            .processes()
            .iter()
            .map(|process| (process.pid, process))
            .collect();
        table.ancestors(pid).find_map(|ancestor| {
            let process = by_pid.get(&ancestor)?;
            let on_it: Vec<&ActorRecord> = self
                .records
                .iter()
                .copied()
                .filter(|record| is_harness(record, process))
                .collect();
            choose(&on_it, summary).map(|record| record.key.to_string())
        })
    }
}

/// `true` when `process` is the harness process `record` names, by [`Liveness::UnlessGone`].
fn is_harness(record: &ActorRecord, process: &RunningProcess) -> bool {
    record.process.is_some()
        && Liveness::UnlessGone.runs(record.process, std::slice::from_ref(process))
}

/// Of the actors of one harness process, the one with a shell call in flight on `summary`, the
/// newest first, else the session.
fn choose<'a>(records: &[&'a ActorRecord], summary: &str) -> Option<&'a ActorRecord> {
    let calling = records
        .iter()
        .copied()
        .filter(|record| {
            record
                .in_flight
                .iter()
                .any(|call| call.kind == CallKind::Shell)
                && record
                    .action
                    .as_ref()
                    .is_some_and(|action| action.target == summary)
        })
        .max_by(|a, b| {
            a.since
                .at()
                .cmp(&b.since.at())
                .then_with(|| b.key.cmp(&a.key))
        });
    calling.or_else(|| {
        records
            .iter()
            .copied()
            .filter(|record| !record.key.is_subagent())
            .max_by_key(|record| record.updated.at())
            .or_else(|| records.first().copied())
    })
}

/// The jobs of each actor of `owners`, the actor of each job of `records` by its place there.
#[must_use]
pub fn actor_jobs(records: &[&JobRecord], owners: &[Option<String>]) -> BTreeMap<String, ActorJob> {
    let mut queue: Vec<(&JobRecord, &str)> = records
        .iter()
        .zip(owners)
        .filter_map(|(record, owner)| Some((*record, owner.as_deref()?)))
        .collect();
    queue.sort_by_key(|(record, _)| record.ticket);
    let mut jobs: BTreeMap<String, ActorJob> = BTreeMap::new();
    for (record, owner) in queue {
        let job = jobs.entry(owner.to_owned()).or_default();
        match record.state {
            JobState::Running => job.running = true,
            JobState::Waiting => {
                if job.waiting.is_none() {
                    job.waiting = Some(
                        record
                            .label
                            .clone()
                            .unwrap_or_else(|| record.summary.clone()),
                    );
                }
            }
        }
    }
    jobs
}

/// The facts `machine.jobs` takes besides the state.
pub struct JobsFacts<'a> {
    pub heavy: &'a HeavyList,
    pub table: &'a HeavyTable<'a>,
    /// The actor of each job of the state's records, in their order.
    pub owners: &'a [Option<String>],
    pub harnesses: &'a Harnesses<'a>,
    pub folders: &'a PoolFolders,
    pub now: Timestamp,
}

/// `machine.jobs` of `state`, `None` without a folder for this user's job settings.
#[must_use]
pub fn jobs_document(state: &JobsState, facts: &JobsFacts<'_>) -> Option<Jobs> {
    let machine = state.machine.as_ref()?;
    let config = machine.config();
    let owners: BTreeMap<&JobId, &str> = machine
        .records()
        .into_iter()
        .zip(facts.owners)
        .filter_map(|(record, owner)| Some((&record.id, owner.as_deref()?)))
        .collect();
    let entry = |record: &&JobRecord| JobEntry {
        id: record.id.to_string(),
        summary: record.summary.clone(),
        label: record.label.clone(),
        worktree: facts.folders.worktree_of(&record.cwd),
        actor: owners.get(&record.id).map(|owner| (*owner).to_owned()),
        since: record.since,
    };
    let (running, waiting) = machine.queue();
    Some(Jobs {
        limit: machine.limit(),
        default: config.default_limit.get(),
        min: LIMIT_BOUNDS.0,
        max: LIMIT_BOUNDS.1,
        enforce: config.enforce,
        source: JobSources {
            limit: config.limit_source,
            enforce: config.enforce_source,
        },
        config: config.path.to_string_lossy().into_owned(),
        error: machine.error(),
        heavy: facts
            .heavy
            .patterns()
            .iter()
            .map(|listed| HeavyEntry {
                pattern: listed.pattern.as_str().to_owned(),
                source: listed.source,
            })
            .collect(),
        suggested: state
            .suggested
            .iter()
            .filter(|suggestion| !facts.heavy.contains(&suggestion.pattern))
            .map(|suggestion| SuggestionEntry {
                pattern: suggestion.pattern.as_str().to_owned(),
                from: suggestion.from.clone(),
            })
            .collect(),
        running: running.iter().map(entry).collect(),
        waiting: waiting.iter().map(entry).collect(),
        outside: outside(facts),
    })
}

/// The outermost heavy processes no job holds, newest first, each with its worktree and actor.
fn outside(facts: &JobsFacts<'_>) -> Vec<JobEntry> {
    facts
        .table
        .outside()
        .into_iter()
        .map(|process| {
            let summary = facts
                .table
                .name(process.pid)
                .map(|name| name.name.clone())
                .unwrap_or_default();
            JobEntry {
                id: format!("process-{}", process.pid),
                actor: facts.harnesses.actor_of(process.pid, &summary, facts.table),
                summary,
                label: None,
                worktree: facts.folders.worktree_of(&process.cwd),
                since: process
                    .started
                    .map_or(facts.now, |started| started.second()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
