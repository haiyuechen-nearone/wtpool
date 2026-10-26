//! [`Sources`]: where `wtpool status` reads the time, the process table and the machine's name.

use std::path::PathBuf;

use crate::clock::{Clock, SystemClock};
use crate::processes::{self, ProcessTable, SystemProcesses};

/// Where `wtpool status` reads the time, the process table and the machine's name.
pub struct Sources<C = Clock, P = ProcessTable> {
    pub clock: C,
    pub processes: P,
    /// The host name, `localhost` when the system does not tell it.
    pub machine: String,
}

impl Sources {
    /// The system clock, process table and host name.
    #[must_use]
    pub fn system() -> Self {
        Self {
            clock: Clock::System(SystemClock),
            processes: ProcessTable::System(SystemProcesses),
            machine: processes::host_name()
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "localhost".to_owned()),
        }
    }

    /// The system's sources, with the clock read from the file `clock` and the process table
    /// from the file `processes` where they are named.
    #[must_use]
    pub fn from_files(clock: Option<PathBuf>, processes: Option<PathBuf>) -> Self {
        Self {
            clock: Clock::from_file(clock),
            processes: ProcessTable::from_file(processes),
            ..Self::system()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{ReadClock, SystemClock, Timestamp};
    use crate::processes::{ListProcesses, ProcessQuery};

    #[test]
    fn from_files__should_read_the_named_files_else_the_system() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let (clock, processes) = (dir.path().join("clock"), dir.path().join("procs"));
        std::fs::write(&clock, "2026-10-20T12:00:00Z").unwrap();
        std::fs::write(
            &processes,
            "{\"pid\":7,\"name\":\"cargo\",\"cwd\":\"/p\"}\n",
        )
        .unwrap();
        let before = SystemClock.now();
        let query = ProcessQuery::all().with_name().with_cwd();

        // When
        let files = Sources::from_files(Some(clock), Some(processes));
        let system = Sources::from_files(None, None);

        // Then
        assert_eq!(files.clock.now(), Timestamp::from_seconds(1_792_497_600));
        assert_eq!(files.processes.list(&query).len(), 1);
        assert!(system.clock.now() >= before);
        let pid = std::process::id();
        assert!(system.processes.list(&query).iter().any(|p| p.pid == pid));
        let host = processes::host_name().filter(|name| !name.is_empty());
        assert_eq!(
            files.machine,
            host.unwrap_or_else(|| "localhost".to_owned())
        );
        assert_eq!(system.machine, files.machine);
    }
}
