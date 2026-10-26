//! The state `wtpool status` keeps of one pool: the worktrees with their git facts, the build
//! processes and the activity records, each refreshed on its own, and the document they make.

mod view;

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::thread;

use super::agents;
use super::builds;
use super::document::{Agents, Repository};
use super::facts::{Facts, RefFacts};
use super::jobs::JobsState;
use super::records::{ActorRecord, RecordState, Records};
use super::repository::repository;
use super::sources::Sources;
use crate::activity::ACTIVITY_DIR;
use crate::clock::Timestamp;
use crate::config::Config;
use crate::error::Error;
use crate::git::{Git, GitMode, layout};
use crate::jobs::JobId;
use crate::paths::comparable;
use crate::processes::{ListProcesses, Liveness, RunningProcess};
use crate::report::Discard;
use crate::worktree::Worktree;

/// One pool as `wtpool status` last read it.
#[derive(Debug)]
pub struct PoolState {
    config: Config,
    common: PathBuf,
    git_mode: GitMode,
    reach: Reach,
    machine: String,
    forge_repository: fn(&str) -> Option<String>,
    repository: Repository,
    worktrees: Vec<Tracked>,
    processes: Vec<RunningProcess>,
    records: Records,
    agents: Agents,
    jobs: JobsState,
}

/// The actors of a pool, read without the git facts and the repository a document also shows.
#[derive(Debug)]
pub struct PoolActors(PoolState);

/// A listed worktree and its facts.
#[derive(Debug)]
struct Tracked {
    worktree: Worktree,
    /// The path with symlinks resolved, as process tables and records name it.
    comparable: PathBuf,
    /// The folder that holds its `HEAD` and index, when known.
    git_dir: Option<PathBuf>,
    /// `None` when the folder is gone, and in a state that reads only the actors.
    facts: Option<Facts>,
    last_change: Option<Timestamp>,
}

/// The reach of a refresh of every worktree beyond the worktree list, the process table, the
/// records and the jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// The git facts of each worktree and the repository too.
    Everything,
    /// Only the facts the actors need.
    Actors,
}

impl PoolState {
    /// Reads everything about the pool of `config`, whose common git directory is `common`,
    /// from `sources`, through read-only git.
    pub fn load<C, P: ListProcesses>(
        config: Config,
        common: PathBuf,
        sources: &Sources<C, P>,
    ) -> Result<Self, Error> {
        Self::read(
            Reach::Everything,
            GitMode::ReadOnly,
            config,
            common,
            sources,
        )
    }

    fn read<C, P: ListProcesses>(
        reach: Reach,
        git_mode: GitMode,
        config: Config,
        common: PathBuf,
        sources: &Sources<C, P>,
    ) -> Result<Self, Error> {
        let records = Records::new(common.join(ACTIVITY_DIR));
        let markers = config
            .main_worktree
            .as_ref()
            .unwrap_or(&config.foreground.value)
            .to_path_buf();
        let host = &sources.host;
        let jobs = JobsState::read(host.jobs.clone(), host.cores, &common, markers)?;
        let mut state = Self {
            config,
            common,
            git_mode,
            reach,
            machine: host.name.clone(),
            forge_repository: sources.forge_repository,
            repository: Repository {
                name: String::new(),
                github: None,
            },
            worktrees: Vec::new(),
            processes: Vec::new(),
            records,
            agents: Agents::Off,
            jobs,
        };
        state.refresh_all(&sources.processes)?;
        Ok(state)
    }

    /// Lists the worktrees again and reads the process table, the records and the jobs, and the
    /// facts and the repository unless only the actors are read.
    pub fn refresh_all(&mut self, processes: &impl ListProcesses) -> Result<(), Error> {
        let foreground = Git::at(&self.config.foreground.value).with_mode(self.git_mode);
        let listed = Worktree::list_with(&foreground, &mut Discard)?;
        // git lists the main worktree first.
        let name = listed.first().map(Worktree::name).unwrap_or_default();
        let common = self.common.as_path();
        let remote = self.config.remote.as_str();
        let (forge_repository, reach) = (self.forge_repository, self.reach);
        let (worktrees, table, repository) = thread::scope(|scope| {
            let table = scope.spawn(|| processes.list(&builds::query()));
            let repository = (reach == Reach::Everything)
                .then(|| scope.spawn(|| repository(name, &foreground, remote, forge_repository)));
            let worktrees = track_all(listed, common, reach);
            (
                worktrees,
                table.join().unwrap_or_default(),
                repository.and_then(|repository| repository.join().ok()),
            )
        });
        self.worktrees = worktrees;
        self.processes = table;
        if let Some(repository) = repository {
            self.repository = repository;
        }
        self.rescan_records();
        self.jobs.refresh_all(&self.common);
        Ok(())
    }

    pub fn refresh_machine_config(&mut self) {
        self.jobs.refresh_machine_config();
    }

    pub fn refresh_repository_config(&mut self) {
        self.jobs.refresh_repository_config(&self.common);
    }

    /// Reads the process table again, and nothing else.
    pub fn refresh_processes(&mut self, processes: &impl ListProcesses) {
        self.processes = processes.list(&builds::query());
    }

    /// `true` while heavy processes outside a job are flagged, so the process table is worth
    /// reading between heartbeats.
    #[must_use]
    pub fn flags_outside(&self) -> bool {
        self.jobs.flags_outside()
    }

    /// Reads again the jobs `ids` name, or every job when `all` is set, and the process table,
    /// as a job's start or end changes it.
    pub fn refresh_jobs(
        &mut self,
        ids: &BTreeSet<JobId>,
        all: bool,
        processes: &impl ListProcesses,
    ) {
        self.jobs.refresh_jobs(ids, all);
        self.processes = processes.list(&builds::query());
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
        tracked.worktree = fresh;
        tracked.read_facts();
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

    /// Reads the record called `name` again, and gives its worktree as [`Records::refresh`]
    /// does. A record of a process the table does not hold yet reads the process table again, so
    /// a new session shows at once.
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

    /// The time an actor next changes state on its own, by the clock of the records.
    #[must_use]
    pub fn next_change(&self, now: Timestamp) -> Option<Timestamp> {
        agents::next_change(&self.counted_records(), now)
    }

    fn counted_records(&self) -> Vec<&ActorRecord> {
        self.records.counted(|record| {
            record.state != RecordState::Ended
                && Liveness::UnlessGone.runs(record.process, &self.processes)
        })
    }

    pub fn rescan_records(&mut self) {
        self.records.rescan();
        self.agents = agents_of(&self.records);
    }

    /// True when the linked worktrees' git folders differ from the listed ones, so the worktree
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
}

impl Tracked {
    /// True when `path` names this worktree, as written or with symlinks resolved.
    fn holds(&self, path: &Path) -> bool {
        path == self.worktree.path().as_path() || path == self.comparable
    }

    /// Reads the git facts and the time of the last change again.
    fn read_facts(&mut self) {
        self.facts = (!self.worktree.is_gone()).then(|| Facts::read(&self.worktree));
        self.last_change = self.git_dir.as_deref().and_then(last_change);
    }
}

fn track_all(listed: Vec<Worktree>, common: &Path, reach: Reach) -> Vec<Tracked> {
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
                scope.spawn(move || {
                    let mut tracked = Tracked {
                        worktree,
                        comparable,
                        git_dir,
                        facts: None,
                        last_change: None,
                    };
                    if reach == Reach::Everything {
                        tracked.read_facts();
                    }
                    tracked
                })
            })
            .collect();
        readers
            .into_iter()
            .filter_map(|reader| reader.join().ok())
            .collect()
    })
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
