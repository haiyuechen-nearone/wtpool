//! [`Sources`]: where `wtpool status` reads the time, the process table, the machine's name,
//! this user's heavy jobs and the name of the repository on its forge.

use std::ffi::OsString;
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
    pub host: Host,
    /// The repository a remote's URL names on the forge, as
    /// [`Forge::repository`](crate::Forge::repository) reads it.
    pub forge_repository: fn(&str) -> Option<String>,
}

/// The machine a document describes, and where this user's heavy jobs are on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// The host name, `localhost` when the system does not tell it.
    pub name: String,
    /// `None` without a folder for this user's job settings.
    pub jobs: Option<JobPaths>,
    /// The cores the default limit of heavy jobs is a quarter of.
    pub cores: usize,
}

impl Host {
    /// The system's host name and cores, and the job settings of the user `var`'s environment
    /// names.
    #[must_use]
    pub fn system(var: impl Fn(&str) -> Option<OsString>) -> Self {
        Self {
            name: processes::host_name()
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "localhost".to_owned()),
            jobs: JobPaths::from_env(var).ok(),
            cores: std::thread::available_parallelism().map_or(1, NonZeroUsize::get),
        }
    }
}

impl Sources {
    /// The system clock and process table, the [`Host::system`] of `var`, and the forge's names
    /// of `forge_repository`.
    #[must_use]
    pub fn system(
        var: impl Fn(&str) -> Option<OsString>,
        forge_repository: fn(&str) -> Option<String>,
    ) -> Self {
        Self {
            clock: Clock::System(SystemClock),
            processes: ProcessTable::System(SystemProcesses),
            host: Host::system(var),
            forge_repository,
        }
    }

    /// [`Sources::system`], with the clock read from the file `clock` and the process table from
    /// the file `processes` where they are named.
    #[must_use]
    pub fn from_files(
        clock: Option<PathBuf>,
        processes: Option<PathBuf>,
        var: impl Fn(&str) -> Option<OsString>,
        forge_repository: fn(&str) -> Option<String>,
    ) -> Self {
        Self {
            clock: Clock::from_file(clock),
            processes: ProcessTable::from_file(processes),
            ..Self::system(var, forge_repository)
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
        let unset = |_: &str| None;

        // When
        let files = Sources::from_files(Some(clock), Some(processes), unset, |url| {
            Some(url.to_owned())
        });
        let system = Sources::from_files(None, None, unset, |_| None);

        // Then
        assert_eq!(files.clock.now(), Timestamp::from_seconds(1_792_497_600));
        assert_eq!(files.processes.list(&query).len(), 1);
        assert!(system.clock.now() >= before);
        let pid = std::process::id();
        assert!(system.processes.list(&query).iter().any(|p| p.pid == pid));
        assert_eq!(system.host, files.host);
        assert_eq!(
            (files.forge_repository)("acme/app").as_deref(),
            Some("acme/app")
        );
        assert_eq!((system.forge_repository)("acme/app"), None);
    }

    #[test]
    fn host_system__should_read_the_job_settings_from_var() {
        // Given
        let home = std::env::temp_dir().join("home");
        let var = |name: &str| (name == "HOME").then(|| home.clone().into_os_string());

        // When
        let host = Host::system(var);

        // Then
        let name = processes::host_name().filter(|name| !name.is_empty());
        assert_eq!(host.name, name.unwrap_or_else(|| "localhost".to_owned()));
        assert_eq!(host.jobs, JobPaths::from_env(var).ok());
        assert!(host.jobs.is_some_and(|jobs| jobs.config.starts_with(&home)));
        assert!(host.cores >= 1);
    }
}
