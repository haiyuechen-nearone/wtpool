//! The process port, [`ListProcesses`]: the hook (H19, H21) and `wtpool status` (S4, A1) read
//! the process table through it alone, the system's, or in a `verify` build a file of JSON lines
//! (`WTPOOL_VERIFY_PROCS`). [`Liveness`] names the two rules for counting the process a record
//! names as running.

mod file;
mod liveness;

use std::path::PathBuf;

pub use file::ProcessFile;
pub use liveness::{Liveness, RecordedProcess};

use crate::clock::Timestamp;
pub use crate::platform::processes::{SystemProcesses, host_name};

/// Lists the processes that run now.
pub trait ListProcesses: Send + Sync {
    /// The processes `query` names, with the facts it asks for, in no set order.
    fn list(&self, query: &ProcessQuery<'_>) -> Vec<RunningProcess>;

    /// This process with its parent, and with its start only where the table tells it to the
    /// microsecond (H24). `None` when the table does not list it.
    fn this_process(&self) -> Option<RunningProcess> {
        let pids = [std::process::id()];
        let query = ProcessQuery::only(&pids).with_parent().with_started();
        self.list(&query)
            .into_iter()
            .rfind(|process| pids.contains(&process.pid))
    }
}

/// The processes a listing reads, and the facts besides the pid it asks for. A table may pay for
/// each fact, so a listing gives only the facts asked for, `None` or empty for the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessQuery<'a> {
    /// The pids to look up, `None` for every process.
    pub pids: Option<&'a [u32]>,
    pub parent: bool,
    pub started: bool,
    pub name: bool,
    pub cwd: bool,
}

/// A process the table lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningProcess {
    pub pid: u32,
    /// The parent's pid, `None` when the table does not say.
    pub parent: Option<u32>,
    /// `None` when the table does not say.
    pub started: Option<StartTime>,
    pub name: String,
    /// The working folder, empty when the table does not say.
    pub cwd: PathBuf,
}

/// When a process started, in microseconds since the Unix epoch: the system's table tells the
/// second, the file of a `verify` build the microsecond.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StartTime(i64);

/// The process table of a run: the system's, or the file a `verify` build reads instead.
#[derive(Debug, Clone)]
pub enum ProcessTable {
    System(SystemProcesses),
    File(ProcessFile),
}

impl<'a> ProcessQuery<'a> {
    /// Every process, with no fact but its pid.
    #[must_use]
    pub const fn all() -> Self {
        Self {
            pids: None,
            parent: false,
            started: false,
            name: false,
            cwd: false,
        }
    }

    /// The processes among `pids` alone, so the cost follows them, not the machine.
    #[must_use]
    pub const fn only(pids: &'a [u32]) -> Self {
        Self {
            pids: Some(pids),
            ..Self::all()
        }
    }

    #[must_use]
    pub const fn with_parent(self) -> Self {
        Self {
            parent: true,
            ..self
        }
    }

    #[must_use]
    pub const fn with_started(self) -> Self {
        Self {
            started: true,
            ..self
        }
    }

    #[must_use]
    pub const fn with_name(self) -> Self {
        Self { name: true, ..self }
    }

    #[must_use]
    pub const fn with_cwd(self) -> Self {
        Self { cwd: true, ..self }
    }
}

impl StartTime {
    /// The start of `second`, `None` past the microseconds an `i64` holds.
    #[must_use]
    pub fn from_second(second: Timestamp) -> Option<Self> {
        second.seconds().checked_mul(1_000_000).map(Self)
    }

    /// Reads an RFC 3339 time such as `2026-10-20T12:00:00.000005Z`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let parsed: jiff::Timestamp = text.parse().ok()?;
        Some(Self(parsed.as_microsecond()))
    }

    /// The whole second, as [`Timestamp::parse`] gives it for the same text.
    #[must_use]
    pub fn second(self) -> Timestamp {
        Timestamp::from_seconds(self.0 / 1_000_000)
    }

    #[must_use]
    pub fn micros(self) -> i64 {
        self.0
    }
}

impl ProcessTable {
    /// The table `file` holds when it is given, else the system's.
    #[must_use]
    pub fn from_file(file: Option<PathBuf>) -> Self {
        file.map_or(Self::System(SystemProcesses), |path| {
            Self::File(ProcessFile::new(path))
        })
    }
}

impl ListProcesses for ProcessTable {
    fn list(&self, query: &ProcessQuery<'_>) -> Vec<RunningProcess> {
        match self {
            Self::System(table) => table.list(query),
            Self::File(table) => table.list(query),
        }
    }

    fn this_process(&self) -> Option<RunningProcess> {
        match self {
            Self::System(table) => table.this_process(),
            Self::File(table) => table.this_process(),
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::whole_second("2026-10-20T12:00:00Z", 1_792_497_600_000_000, 1_792_497_600)]
    #[case::micros("2026-10-20T12:00:00.000005Z", 1_792_497_600_000_005, 1_792_497_600)]
    #[case::before_the_epoch("1969-12-31T23:59:59.5Z", -500_000, 0)]
    fn start_time_parse__should_keep_the_microsecond_and_its_second(
        #[case] text: &str,
        #[case] micros: i64,
        #[case] second: i64,
    ) {
        // When
        let started = StartTime::parse(text).unwrap();

        // Then
        assert_eq!(started.micros(), micros);
        assert_eq!(Some(started.second()), Timestamp::parse(text));
        assert_eq!(started.second().seconds(), second);
    }

    #[rstest]
    #[case::in_range(1_792_497_600, Some(1_792_497_600_000_000))]
    #[case::past_the_range(i64::MAX, None)]
    fn start_time_from_second__should_start_at_the_second(
        #[case] second: i64,
        #[case] micros: Option<i64>,
    ) {
        // When
        let started = StartTime::from_second(Timestamp::from_seconds(second));

        // Then
        assert_eq!(started.map(StartTime::micros), micros);
        assert_eq!(StartTime::parse("noon"), None);
    }

    #[test]
    fn process_query__should_ask_for_the_named_facts() {
        // Given
        let pids = [7];

        // When
        let query = ProcessQuery::only(&pids)
            .with_parent()
            .with_started()
            .with_name()
            .with_cwd();

        // Then
        let expected = ProcessQuery {
            pids: Some(&pids[..]),
            parent: true,
            started: true,
            name: true,
            cwd: true,
        };
        assert_eq!(query, expected);
        assert_eq!(ProcessQuery::all().pids, None);
    }

    #[test]
    fn process_table__should_read_the_named_file_else_the_system() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("procs.jsonl");
        let own = format!(
            r#"{{"pid":{},"ppid":7001,"started":"2026-10-20T12:00:00.000005Z"}}"#,
            std::process::id()
        );
        std::fs::write(&path, own).unwrap();
        let query = ProcessQuery::all();

        // When
        let file = ProcessTable::from_file(Some(path));
        let system = ProcessTable::from_file(None);

        // Then
        assert_eq!(file.list(&query).len(), 1);
        let own = file.this_process().unwrap();
        assert_eq!(
            own.started.map(StartTime::micros),
            Some(1_792_497_600_000_005)
        );
        let pid = std::process::id();
        assert!(system.list(&query).iter().any(|process| process.pid == pid));
        assert_eq!(system.this_process().unwrap().started, None);
    }
}
