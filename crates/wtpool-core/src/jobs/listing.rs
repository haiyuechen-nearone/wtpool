//! [`JobListing`]: the limit, the policy and the heavy patterns, then the running jobs, the
//! waiting ones in queue order and the heavy processes outside any job.

use std::path::PathBuf;

use super::JobPaths;
use super::config::MachineConfig;
use super::heavy_table::HeavyTable;
use super::machine::MachineJobs;
use super::record::JobRecord;
use crate::clock::Timestamp;
use crate::heavy::{HeavyList, RepositoryPatterns, Suggestion, Suggestions};
use crate::processes::{ListProcesses, ProcessQuery};

/// The content of `wtpool job ls`. The job folder is only read, and a dead job's record skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobListing {
    /// The defaults while [`error`](Self::error) names a fault of the machine config.
    pub config: MachineConfig,
    /// The fault that keeps the machine config or the job folder from being read.
    pub error: Option<String>,
    config_valid: bool,
    pub heavy: HeavyList,
    pub suggested: Vec<Suggestion>,
    pub running: Vec<ListedJob>,
    /// In queue order.
    pub waiting: Vec<ListedJob>,
    /// The outermost heavy processes that no job holds, newest first.
    pub outside: Vec<ListedJob>,
}

/// A job, or a heavy process outside any job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedJob {
    pub id: String,
    pub summary: String,
    pub label: Option<String>,
    pub folder: PathBuf,
    /// When it started, or for a waiting job when it queued.
    pub since: Timestamp,
}

impl JobListing {
    /// The listing of the jobs in `paths` and of the heavy processes of `processes`, the heavy
    /// list the machine's patterns and `repository`'s, on a machine of `cores` cores, `now`.
    #[must_use]
    pub fn gather(
        paths: &JobPaths,
        cores: usize,
        repository: &RepositoryPatterns,
        suggested: Vec<Suggestion>,
        processes: &dyn ListProcesses,
        now: Timestamp,
    ) -> Self {
        let jobs = MachineJobs::read(paths.clone(), cores);
        let config = jobs.config().clone();
        let heavy = HeavyList::of(&config, repository);
        let records = jobs.records();
        let outside = outside_jobs(&heavy, &records, processes, now);
        let (running, waiting) = jobs.queue();
        let suggested = Suggestions::lacking_in(&suggested, &heavy)
            .cloned()
            .collect();
        Self {
            config,
            error: jobs.error(),
            config_valid: jobs.limit().is_some(),
            heavy,
            suggested,
            running: listed(&running),
            waiting: listed(&waiting),
            outside,
        }
    }

    /// The limit, `None` while the machine config is not valid.
    #[must_use]
    pub fn limit(&self) -> Option<u32> {
        self.config_valid.then_some(self.config.limit.get())
    }
}

/// `records` as the listing shows them, in their order.
fn listed(records: &[&JobRecord]) -> Vec<ListedJob> {
    records
        .iter()
        .map(|record| ListedJob {
            id: record.id.to_string(),
            summary: record.summary.clone(),
            label: record.label.clone(),
            folder: record.cwd.clone(),
            since: record.since,
        })
        .collect()
}

/// The outermost processes of `processes` that a pattern of `heavy` names and that neither are
/// nor descend from the process of a live job of `jobs`, newest first.
fn outside_jobs(
    heavy: &HeavyList,
    jobs: &[&JobRecord],
    processes: &dyn ListProcesses,
    now: Timestamp,
) -> Vec<ListedJob> {
    if heavy.patterns().is_empty() {
        return Vec::new();
    }
    let query = ProcessQuery::all()
        .with_parent()
        .with_started()
        .with_name()
        .with_cwd()
        .with_cmd();
    let table = processes.list(&query);
    let heavy_table = HeavyTable::new(&table, heavy, &HeavyList::default(), jobs);
    heavy_table
        .outside()
        .into_iter()
        .map(|process| ListedJob {
            id: format!("process-{}", process.pid),
            summary: heavy_table
                .name(process.pid)
                .map(|name| name.name.clone())
                .unwrap_or_default(),
            label: None,
            folder: process.cwd.clone(),
            since: process.started.map_or(now, |started| started.second()),
        })
        .collect()
}

#[cfg(test)]
mod tests;
