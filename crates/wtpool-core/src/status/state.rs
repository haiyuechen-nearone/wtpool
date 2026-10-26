//! What `wtpool status` knows of one pool: the worktrees with their git facts, the build processes
//! and the activity records, each refreshed on its own, and the document they make.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::thread;

use super::agents::{self, Context};
use super::builds::{self, builds_below, builds_by_worktree};
use super::document::{
    Actor, ActorState, Agents, Build, Document, DocumentKind, DocumentVersion, Machine, PoolStatus,
    Repository, WorktreeStatus,
};
use super::facts::{self, Busy, Facts, RefFacts};
use super::records::{Record, RecordState, Records};
use super::repository::repository;
use crate::activity::ACTIVITY_DIR;
use crate::clock::Timestamp;
use crate::config::Config;
use crate::error::Error;
use crate::git::{Git, GitMode, layout};
use crate::paths::comparable;
use crate::processes::{ListProcesses, Liveness, RunningProcess};
use crate::report::Discard;
use crate::worktree::{Head, Worktree};

/// One pool as `wtpool status` last read it.
#[derive(Debug)]
pub struct PoolState {
    config: Config,
    common: PathBuf,
    git_mode: GitMode,
    machine: String,
    repository: Repository,
    worktrees: Vec<Tracked>,
    processes: Vec<RunningProcess>,
    records: Records,
    agents: Agents,
}

/// A listed worktree and its facts.
#[derive(Debug)]
struct Tracked {
    worktree: Worktree,
    /// The path with symlinks resolved, as process tables and records name it.
    comparable: PathBuf,
    /// The folder that holds its `HEAD` and index, when known.
    git_dir: Option<PathBuf>,
    /// `None` when the folder is gone.
    facts: Option<Facts>,
    /// The newest modification of its `HEAD` and index.
    last_change: Option<Timestamp>,
}

impl PoolState {
    /// Reads everything about the pool of `config`, whose common git directory is `common`, on
    /// the machine called `machine`, through read-only git.
    pub fn load(
        config: Config,
        common: PathBuf,
        processes: &impl ListProcesses,
        machine: String,
    ) -> Result<Self, Error> {
        Self::load_with(GitMode::ReadOnly, config, common, processes, machine)
    }

    /// [`PoolState::load`] through git in `git_mode`.
    pub fn load_with(
        git_mode: GitMode,
        config: Config,
        common: PathBuf,
        processes: &impl ListProcesses,
        machine: String,
    ) -> Result<Self, Error> {
        let records = Records::new(common.join(ACTIVITY_DIR));
        let mut state = Self {
            config,
            common,
            git_mode,
            machine,
            repository: Repository {
                name: String::new(),
                github: None,
            },
            worktrees: Vec::new(),
            processes: Vec::new(),
            records,
            agents: Agents::Off,
        };
        state.refresh_all(processes)?;
        Ok(state)
    }

    /// Lists the worktrees again and reads every fact, the process table, the repository and
    /// the records.
    pub fn refresh_all(&mut self, processes: &impl ListProcesses) -> Result<(), Error> {
        let foreground = Git::at(&self.config.foreground).with_mode(self.git_mode);
        let listed = Worktree::list_with(&foreground, &mut Discard)?;
        // git lists the main worktree first.
        let name = listed.first().map(Worktree::name).unwrap_or_default();
        let common = self.common.as_path();
        let remote = self.config.remote.as_str();
        let (worktrees, table, repository) = thread::scope(|scope| {
            let table = scope.spawn(|| processes.list(&builds::query()));
            let repository = scope.spawn(|| repository(name, &foreground, remote));
            let worktrees = track_all(listed, common);
            (
                worktrees,
                table.join().unwrap_or_default(),
                repository.join().ok(),
            )
        });
        self.worktrees = worktrees;
        self.processes = table;
        if let Some(repository) = repository {
            self.repository = repository;
        }
        self.rescan_records();
        Ok(())
    }

    /// Reads again the worktree whose git folder is `git_dir`, running git only in that
    /// worktree. `false` when no listed worktree has that folder or git no longer lists it.
    pub fn refresh_git_dir(&mut self, git_dir: &Path) -> bool {
        let Some(tracked) = self
            .worktrees
            .iter_mut()
            .find(|tracked| tracked.git_dir.as_deref() == Some(git_dir))
        else {
            return false;
        };
        let fresh = Worktree::list_with(&tracked.worktree.git(), &mut Discard)
            .ok()
            .and_then(|listed| {
                listed
                    .into_iter()
                    .find(|worktree| worktree.path() == tracked.worktree.path())
            });
        let Some(fresh) = fresh else {
            return false;
        };
        tracked.facts = read_facts(&fresh);
        tracked.last_change = tracked.git_dir.as_deref().and_then(last_change);
        tracked.worktree = fresh;
        true
    }

    /// Reads again, in every worktree, the facts a ref change may change.
    pub fn refresh_refs(&mut self) {
        thread::scope(|scope| {
            for tracked in &mut self.worktrees {
                let Tracked {
                    worktree, facts, ..
                } = tracked;
                if let Some(facts) = facts {
                    let worktree: &Worktree = worktree;
                    scope.spawn(move || facts.refs = RefFacts::read(worktree));
                }
            }
        });
    }

    /// Reads the record called `name` again. Gives its worktree when the change ended an edit
    /// tool or Bash call there. A record of a process the table does not hold yet reads the
    /// process table again, so a new session shows at once.
    pub fn refresh_record(
        &mut self,
        name: &OsStr,
        processes: &impl ListProcesses,
    ) -> Option<PathBuf> {
        let edited = self.records.refresh(name);
        self.agents = agents_of(&self.records);
        let key = Path::new(name).file_stem().unwrap_or_default();
        let unknown = self
            .records
            .get(&key.to_string_lossy())
            .is_some_and(|record| !Liveness::UnlessGone.runs(record.process, &self.processes));
        if unknown {
            self.processes = processes.list(&builds::query());
        }
        edited
    }

    /// Reads again the worktree at `path`, as [`PoolState::refresh_git_dir`] does. `false` when
    /// no listed worktree is there.
    pub fn refresh_worktree(&mut self, path: &Path) -> bool {
        let git_dir = self
            .worktrees
            .iter()
            .find(|tracked| tracked.holds(path))
            .and_then(|tracked| tracked.git_dir.clone());
        git_dir.is_some_and(|git_dir| self.refresh_git_dir(&git_dir))
    }

    /// When an actor next changes state on its own, by the clock of the records.
    #[must_use]
    pub fn next_change(&self, now: Timestamp) -> Option<Timestamp> {
        agents::next_change(&self.counted_records(), now)
    }

    fn counted_records(&self) -> Vec<&Record> {
        self.records.counted(|record| {
            record.state != RecordState::Ended
                && Liveness::UnlessGone.runs(record.process, &self.processes)
        })
    }

    pub fn rescan_records(&mut self) {
        self.records.rescan();
        self.agents = agents_of(&self.records);
    }

    /// Whether the linked worktrees' git folders differ from the listed ones, so the worktree
    /// list may have changed.
    #[must_use]
    pub fn git_dirs_changed(&self) -> bool {
        let found: BTreeSet<PathBuf> = linked_git_dirs(&self.common).into_values().collect();
        let known: BTreeSet<PathBuf> = self
            .worktrees
            .iter()
            .filter_map(|tracked| tracked.git_dir.clone())
            .filter(|git_dir| *git_dir != self.common)
            .collect();
        found != known
    }

    /// The document of what is known now, generated at `now`.
    #[must_use]
    pub fn document(&self, now: Timestamp) -> Document {
        let paths: Vec<PathBuf> = self
            .worktrees
            .iter()
            .map(|tracked| tracked.comparable.clone())
            .collect();
        let builds = builds_by_worktree(&self.processes, &paths, &self.config.build_processes);
        let records = self.counted_records();
        let building = |path: &Path| {
            self.worktrees
                .iter()
                .zip(&builds)
                .any(|(tracked, builds)| tracked.holds(path) && !builds.is_empty())
        };
        let in_pool: Vec<&Record> = records
            .iter()
            .filter(|record| self.worktrees.iter().any(|t| t.holds(&record.worktree)))
            .copied()
            .collect();
        let superseded = agents::superseded(&in_pool);
        let below = builds_below(&self.processes, &self.config.build_processes);
        let context = Context {
            now,
            processes: &self.processes,
            records: &records,
            building: &building,
            superseded: &superseded,
            builds_below: &below,
        };
        let worktrees = self
            .worktrees
            .iter()
            .zip(&builds)
            .map(|(tracked, builds)| {
                let placed: Vec<&Record> = records
                    .iter()
                    .filter(|record| tracked.holds(&record.worktree))
                    .copied()
                    .collect();
                tracked.status(&self.config, builds.clone(), &placed, &context)
            })
            .collect();
        Document {
            kind: DocumentKind::Status,
            version: DocumentVersion,
            generated: now,
            machine: Machine {
                name: self.machine.clone(),
            },
            pools: vec![PoolStatus {
                common_dir: self.common.to_string_lossy().into_owned(),
                agents: self.agents,
                skipped_records: self.records.skipped(),
                worktrees,
                repository: self.repository.clone(),
            }],
        }
    }
}

impl Tracked {
    /// Whether `path` names this worktree, as written or with symlinks resolved.
    fn holds(&self, path: &Path) -> bool {
        path == self.worktree.path().as_path() || path == self.comparable
    }

    fn status(
        &self,
        config: &Config,
        builds: Vec<Build>,
        placed: &[&Record],
        context: &Context<'_>,
    ) -> WorktreeStatus {
        let worktree = &self.worktree;
        let role = config.role_of(worktree);
        let missing = self.facts.is_none();
        let head = worktree.head();
        let git = facts::git_status(head, self.facts.as_ref());
        let reasons = facts::reasons(role, missing, head, &git);
        let free = reasons.is_empty();
        let mut actors: Vec<Actor> = placed
            .iter()
            .filter_map(|record| agents::actor(record, context))
            .collect();
        agents::sort(&mut actors);
        let has = |state: ActorState| actors.iter().any(|actor| actor.state == state);
        let busy = Busy {
            waiting: has(ActorState::Waiting),
            working: has(ActorState::Working),
            building: !builds.is_empty(),
            idle: has(ActorState::Idle) || has(ActorState::Quiet),
        };
        let activity = facts::activity(role, missing, head, &git, free, busy);
        WorktreeStatus {
            path: worktree.path().to_string_lossy().into_owned(),
            name: worktree.name(),
            role,
            missing,
            branch: match head {
                Head::Branch(branch) => Some(branch.clone()),
                Head::Detached | Head::Unknown => None,
            },
            upstream: self
                .facts
                .as_ref()
                .and_then(|facts| facts.refs.upstream.as_ref())
                .map(|tracking| tracking.name.clone()),
            git,
            free,
            reasons,
            activity,
            actors,
            builds,
            last_event: placed
                .iter()
                .map(|record| &record.updated)
                .max_by_key(|updated| updated.at())
                .cloned(),
            last_change: self.last_change,
        }
    }
}

/// Every listed worktree with its facts, read in parallel. A bare repository is no worktree.
fn track_all(listed: Vec<Worktree>, common: &Path) -> Vec<Tracked> {
    let linked = linked_git_dirs(common);
    thread::scope(|scope| {
        let readers: Vec<_> = listed
            .into_iter()
            .enumerate()
            .filter(|(_, worktree)| !worktree.is_bare())
            .map(|(index, worktree)| {
                let comparable = comparable(worktree.path());
                let git_dir = if index == 0 {
                    Some(common.to_path_buf())
                } else {
                    linked.get(&comparable).cloned()
                };
                scope.spawn(move || Tracked {
                    facts: read_facts(&worktree),
                    last_change: git_dir.as_deref().and_then(last_change),
                    worktree,
                    comparable,
                    git_dir,
                })
            })
            .collect();
        readers
            .into_iter()
            .filter_map(|reader| reader.join().ok())
            .collect()
    })
}

fn read_facts(worktree: &Worktree) -> Option<Facts> {
    (!worktree.is_gone()).then(|| Facts::read(worktree))
}

fn agents_of(records: &Records) -> Agents {
    if records.enabled() {
        Agents::On
    } else {
        Agents::Off
    }
}

/// The git folder `<common>/worktrees/<id>` of each linked worktree, by the worktree's path
/// with symlinks resolved, read from each folder's `gitdir` file without git.
fn linked_git_dirs(common: &Path) -> BTreeMap<PathBuf, PathBuf> {
    std::fs::read_dir(layout::worktrees_dir(common))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let git_dir = entry.path();
            let text = std::fs::read_to_string(layout::back_link_file(&git_dir)).ok()?;
            let dot_git = git_dir.join(layout::back_link(&text));
            Some((comparable(dot_git.parent()?), git_dir))
        })
        .collect()
}

/// The newest modification of the worktree's `HEAD` and index.
fn last_change(git_dir: &Path) -> Option<Timestamp> {
    [layout::HEAD, layout::INDEX]
        .iter()
        .filter_map(|name| git_dir.join(name).symlink_metadata().ok()?.modified().ok())
        .map(Timestamp::from_system_time)
        .max()
}

#[cfg(test)]
mod tests;
