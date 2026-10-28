//! [`HeavyTable`]: the processes of a process table that a heavy pattern names, and the live job
//! that holds each.

use std::collections::{HashMap, HashSet};

use super::record::JobRecord;
use crate::heavy::{CommandWords, HeavyList};
use crate::processes::{MAX_ANCESTORS, RunningProcess};

/// The name a heavy pattern gives a process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeavyName {
    /// The command's words the pattern matched.
    pub name: String,
    /// Only a suggestion names it, so it shows as a build and nothing flags it.
    pub suggested: bool,
}

/// The heavy processes of a process table, and the job processes among them.
#[derive(Debug)]
pub struct HeavyTable<'a> {
    table: &'a [RunningProcess],
    parents: HashMap<u32, u32>,
    names: HashMap<u32, HeavyName>,
    /// The job each job process runs.
    jobs: HashMap<u32, &'a JobRecord>,
}

impl<'a> HeavyTable<'a> {
    /// The processes of `table` that `heavy` names, or else `suggested`, and those that run a
    /// job of `jobs`.
    #[must_use]
    pub fn new(
        table: &'a [RunningProcess],
        heavy: &HeavyList,
        suggested: &HeavyList,
        jobs: &[&'a JobRecord],
    ) -> Self {
        let named_by = |list: &HeavyList, words: &CommandWords| {
            (!list.patterns().is_empty())
                .then(|| list.name_of(words))
                .flatten()
        };
        let names = table
            .iter()
            .filter_map(|process| {
                let words = CommandWords::of_process(&process.name, &process.cmd);
                let name = match named_by(heavy, &words) {
                    Some(name) => HeavyName {
                        name,
                        suggested: false,
                    },
                    None => HeavyName {
                        name: named_by(suggested, &words)?,
                        suggested: true,
                    },
                };
                Some((process.pid, name))
            })
            .collect();
        let jobs = table
            .iter()
            .filter_map(|process| {
                let job = jobs.iter().find(|job| runs(job, process))?;
                Some((process.pid, *job))
            })
            .collect();
        Self {
            table,
            parents: table
                .iter()
                .filter_map(|process| Some((process.pid, process.parent?)))
                .collect(),
            names,
            jobs,
        }
    }

    /// Every process of the table.
    #[must_use]
    pub fn processes(&self) -> &'a [RunningProcess] {
        self.table
    }

    /// The name a pattern gives the process `pid`, if any does.
    #[must_use]
    pub fn name(&self, pid: u32) -> Option<&HeavyName> {
        self.names.get(&pid)
    }

    /// The job whose process is `pid` or the nearest of its ancestors.
    #[must_use]
    pub fn holding_job(&self, pid: u32) -> Option<&'a JobRecord> {
        std::iter::once(pid)
            .chain(self.ancestors(pid))
            .find_map(|pid| self.jobs.get(&pid).copied())
    }

    /// The pids of the ancestors of `pid` in the table, nearest first.
    pub fn ancestors(&self, pid: u32) -> impl Iterator<Item = u32> + '_ {
        let mut next = self.parents.get(&pid).copied();
        let mut seen = HashSet::from([pid]);
        std::iter::from_fn(move || {
            let pid = next.filter(|pid| seen.insert(*pid))?;
            next = self.parents.get(&pid).copied();
            Some(pid)
        })
        .take(MAX_ANCESTORS)
    }

    /// The outermost processes a listed pattern names that no job holds, newest first: none of
    /// their ancestors is such a process or a job's.
    #[must_use]
    pub fn outside(&self) -> Vec<&'a RunningProcess> {
        let listed = |pid: u32| self.names.get(&pid).is_some_and(|name| !name.suggested);
        let mut outside: Vec<&RunningProcess> = self
            .table
            .iter()
            .filter(|process| listed(process.pid) && self.holding_job(process.pid).is_none())
            .filter(|process| self.ancestors(process.pid).all(|pid| !listed(pid)))
            .collect();
        outside.sort_by_key(|process| std::cmp::Reverse((process.started, process.pid)));
        outside
    }
}

/// `true` when `process` is the one `job` runs in: the same pid, and the same start where both
/// tell one.
fn runs(job: &JobRecord, process: &RunningProcess) -> bool {
    let same_start = match (job.started, process.started) {
        (Some(recorded), Some(listed)) => recorded == listed.second(),
        _ => true,
    };
    job.pid == process.pid && same_start
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::clock::Timestamp;
    use crate::heavy::Pattern;
    use crate::jobs::JobState;
    use crate::processes::StartTime;

    fn process(pid: u32, parent: u32, cmd: &[&str], started: &str) -> RunningProcess {
        RunningProcess {
            pid,
            parent: Some(parent),
            started: StartTime::parse(started),
            name: cmd.first().copied().unwrap_or_default().to_owned(),
            cwd: PathBuf::from("/w"),
            cmd: cmd.iter().map(|word| (*word).to_owned()).collect(),
        }
    }

    fn job(pid: u32, started: Option<&str>) -> JobRecord {
        JobRecord {
            started: started.and_then(Timestamp::parse),
            state: JobState::Running,
            ..JobRecord::sample(pid)
        }
    }

    fn list(patterns: &[&str]) -> HeavyList {
        let patterns: Vec<Pattern> = patterns
            .iter()
            .map(|p| Pattern::parse(p).unwrap())
            .collect();
        HeavyList::new(&patterns, &[], &[])
    }

    #[test]
    fn heavy_table__should_name_by_the_list_then_the_suggestions() {
        // Given
        let at = "2026-10-20T11:00:00Z";
        let table = [
            process(10, 1, &["cargo", "test"], at),
            process(11, 1, &["make", "-j4"], at),
            process(12, 1, &["ls"], at),
        ];

        // When
        let heavy = HeavyTable::new(&table, &list(&["cargo test"]), &list(&["make"]), &[]);

        // Then
        let names: Vec<Option<(&str, bool)>> = [10, 11, 12]
            .iter()
            .map(|pid| heavy.name(*pid).map(|n| (n.name.as_str(), n.suggested)))
            .collect();
        assert_eq!(
            names,
            vec![Some(("cargo test", false)), Some(("make", true)), None]
        );
    }

    #[test]
    fn heavy_table_holding_job__should_find_the_nearest_job_with_its_start() {
        // Given
        let at = "2026-10-20T11:00:00Z";
        let table = [
            process(20, 1, &["wtpool", "job", "--", "make"], at),
            process(21, 20, &["make"], at),
            process(22, 21, &["make", "-C", "sub"], at),
            process(30, 1, &["wtpool", "job", "--", "make"], at),
            process(31, 30, &["make"], at),
        ];
        let jobs = [job(20, Some(at)), job(30, Some("2026-10-20T10:00:00Z"))];
        let jobs: Vec<&JobRecord> = jobs.iter().collect();

        // When
        let heavy = HeavyTable::new(&table, &list(&["make"]), &list(&[]), &jobs);

        // Then
        let holders: Vec<Option<u32>> = [20, 21, 22, 31]
            .iter()
            .map(|pid| Some(heavy.holding_job(*pid)?.pid))
            .collect();
        assert_eq!(holders, vec![Some(20), Some(20), Some(20), None]);
    }

    #[test]
    fn heavy_table_outside__should_list_outermost_listed_processes_newest_first() {
        // Given
        let table = [
            process(10, 1, &["make", "all"], "2026-10-20T11:00:00Z"),
            process(11, 10, &["make", "-C", "sub"], "2026-10-20T11:00:01Z"),
            process(20, 1, &["npm", "test"], "2026-10-20T11:00:02Z"),
            process(21, 20, &["make"], "2026-10-20T11:00:03Z"),
            process(30, 1, &["cargo", "build"], "2026-10-20T11:00:04Z"),
        ];

        // When
        let heavy = HeavyTable::new(
            &table,
            &list(&["make", "cargo build"]),
            &list(&["npm test"]),
            &[],
        );

        // Then
        let pids: Vec<u32> = heavy.outside().iter().map(|p| p.pid).collect();
        assert_eq!(pids, vec![30, 21, 10]);
    }

    #[test]
    fn heavy_table__should_stop_at_a_loop_of_parents() {
        // Given
        let at = "2026-10-20T11:00:00Z";
        let table = [process(5, 6, &["make"], at), process(6, 5, &["zsh"], at)];

        // When
        let heavy = HeavyTable::new(&table, &list(&["make"]), &list(&[]), &[]);

        // Then
        assert_eq!(heavy.ancestors(5).collect::<Vec<_>>(), vec![6]);
        assert_eq!(heavy.outside().len(), 1);
    }
}
