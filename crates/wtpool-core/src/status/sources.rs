//! [`Sources`]: where `wtpool status` reads the time, the process table, the machine's name,
//! this user's heavy jobs and the name of the repository on its forge.

use std::num::NonZeroUsize;
use std::path::PathBuf;

use crate::clock::{Clock, SystemClock};
use crate::jobs::JobPaths;
use crate::processes::{self, ProcessTable, SystemProcesses};

/// Where `wtpool status` reads the time, the process table, the machine's name, this user's
/// heavy jobs and the name of the repository on its forge.
pub struct Sources<C = Clock, P = ProcessTable> {
    pub clock: C,
    pub processes: P,
    /// The host name, `localhost` when the system does not tell it.
    pub machine: String,
    /// `None` without a folder for this user's job settings.
    pub jobs: Option<JobPaths>,
    /// The cores the default limit of heavy jobs is a quarter of.
    pub cores: usize,
    /// The repository a remote's URL names on the forge, as
    /// [`Forge::repository`](crate::Forge::repository) reads it.
    pub forge_repository: fn(&str) -> Option<String>,
}

/// The machine a document describes, and where this user's heavy jobs are on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub name: String,
    pub jobs: Option<JobPaths>,
    pub cores: usize,
}

impl<C, P> Sources<C, P> {
    #[must_use]
    pub fn host(&self) -> Host {
        Host {
            name: self.machine.clone(),
            jobs: self.jobs.clone(),
            cores: self.cores,
        }
    }
}

impl Sources {
    /// The system clock, process table, host name and cores, the job settings of the user this
    /// process's environment names, and the forge's names of `forge_repository`.
    #[must_use]
    pub fn system(forge_repository: fn(&str) -> Option<String>) -> Self {
        Self {
            clock: Clock::System(SystemClock),
            processes: ProcessTable::System(SystemProcesses),
            machine: processes::host_name()
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "localhost".to_owned()),
            jobs: JobPaths::from_env(|name| std::env::var_os(name)).ok(),
            cores: std::thread::available_parallelism().map_or(1, NonZeroUsize::get),
            forge_repository,
        }
    }

    /// [`Sources::system`], with the clock read from the file `clock` and the process table from
    /// the file `processes` where they are named.
    #[must_use]
    pub fn from_files(
        clock: Option<PathBuf>,
        processes: Option<PathBuf>,
        forge_repository: fn(&str) -> Option<String>,
    ) -> Self {
        Self {
            clock: Clock::from_file(clock),
            processes: ProcessTable::from_file(processes),
            ..Self::system(forge_repository)
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
        let files = Sources::from_files(Some(clock), Some(processes), |url| Some(url.to_owned()));
        let system = Sources::from_files(None, None, |_| None);

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
        assert_eq!(
            (files.forge_repository)("acme/app").as_deref(),
            Some("acme/app")
        );
        assert_eq!((system.forge_repository)("acme/app"), None);
    }
}
