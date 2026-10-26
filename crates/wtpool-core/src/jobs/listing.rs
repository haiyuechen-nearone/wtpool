//! [`JobListing`]: the limit, the policy and the heavy patterns, then the running jobs, the
//! waiting ones in queue order and the heavy processes outside any job.

use std::path::PathBuf;

use serde::Serialize;

use super::JobPaths;
use super::config::{LIMIT_BOUNDS, MachineConfig};
use super::heavy_table::HeavyTable;
use super::machine::MachineJobs;
use super::record::JobRecord;
use crate::clock::Timestamp;
use crate::heavy::{HeavyList, RepositoryPatterns, Suggestion, SuggestionJson, Suggestions};
use crate::processes::{ListProcesses, ProcessQuery};

/// What `wtpool job ls` shows. The job folder is only read, and a dead job's record skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobListing {
    /// The defaults while [`error`](Self::error) names a fault of the machine config.
    pub config: MachineConfig,
    /// Why the machine config or the job folder cannot be read.
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

    /// One line of JSON, the fields in the listing's order.
    #[must_use]
    pub fn to_json(&self) -> String {
        fn entries(jobs: &[ListedJob]) -> Vec<EntryJson<'_>> {
            jobs.iter()
                .map(|job| EntryJson {
                    id: &job.id,
                    summary: &job.summary,
                    label: job.label.as_deref(),
                    folder: job.folder.to_string_lossy().into_owned(),
                    since: job.since,
                })
                .collect()
        }
        let document = ListingJson {
            limit: self.limit(),
            default: self.config.default_limit.get(),
            min: LIMIT_BOUNDS.0,
            max: LIMIT_BOUNDS.1,
            enforce: self.config.enforce.as_str(),
            source: SourceJson {
                limit: self.config.limit_source.as_str(),
                enforce: self.config.enforce_source.as_str(),
            },
            config: self.config.path.to_string_lossy().into_owned(),
            error: self.error.as_deref(),
            heavy: self
                .heavy
                .patterns()
                .iter()
                .map(|listed| HeavyJson {
                    pattern: listed.pattern.as_str(),
                    source: listed.source.as_str(),
                })
                .collect(),
            suggested: self.suggested.iter().map(SuggestionJson::from).collect(),
            running: entries(&self.running),
            waiting: entries(&self.waiting),
            outside: entries(&self.outside),
        };
        // A document of strings and numbers always serializes; `{}` keeps the line JSON.
        serde_json::to_string(&document).unwrap_or_else(|_| "{}".to_owned()) + "\n"
    }
}

#[derive(Serialize)]
struct ListingJson<'a> {
    limit: Option<u32>,
    default: u32,
    min: u32,
    max: u32,
    enforce: &'a str,
    source: SourceJson,
    config: String,
    error: Option<&'a str>,
    heavy: Vec<HeavyJson<'a>>,
    suggested: Vec<SuggestionJson<'a>>,
    running: Vec<EntryJson<'a>>,
    waiting: Vec<EntryJson<'a>>,
    outside: Vec<EntryJson<'a>>,
}

#[derive(Serialize)]
struct SourceJson {
    limit: &'static str,
    enforce: &'static str,
}

#[derive(Serialize)]
struct HeavyJson<'a> {
    pattern: &'a str,
    source: &'static str,
}

#[derive(Serialize)]
struct EntryJson<'a> {
    id: &'a str,
    summary: &'a str,
    label: Option<&'a str>,
    folder: String,
    since: Timestamp,
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
