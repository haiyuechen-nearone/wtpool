//! `wtpool status`: the pool's worktrees, git facts, builds and activity as one JSON document,
//! once (`--json`) or per change (`--watch`).

mod agents;
mod builds;
mod document;
mod facts;
#[cfg(test)]
mod fixture;
mod live;
mod outputs;
mod record_time;
mod records;
mod repository;
mod schema;
mod sources;
mod state;
mod watch;

use std::path::{Path, PathBuf};

pub use crate::activity::{PlacedBy, WaitReason};
pub use document::{
    Action, Activity, Actor, ActorKind, ActorState, Agents, Build, Document, DocumentKind,
    DocumentVersion, GitStatus, Machine, OperationName, PoolStatus, Reason, Repository,
    WorktreeStatus,
};
pub use record_time::RecordTime;
pub use schema::{ToSchema, status_schema, status_schema_text};
pub use sources::Sources;
pub use watch::watch as watch_status;

use crate::clock::ReadClock;
use crate::config::Config;
use crate::error::Error;
use crate::paths;
use crate::processes::ListProcesses;
use state::PoolState;

/// `wtpool status --json`: the document of the pool of `config`, whose common git directory is
/// `common`, as one line.
pub fn status_line<C: ReadClock, P: ListProcesses>(
    config: Config,
    common: PathBuf,
    sources: &Sources<C, P>,
) -> Result<String, Error> {
    let state = PoolState::load(config, common, &sources.processes, sources.machine.clone())?;
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
    let state = PoolState::load(config, common, &sources.processes, sources.machine.clone())?;
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
