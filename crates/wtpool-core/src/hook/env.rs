//! The environment of a hook run, and the facts of its own process.

use std::ffi::OsString;
use std::path::PathBuf;

use super::log;
use super::record::HarnessProcess;
use crate::clock::{Clock, ReadClock};
use crate::platform::dirs;
use crate::processes::{ListProcesses, ProcessQuery, ProcessTable, StartTime};

/// The inputs of a hook run besides its payload.
#[derive(Debug, Clone)]
pub struct HookEnv {
    pub clock: Clock,
    pub processes: ProcessTable,
    /// `true` when `WTPOOL_VERIFY_SANDBOX` is `1`.
    pub sandbox: bool,
    pub home: Option<PathBuf>,
    pub tz: Option<String>,
    /// The time `main` began, in microseconds.
    pub entered_micros: i64,
    /// The folder of the hook's log, `None` without a usable one.
    pub log: Option<PathBuf>,
}

/// The facts of this hook process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Me {
    /// The start time of this process, in microseconds.
    pub started_micros: i64,
    /// The parent, unless it is pid 0 or 1 or its start time is unknown.
    pub parent: Option<HarnessProcess>,
}

impl HookEnv {
    /// [`Self::from_env`] of this process's environment.
    #[must_use]
    pub fn from_process(clock: Option<PathBuf>, processes: Option<PathBuf>) -> Self {
        Self::from_env(clock, processes, |name| std::env::var_os(name))
    }

    /// The environment `var` reads, the clock and process table read from the named files when
    /// they are given.
    #[must_use]
    pub fn from_env(
        clock: Option<PathBuf>,
        processes: Option<PathBuf>,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Self {
        let home = dirs::home_from(&var);
        let state_home = var("XDG_STATE_HOME").map(PathBuf::from);
        let clock = Clock::from_file(clock);
        Self {
            processes: ProcessTable::from_file(processes),
            sandbox: var("WTPOOL_VERIFY_SANDBOX").is_some_and(|v| v == "1"),
            log: log::log_dir(state_home.as_deref(), home.as_deref()),
            home,
            tz: var("TZ").and_then(|tz| tz.into_string().ok()),
            entered_micros: clock.system_micros(),
            clock,
        }
    }

    /// The facts of this process from its process table. The start is
    /// [`Self::entered_micros`] when the table does not tell it.
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
    fn from_env__should_read_the_variables_it_is_given() {
        // Given
        let var = |name: &str| {
            let value = match name {
                "HOME" => "/h",
                "TZ" => "Europe/Zurich",
                "WTPOOL_VERIFY_SANDBOX" => "1",
                _ => return None,
            };
            Some(OsString::from(value))
        };

        // When
        let env = HookEnv::from_env(None, None, var);

        // Then
        assert_eq!(env.home, Some(PathBuf::from("/h")));
        assert_eq!(env.tz.as_deref(), Some("Europe/Zurich"));
        assert!(env.sandbox);
        assert_eq!(env.log, Some(PathBuf::from("/h/.local/state/wtpool")));
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
