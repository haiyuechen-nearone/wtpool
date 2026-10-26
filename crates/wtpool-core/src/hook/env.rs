//! The inputs of a hook run besides its payload, and the facts of its own process (H19, H24).

use std::path::PathBuf;

use super::log;
use super::record::HarnessProcess;
use crate::clock::{self, Clock};
use crate::fs::home_folder;
use crate::processes::{ListProcesses, ProcessQuery, ProcessTable, StartTime};

/// What a hook run reads besides its input.
#[derive(Debug, Clone)]
pub struct HookEnv {
    pub clock: Clock,
    pub processes: ProcessTable,
    /// `WTPOOL_VERIFY_SANDBOX` is `1`.
    pub sandbox: bool,
    pub home: Option<PathBuf>,
    pub tz: Option<String>,
    /// When `main` began, in microseconds.
    pub entered_micros: i64,
    /// The folder of the hook's log (H23), `None` without a usable one.
    pub log: Option<PathBuf>,
}

/// The facts of this hook process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Me {
    /// The start time of this process in microseconds (H24).
    pub started_micros: i64,
    /// The parent, unless it is pid 0 or 1 or its start time is unknown (H19).
    pub parent: Option<HarnessProcess>,
}

impl HookEnv {
    /// The environment of this process, the clock and process table read from the named
    /// files when they are given.
    #[must_use]
    pub fn from_process(clock: Option<PathBuf>, processes: Option<PathBuf>) -> Self {
        let home = home_folder(std::env::var_os("HOME"), std::env::home_dir);
        let state_home = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from);
        Self {
            clock: Clock::from_file(clock),
            processes: ProcessTable::from_file(processes),
            sandbox: std::env::var_os("WTPOOL_VERIFY_SANDBOX").is_some_and(|v| v == "1"),
            log: log::log_dir(state_home.as_deref(), home.as_deref()),
            home,
            tz: std::env::var("TZ").ok(),
            entered_micros: clock::system_micros(),
        }
    }

    /// The facts of this process from its process table, the start being when `main` began
    /// unless the table tells it.
    #[must_use]
    pub fn me(&self) -> Me {
        let this = self.processes.this_process();
        let started_micros = this
            .as_ref()
            .and_then(|this| this.started)
            .map_or(self.entered_micros, StartTime::micros);
        let parent = this
            .and_then(|this| this.parent)
            .filter(|ppid| *ppid > 1)
            .and_then(|ppid| harness_process(&self.processes, ppid));
        Me {
            started_micros,
            parent,
        }
    }
}

/// The process `pid` with its start time, `None` while the table does not tell it.
fn harness_process(table: &impl ListProcesses, pid: u32) -> Option<HarnessProcess> {
    let pids = [pid];
    let listed = table.list(&ProcessQuery::only(&pids).with_started());
    let parent = listed.into_iter().rfind(|process| process.pid == pid)?;
    let started = parent.started?.second().to_rfc3339();
    Some(HarnessProcess { pid, started })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::processes::{Liveness, RecordedProcess};

    fn env_of(lines: &[String]) -> (tempfile::TempDir, HookEnv) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("procs.jsonl");
        std::fs::write(&path, lines.join("\n")).unwrap();
        let mut env = HookEnv::from_process(None, Some(path));
        env.entered_micros = 42;
        (dir, env)
    }

    fn line(pid: u32, ppid: u32, started: &str) -> String {
        format!(r#"{{"pid":{pid},"ppid":{ppid},"started":"{started}"}}"#)
    }

    #[rstest]
    #[case::listed_parent(7001, Some(7001))]
    #[case::orphan(1, None)]
    #[case::unknown_parent(7099, None)]
    #[case::parent_without_start(7002, None)]
    fn me__should_read_the_parent_from_the_table(#[case] ppid: u32, #[case] parent: Option<u32>) {
        // Given
        let own = line(std::process::id(), ppid, "2026-10-20T12:00:00.000005Z");
        let harness = line(7001, 1, "2026-10-20T11:00:00Z");
        let unstarted = r#"{"pid":7002,"ppid":1}"#.to_owned();
        let (_dir, env) = env_of(&[harness, unstarted, own]);

        // When
        let me = env.me();

        // Then
        assert_eq!(me.started_micros, 1_792_497_600_000_005);
        let expected = parent.map(|pid| HarnessProcess {
            pid,
            started: "2026-10-20T11:00:00Z".to_owned(),
        });
        assert_eq!(me.parent, expected);
    }

    #[test]
    fn me__should_use_the_entry_time_when_unlisted() {
        // Given
        let (_dir, env) = env_of(&[line(7001, 1, "2026-10-20T11:00:00Z")]);

        // When
        let me = env.me();

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
        // Given
        let mut env = HookEnv::from_process(None, None);
        env.entered_micros = 42;

        // When
        let me = env.me();

        // Then
        assert_eq!(me.started_micros, 42);
        let parent = me.parent.expect("the test runner has a parent");
        let pids = [parent.pid];
        let table = env
            .processes
            .list(&ProcessQuery::only(&pids).with_started());
        let recorded = RecordedProcess {
            pid: parent.pid,
            started: crate::clock::Timestamp::parse(&parent.started),
        };
        assert!(Liveness::SameStart.runs(Some(recorded), &table));
    }
}
