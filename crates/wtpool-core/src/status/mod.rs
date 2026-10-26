//! `wtpool status`: the pool's worktrees, git facts, builds and activity as one JSON document,
//! once (`--json`) or per change (`--watch`).

mod agents;
mod document;
mod facts;
#[cfg(test)]
mod fixture;
mod live;
mod outputs;
mod procs;
mod records;
mod repository;
mod schema;
mod state;
mod time;
mod watch;

use std::path::{Path, PathBuf};

pub use crate::activity::{PlacedBy, WaitReason};
pub use document::{
    Action, Activity, Actor, ActorKind, ActorState, Agents, Build, Document, DocumentKind,
    DocumentVersion, GitStatus, Machine, OperationName, PoolStatus, Reason, Repository,
    WorktreeStatus,
};
pub use procs::{ListProcesses, Process, ProcessFile, SystemProcesses};
pub use schema::{ToSchema, status_schema, status_schema_text};
pub use time::{FileClock, ReadClock, RecordTime, SystemClock, Timestamp};
pub use watch::watch as watch_status;

use crate::config::Config;
use crate::error::Error;
use crate::paths;
use state::PoolState;

/// Where `wtpool status` reads the time and the process table.
pub struct Sources {
    pub clock: Box<dyn ReadClock>,
    pub processes: Box<dyn ListProcesses>,
}

impl Sources {
    /// The system clock and process table.
    #[must_use]
    pub fn system() -> Self {
        Self {
            clock: Box::new(SystemClock),
            processes: Box::new(SystemProcesses),
        }
    }

    /// The system's sources, with the clock read from the file `clock` and the process table
    /// from the file `processes` where they are named.
    #[must_use]
    pub fn from_files(clock: Option<PathBuf>, processes: Option<PathBuf>) -> Self {
        let mut sources = Self::system();
        if let Some(path) = clock {
            sources.clock = Box::new(FileClock::new(path));
        }
        if let Some(path) = processes {
            sources.processes = Box::new(ProcessFile::new(path));
        }
        sources
    }
}

/// `wtpool status --json`: the document of the pool of `config`, whose common git directory is
/// `common`, as one line.
pub fn status_line(config: Config, common: PathBuf, sources: &Sources) -> Result<String, Error> {
    let state = PoolState::load(config, common, sources.processes.as_ref())?;
    let document = state.document(sources.clock.now());
    let mut line = serde_json::to_string(&document).unwrap_or_default();
    line.push('\n');
    Ok(line)
}

/// Per path of `paths`, the actors of that worktree as the document of the pool of `config`,
/// whose common git directory is `common`, shows them at the time of `sources`.
pub(crate) fn actors_at(
    config: Config,
    common: PathBuf,
    sources: &Sources,
    paths: &[&Path],
) -> Result<Vec<Vec<Actor>>, Error> {
    let state = PoolState::load(config, common, sources.processes.as_ref())?;
    let document = state.document(sources.clock.now());
    let worktrees: Vec<WorktreeStatus> = document
        .pools
        .into_iter()
        .flat_map(|pool| pool.worktrees)
        .collect();
    Ok(paths
        .iter()
        .map(|path| {
            worktrees
                .iter()
                .filter(|worktree| paths::same_path(Path::new(&worktree.path), path))
                .flat_map(|worktree| worktree.actors.iter().cloned())
                .collect()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AbsolutePath;
    use crate::status::fixture::must_fixture;

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

        // When
        let files = Sources::from_files(Some(clock), Some(processes));
        let system = Sources::from_files(None, None);

        // Then
        assert_eq!(files.clock.now(), Timestamp::from_seconds(1_792_497_600));
        assert_eq!(files.processes.processes().len(), 1);
        assert!(system.clock.now() >= before);
        let pid = std::process::id();
        assert!(system.processes.processes().iter().any(|p| p.pid == pid));
    }

    #[test]
    fn status_line__should_print_one_document_line() {
        // Given
        let fixture = must_fixture();

        // When
        let line = status_line(fixture.config(), fixture.common.clone(), &Sources::system());

        // Then
        let line = line.unwrap();
        assert!(
            line.ends_with("}\n") && line.matches('\n').count() == 1,
            "{line}"
        );
        let document: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            document["pools"][0]["worktrees"].as_array().unwrap().len(),
            3
        );
    }

    #[test]
    fn status_line__should_fail_when_the_foreground_is_no_worktree() {
        // Given
        let fixture = must_fixture();
        let mut config = fixture.config();
        config.foreground = AbsolutePath::try_new(&fixture.root).unwrap();

        // When
        let line = status_line(config, fixture.common.clone(), &Sources::system());

        // Then
        assert!(line.is_err());
    }
}
