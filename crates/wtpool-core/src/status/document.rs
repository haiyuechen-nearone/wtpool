//! The document `wtpool status --json` prints and `wtpool status --watch` prints per change: one pool
//! of worktrees with their git facts, activity, actors and builds.

use serde::Serialize;

use super::record_time::RecordTime;
use crate::activity::{PlacedBy, WaitReason};
use crate::clock::Timestamp;
use crate::heavy::PatternSource;
use crate::jobs::{ConfigSource, Enforce};
use crate::worktree_state::Role;

/// One `wtpool status` document, `"type": "status"`, version 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Document {
    #[serde(rename = "type")]
    pub kind: DocumentKind,
    pub version: DocumentVersion,
    pub generated: Timestamp,
    pub machine: Machine,
    pub pools: Vec<PoolStatus>,
}

/// Serializes as `"status"`, the only document type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    #[default]
    Status,
}

/// Serializes as `1`, the only document version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocumentVersion;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Machine {
    pub name: String,
    /// `None` without a folder for this user's job settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jobs: Option<Jobs>,
}

/// The heavy jobs of this user and machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Jobs {
    /// `None` while [`error`](Self::error) names a fault of the machine config.
    pub limit: Option<u32>,
    pub default: u32,
    pub min: u32,
    pub max: u32,
    pub enforce: Enforce,
    pub source: JobSources,
    pub config: String,
    pub error: Option<String>,
    pub heavy: Vec<HeavyEntry>,
    pub suggested: Vec<SuggestionEntry>,
    pub running: Vec<JobEntry>,
    /// In queue order.
    pub waiting: Vec<JobEntry>,
    /// The outermost heavy processes no job holds, newest first.
    pub outside: Vec<JobEntry>,
}

/// Where the limit and the policy each come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct JobSources {
    pub limit: ConfigSource,
    pub enforce: ConfigSource,
}

/// A heavy pattern and the list it comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HeavyEntry {
    pub pattern: String,
    pub source: PatternSource,
}

/// A pattern suggested for this repository, and the marker or other source it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuggestionEntry {
    pub pattern: String,
    pub from: String,
}

/// A job, or a heavy process outside any job, and whose it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JobEntry {
    pub id: String,
    pub summary: String,
    pub label: Option<String>,
    /// The path of the worktree of this pool its folder lies in.
    pub worktree: Option<String>,
    /// The key of the actor of this pool it belongs to.
    pub actor: Option<String>,
    /// When it started, or for a waiting job when it queued.
    pub since: Timestamp,
}

/// The worktrees of one repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PoolStatus {
    pub common_dir: String,
    pub agents: Agents,
    pub skipped_records: u64,
    pub worktrees: Vec<WorktreeStatus>,
    pub repository: Repository,
}

/// The repository of a pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Repository {
    /// The main worktree's folder name.
    pub name: String,
    /// `<owner>/<repo>` on GitHub, read offline from the remote's URL.
    pub github: Option<String>,
}

/// [`Agents::On`] when `<common dir>/wtpool-activity/` exists, so hooks record agent activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Agents {
    On,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorktreeStatus {
    pub path: String,
    pub name: String,
    pub role: Role,
    pub missing: bool,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub git: GitStatus,
    pub free: bool,
    pub reasons: Vec<Reason>,
    pub activity: Activity,
    pub actors: Vec<Actor>,
    pub builds: Vec<Build>,
    pub last_event: Option<RecordTime>,
    pub last_change: Option<Timestamp>,
}

/// The git facts of one worktree. `None` counts and [`Self::unknown`] mean git could not tell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct GitStatus {
    pub dirty: Option<u64>,
    pub ahead: Option<u64>,
    pub behind: Option<u64>,
    pub unreferenced: Option<u64>,
    pub operation: Option<OperationName>,
    pub unknown: bool,
}

/// A cause that keeps `wtpool bg` from taking a worktree. Causes sort in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Foreground,
    Unmanaged,
    Branch,
    Dirty,
    Unreferenced,
    Operation,
    Unknown,
    Missing,
}

/// A worktree's activity or, without one, its availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    Missing,
    Waiting,
    Working,
    Building,
    Idle,
    Foreground,
    Free,
    NotReusable,
    Unknown,
    Unused,
}

/// A git operation stopped half way, as `wtpool ls` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationName {
    Rebase,
    Am,
    Merge,
    CherryPick,
    Revert,
    Bisect,
}

/// An agent session or subagent placed in a worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Actor {
    pub key: String,
    pub kind: ActorKind,
    pub label: String,
    pub state: ActorState,
    pub reason: Option<WaitReason>,
    pub action: Option<Action>,
    pub placed_by: PlacedBy,
    pub error: Option<String>,
    pub since: RecordTime,
    pub updated: RecordTime,
    /// The harness whose hooks wrote the actor's record, such as `claude-code`.
    pub harness: String,
    /// The actor's running background tasks, newest first, then the one that ended last.
    /// Omitted from the JSON when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<Output>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    Session,
    Subagent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorState {
    Working,
    Quiet,
    Waiting,
    Idle,
    Done,
}

/// A tool and its target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Action {
    pub tool: String,
    pub target: String,
}

/// A build process working in a worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Build {
    pub pid: u32,
    /// The words of the heavy pattern it matches.
    pub name: String,
    /// A live job's process is the build or one of its ancestors, or only a suggestion names it.
    pub queued: bool,
}

/// A background task of an actor and the file its output goes to. wtpool names the file and
/// never reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Output {
    pub task: String,
    pub path: String,
    pub summary: String,
    pub running: bool,
    pub started: RecordTime,
    pub ended: Option<RecordTime>,
}

impl Document {
    /// True when `other` says the same, apart from [`Self::generated`].
    #[must_use]
    pub fn same_content(&self, other: &Self) -> bool {
        self.machine == other.machine && self.pools == other.pools
    }
}

impl Serialize for DocumentVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

impl Reason {
    pub const ALL: [Self; 8] = [
        Self::Foreground,
        Self::Unmanaged,
        Self::Branch,
        Self::Dirty,
        Self::Unreferenced,
        Self::Operation,
        Self::Unknown,
        Self::Missing,
    ];
}

impl Activity {
    pub const ALL: [Self; 10] = [
        Self::Missing,
        Self::Waiting,
        Self::Working,
        Self::Building,
        Self::Idle,
        Self::Foreground,
        Self::Free,
        Self::NotReusable,
        Self::Unknown,
        Self::Unused,
    ];
}

impl OperationName {
    pub const ALL: [Self; 6] = [
        Self::Rebase,
        Self::Am,
        Self::Merge,
        Self::CherryPick,
        Self::Revert,
        Self::Bisect,
    ];
}

impl Agents {
    pub const ALL: [Self; 2] = [Self::On, Self::Off];
}

impl ActorKind {
    pub const ALL: [Self; 2] = [Self::Session, Self::Subagent];
}

impl ActorState {
    pub const ALL: [Self; 5] = [
        Self::Working,
        Self::Quiet,
        Self::Waiting,
        Self::Idle,
        Self::Done,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_document(generated: i64, name: &str) -> Document {
        Document {
            kind: DocumentKind::Status,
            version: DocumentVersion,
            generated: Timestamp::from_seconds(generated),
            machine: Machine {
                name: name.to_owned(),
                jobs: None,
            },
            pools: Vec::new(),
        }
    }

    #[test]
    fn document__should_serialize_type_and_version() {
        // Given
        let document = empty_document(0, "m");

        // When
        let json = serde_json::to_string(&document).unwrap();

        // Then
        assert_eq!(
            json,
            r#"{"type":"status","version":1,"generated":"1970-01-01T00:00:00Z","machine":{"name":"m"},"pools":[]}"#
        );
    }

    #[test]
    fn same_content__should_ignore_only_the_generated_time() {
        // Given
        let document = empty_document(0, "m");

        // When
        let later = document.same_content(&empty_document(60, "m"));
        let other = document.same_content(&empty_document(0, "n"));
        let mut with_pool = empty_document(0, "m");
        with_pool.pools.push(PoolStatus {
            common_dir: "/c".to_owned(),
            agents: Agents::On,
            skipped_records: 0,
            worktrees: Vec::new(),
            repository: Repository {
                name: "app".to_owned(),
                github: Some("acme/app".to_owned()),
            },
        });
        let pools = document.same_content(&with_pool);

        // Then
        assert_eq!((later, other, pools), (true, false, false));
    }
}
