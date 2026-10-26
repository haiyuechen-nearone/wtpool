//! The processes a hook asks about: itself, its parent, the agent harness (H19), and whether the
//! process a record names still lives (H21). Read from the system, or in a `verify` build from
//! a file of JSON lines with `pid`, `ppid`, `started`, `name` and `cwd`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

use super::clock;
use super::record::Process;

/// Where the process facts come from.
#[derive(Debug, Clone)]
pub enum ProcessTable {
    System,
    /// The `WTPOOL_VERIFY_PROCS` file.
    File(PathBuf),
}

/// One line of the process table file.
#[derive(Debug, Clone, Deserialize)]
struct Line {
    pid: u32,
    #[serde(default)]
    ppid: Option<u32>,
    #[serde(default)]
    started: Option<String>,
}

/// The facts of this hook process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Me {
    /// The start time of this process in microseconds (H24).
    pub started_micros: i64,
    /// The parent, unless it is pid 1 or its start time is unknown (H19).
    pub parent: Option<Process>,
}

impl ProcessTable {
    /// The facts of this process, `entered_micros` being when `main` began, the start time
    /// used when the table does not list this process.
    #[must_use]
    pub fn me(&self, entered_micros: i64) -> Me {
        let pid = std::process::id();
        match self {
            Self::File(path) => {
                let lines = read_lines(path);
                let own = lines.get(&pid);
                let started_micros = own
                    .and_then(|line| line.started.as_deref())
                    .and_then(clock::parse_micros)
                    .unwrap_or(entered_micros);
                let parent = own
                    .and_then(|line| line.ppid)
                    .filter(|ppid| *ppid != 1)
                    .and_then(|ppid| {
                        let started = lines.get(&ppid)?.started.as_deref()?;
                        Some(Process {
                            pid: ppid,
                            started: clock::rfc3339(clock::parse_seconds(started)?),
                        })
                    });
                Me {
                    started_micros,
                    parent,
                }
            }
            Self::System => Me {
                started_micros: entered_micros,
                parent: system_parent(),
            },
        }
    }

    /// The processes among `pids` that live now, by pid with their start time. Only those
    /// pids are looked up, so the cost follows the records that name them, not the machine.
    #[must_use]
    pub fn living(&self, pids: &[u32]) -> BTreeMap<u32, String> {
        match self {
            Self::File(path) => read_lines(path)
                .into_iter()
                .filter(|(pid, _)| pids.contains(pid))
                .filter_map(|(pid, line)| {
                    let seconds = clock::parse_seconds(line.started.as_deref()?)?;
                    Some((pid, clock::rfc3339(seconds)))
                })
                .collect(),
            Self::System => system_living(pids),
        }
    }
}

/// Whether `process` is among `living`, with the same start time.
#[must_use]
pub fn is_alive(process: Option<&Process>, living: &BTreeMap<u32, String>) -> bool {
    process.is_some_and(|process| living.get(&process.pid) == Some(&process.started))
}

fn read_lines(path: &std::path::Path) -> BTreeMap<u32, Line> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Line>(line).ok())
        .map(|line| (line.pid, line))
        .collect()
}

/// The process table's facts of `pid` alone: its name, parent and start time.
fn system_process(pid: sysinfo::Pid) -> sysinfo::System {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut system = System::new();
    let refresh = ProcessRefreshKind::nothing().without_tasks();
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, refresh);
    system
}

fn system_parent() -> Option<Process> {
    let ppid = parent_pid().filter(|ppid| *ppid > 1)?;
    let pid = sysinfo::Pid::from_u32(ppid);
    let started = system_process(pid).process(pid)?.start_time();
    Some(Process {
        pid: ppid,
        started: clock::rfc3339(i64::try_from(started).ok()?),
    })
}

/// The pid of this process's parent.
#[cfg(unix)]
fn parent_pid() -> Option<u32> {
    Some(std::os::unix::process::parent_id())
}

/// The pid of this process's parent, from the process table.
#[cfg(not(unix))]
fn parent_pid() -> Option<u32> {
    let me = sysinfo::Pid::from_u32(std::process::id());
    system_process(me)
        .process(me)?
        .parent()
        .map(sysinfo::Pid::as_u32)
}

fn system_living(pids: &[u32]) -> BTreeMap<u32, String> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    if pids.is_empty() {
        return BTreeMap::new();
    }
    let pids: Vec<Pid> = pids.iter().copied().map(Pid::from_u32).collect();
    let mut system = System::new();
    let refresh = ProcessRefreshKind::nothing().without_tasks();
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&pids), true, refresh);
    system
        .processes()
        .values()
        .filter_map(|process| {
            let started = i64::try_from(process.start_time()).ok()?;
            Some((process.pid().as_u32(), clock::rfc3339(started)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn table(lines: &[String]) -> (tempfile::TempDir, ProcessTable) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("procs.jsonl");
        std::fs::write(&path, lines.join("\n")).unwrap();
        (dir, ProcessTable::File(path))
    }

    fn line(pid: u32, ppid: u32, started: &str) -> String {
        format!(r#"{{"pid":{pid},"ppid":{ppid},"started":"{started}","name":"x","cwd":"/"}}"#)
    }

    #[rstest]
    #[case::listed_parent(7001, Some(7001))]
    #[case::orphan(1, None)]
    #[case::unknown_parent(7099, None)]
    fn me__should_read_the_parent_from_the_table(#[case] ppid: u32, #[case] parent: Option<u32>) {
        // Given
        let own = line(std::process::id(), ppid, "2026-10-20T12:00:00.000005Z");
        let (_dir, table) = table(&[line(7001, 1, "2026-10-20T11:00:00Z"), own]);

        // When
        let me = table.me(42);

        // Then
        assert_eq!(me.started_micros, 1_792_497_600_000_005);
        assert_eq!(me.parent.as_ref().map(|p| p.pid), parent);
        if parent.is_some() {
            assert_eq!(me.parent.unwrap().started, "2026-10-20T11:00:00Z");
        }
    }

    #[test]
    fn me__should_use_the_entry_time_when_unlisted() {
        // Given
        let (_dir, table) = table(&[line(7001, 1, "2026-10-20T11:00:00Z")]);

        // When
        let me = table.me(42);

        // Then
        assert_eq!(
            me,
            Me {
                started_micros: 42,
                parent: None
            }
        );
    }

    #[test]
    fn me__should_read_the_system() {
        // When
        let me = ProcessTable::System.me(42);

        // Then
        assert_eq!(me.started_micros, 42);
        let parent = me.parent.expect("the test runner has a parent");
        let living = ProcessTable::System.living(&[parent.pid, std::process::id()]);
        assert!(is_alive(Some(&parent), &living));
        assert!(living.contains_key(&std::process::id()));
    }

    #[rstest]
    #[case::same_start("2026-10-20T11:00:00Z", true)]
    #[case::reused_pid("2026-10-20T13:00:00Z", false)]
    fn is_alive__should_compare_pid_and_start(#[case] started: &str, #[case] alive: bool) {
        // Given
        let (_dir, table) = table(&[line(7001, 1, "2026-10-20T11:00:00Z")]);
        let process = Process {
            pid: 7001,
            started: started.to_owned(),
        };

        // When
        let found = is_alive(Some(&process), &table.living(&[7001]));

        // Then
        assert_eq!(found, alive);
        assert!(!is_alive(None, &table.living(&[7001])));
        assert!(table.living(&[7002]).is_empty());
    }
}
