//! [`Liveness`]: the two rules for counting the process a record names as running.

use super::{RunningProcess, StartTime};
use crate::clock::Timestamp;

/// The process a record names: its agent harness's pid and start time (H19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordedProcess {
    pub pid: u32,
    /// `None` when the record holds no start that the rule reads.
    pub started: Option<Timestamp>,
}

/// A rule for counting the process a record names as running, by the process table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// Only while the table lists the pid with the start the record holds. A record without a
    /// process or a start, and a pid listed without a start or with another, count as dead, so
    /// a reused pid never keeps a record. gc deletes by it (H21).
    SameStart,
    /// Unless the table shows the process gone: its pid unlisted, or listed with another start
    /// where the record and the table both hold one. A record without a process cannot be found
    /// dead. `wtpool status` shows actors by it (A1).
    UnlessGone,
}

impl Liveness {
    /// `true` when `process` runs among `table` by this rule.
    #[must_use]
    pub fn runs(self, process: Option<RecordedProcess>, table: &[RunningProcess]) -> bool {
        let Some(process) = process else {
            return self == Self::UnlessGone;
        };
        let mut starts = table
            .iter()
            .filter(|running| running.pid == process.pid)
            .map(|running| running.started.map(StartTime::second));
        match self {
            Self::SameStart => {
                process.started.is_some() && starts.any(|started| started == process.started)
            }
            Self::UnlessGone => starts.any(|started| {
                started.is_none() || process.started.is_none() || started == process.started
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rstest::rstest;

    use super::*;

    const START: i64 = 1_792_494_000;

    fn listed(pid: u32, started: Option<i64>) -> RunningProcess {
        RunningProcess {
            pid,
            parent: Some(1),
            started: started
                .map(Timestamp::from_seconds)
                .and_then(StartTime::from_second),
            name: "claude".to_owned(),
            cwd: PathBuf::from("/"),
        }
    }

    fn recorded(started: Option<i64>) -> Option<RecordedProcess> {
        Some(RecordedProcess {
            pid: 7001,
            started: started.map(Timestamp::from_seconds),
        })
    }

    #[rstest]
    #[case::same_start(recorded(Some(START)), Some(START), true, true)]
    #[case::reused_pid(recorded(Some(START)), Some(START + 60), false, false)]
    #[case::record_without_start(recorded(None), Some(START), false, true)]
    #[case::table_without_start(recorded(Some(START)), None, false, true)]
    #[case::no_process(None, Some(START), false, true)]
    fn liveness_runs__should_follow_each_rule(
        #[case] process: Option<RecordedProcess>,
        #[case] running: Option<i64>,
        #[case] same_start: bool,
        #[case] unless_gone: bool,
    ) {
        // Given
        let table = [listed(7002, Some(START)), listed(7001, running)];

        // When
        let runs = (
            Liveness::SameStart.runs(process, &table),
            Liveness::UnlessGone.runs(process, &table),
        );

        // Then
        assert_eq!(runs, (same_start, unless_gone));
    }

    #[rstest]
    #[case::same_start(Liveness::SameStart)]
    #[case::unless_gone(Liveness::UnlessGone)]
    fn liveness_runs__should_find_an_unlisted_pid_dead(#[case] rule: Liveness) {
        // Given
        let table = [listed(7002, Some(START))];

        // When
        let runs = rule.runs(recorded(Some(START)), &table);

        // Then
        assert!(!runs);
    }
}
