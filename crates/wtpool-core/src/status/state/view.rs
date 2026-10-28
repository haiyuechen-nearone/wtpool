//! The views of a [`PoolState`]: the whole document, or only the actors of some worktrees.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::{PoolActors, PoolState, Reach, Tracked};
use crate::clock::Timestamp;
use crate::config::Config;
use crate::error::Error;
use crate::git::GitMode;
use crate::heavy::HeavyList;
use crate::jobs::{HeavyTable, JobId, JobRecord};
use crate::paths;
use crate::processes::ListProcesses;
use crate::status::agents::{self, ActorJob, Context};
use crate::status::builds::{self, BuildBelow, build_processes, builds_below, builds_by_worktree};
use crate::status::document::{
    Actor, ActorState, Build, Document, DocumentKind, DocumentVersion, Jobs, Machine, PoolStatus,
    WorktreeStatus,
};
use crate::status::facts::{self, Busy};
use crate::status::jobs::{self, Harnesses, JobsFacts, PoolFolders};
use crate::status::records::ActorRecord;
use crate::status::sources::Sources;
use crate::worktree::Head;

/// The records that count, as the last refresh read them.
struct Counted<'s> {
    /// Neither ended nor of a gone process, in the pool or not.
    all: Vec<&'s ActorRecord>,
    /// The sessions a newer main session of their process replaced.
    superseded: BTreeSet<String>,
    /// Those of `all` in the pool and not replaced.
    shown: Vec<&'s ActorRecord>,
    /// Those of `all` placed in each worktree, in the order of the state's worktrees.
    placed: Vec<Vec<&'s ActorRecord>>,
}

/// The machine's jobs, and the patterns that name heavy processes.
struct JobLists<'s> {
    heavy: HeavyList,
    suggested: HeavyList,
    records: Vec<&'s JobRecord>,
}

/// The builds and the jobs of the process table, each with the actor it belongs to.
struct Work<'a> {
    table: HeavyTable<'a>,
    harnesses: Harnesses<'a>,
    /// The actor of each job of the [`JobLists`], in their order.
    owners: Vec<Option<String>>,
    /// The builds of each worktree, in the order of the state's worktrees.
    builds: Vec<Vec<HeldBuild>>,
    below: BTreeMap<u32, Vec<BuildBelow>>,
    actor_jobs: BTreeMap<String, ActorJob>,
}

/// A build, and the actor of the job that holds it.
struct HeldBuild {
    build: Build,
    owner: Option<String>,
}

impl PoolActors {
    /// [`PoolState::load`] through git in `git_mode`, with only the facts the actors need.
    pub fn load<C, P: ListProcesses>(
        git_mode: GitMode,
        config: Config,
        common: PathBuf,
        sources: &Sources<C, P>,
    ) -> Result<Self, Error> {
        PoolState::read(Reach::Actors, git_mode, config, common, sources).map(Self)
    }

    /// The actors of each worktree of `paths` at `now`, as the document shows them, but for
    /// those whose harness process is among `skipped`.
    #[must_use]
    pub fn at(&self, paths: &[&Path], now: Timestamp, skipped: &[u32]) -> Vec<Vec<Actor>> {
        let state = &self.0;
        let counted = state.counted();
        let lists = state.job_lists();
        let work = state.work(&counted, &lists);
        let actors = state.actors(now, &counted, &work, skipped);
        paths
            .iter()
            .map(|path| {
                state
                    .worktrees
                    .iter()
                    .zip(&actors)
                    .filter(|(tracked, _)| paths::same_path(tracked.worktree.path(), path))
                    .flat_map(|(_, actors)| actors.iter().cloned())
                    .collect()
            })
            .collect()
    }
}

impl PoolState {
    /// The document of the current state, generated at `now`.
    #[must_use]
    pub fn document(&self, now: Timestamp) -> Document {
        let counted = self.counted();
        let lists = self.job_lists();
        let work = self.work(&counted, &lists);
        Document {
            kind: DocumentKind::Status,
            version: DocumentVersion,
            generated: now,
            machine: Machine {
                name: self.machine.clone(),
                jobs: self.jobs_document(now, &lists, &work),
            },
            pools: vec![PoolStatus {
                common_dir: self.common.to_string_lossy().into_owned(),
                agents: self.agents,
                skipped_records: self.records.skipped(),
                worktrees: self.worktree_statuses(now, &counted, &work),
                repository: self.repository.clone(),
            }],
        }
    }

    fn counted(&self) -> Counted<'_> {
        let all = self.counted_records();
        let in_pool: Vec<&ActorRecord> = all
            .iter()
            .filter(|record| self.worktrees.iter().any(|t| t.holds(&record.worktree)))
            .copied()
            .collect();
        let superseded = agents::superseded(&in_pool);
        let shown = in_pool
            .into_iter()
            .filter(|record| !superseded.contains(record.key.session()))
            .collect();
        let placed = self
            .worktrees
            .iter()
            .map(|tracked| {
                all.iter()
                    .filter(|record| tracked.holds(&record.worktree))
                    .copied()
                    .collect()
            })
            .collect();
        Counted {
            all,
            superseded,
            shown,
            placed,
        }
    }

    fn job_lists(&self) -> JobLists<'_> {
        let heavy = self.jobs.heavy();
        JobLists {
            suggested: self.jobs.build_suggestions(&heavy),
            heavy,
            records: self.jobs.records(),
        }
    }

    fn work<'a>(&'a self, counted: &'a Counted<'_>, lists: &JobLists<'a>) -> Work<'a> {
        let table = HeavyTable::new(
            &self.processes,
            &lists.heavy,
            &lists.suggested,
            &lists.records,
        );
        let harnesses = Harnesses::new(&counted.shown);
        let owners: Vec<Option<String>> = lists
            .records
            .iter()
            .map(|job| harnesses.actor_of(job.pid, &job.summary, &table))
            .collect();
        let owner_of: BTreeMap<&JobId, &str> = lists
            .records
            .iter()
            .zip(&owners)
            .filter_map(|(job, owner)| Some((&job.id, owner.as_deref()?)))
            .collect();
        let build_list = build_processes(&table, &owner_of);
        let paths: Vec<PathBuf> = self
            .worktrees
            .iter()
            .map(|tracked| tracked.comparable.clone())
            .collect();
        let builds = builds_by_worktree(&build_list, &paths)
            .into_iter()
            .map(|builds| {
                builds
                    .into_iter()
                    .map(|build| HeldBuild {
                        build: builds::shown(build),
                        owner: build.owner.map(str::to_owned),
                    })
                    .collect()
            })
            .collect();
        let below = builds_below(&self.processes, &build_list);
        let actor_jobs = jobs::actor_jobs(&lists.records, &owners);
        Work {
            table,
            harnesses,
            owners,
            builds,
            below,
            actor_jobs,
        }
    }

    /// The sorted actors of each worktree, in the order of the state's worktrees, but for those
    /// whose harness process is among `skipped`.
    fn actors(
        &self,
        now: Timestamp,
        counted: &Counted<'_>,
        work: &Work<'_>,
        skipped: &[u32],
    ) -> Vec<Vec<Actor>> {
        let building = |path: &Path, key: &str| {
            self.worktrees
                .iter()
                .zip(&work.builds)
                .any(|(tracked, builds)| {
                    tracked.holds(path)
                        && builds
                            .iter()
                            .any(|held| held.owner.as_deref().is_none_or(|owner| owner == key))
                })
        };
        let context = Context {
            now,
            processes: &self.processes,
            records: &counted.all,
            building: &building,
            superseded: &counted.superseded,
            builds_below: &work.below,
            jobs: &work.actor_jobs,
        };
        counted
            .placed
            .iter()
            .map(|placed| {
                let skipped = |record: &&&ActorRecord| {
                    record
                        .process
                        .is_some_and(|process| skipped.contains(&process.pid))
                };
                let mut actors: Vec<Actor> = placed
                    .iter()
                    .filter(|record| !skipped(record))
                    .filter_map(|record| agents::actor(record, &context))
                    .collect();
                agents::sort(&mut actors);
                actors
            })
            .collect()
    }

    fn worktree_statuses(
        &self,
        now: Timestamp,
        counted: &Counted<'_>,
        work: &Work<'_>,
    ) -> Vec<WorktreeStatus> {
        let actors = self.actors(now, counted, work, &[]);
        self.worktrees
            .iter()
            .zip(&work.builds)
            .zip(&counted.placed)
            .zip(actors)
            .map(|(((tracked, builds), placed), actors)| {
                let builds = builds.iter().map(|held| held.build.clone()).collect();
                tracked.status(&self.config, builds, placed, actors)
            })
            .collect()
    }

    /// Each worktree's path as the document shows it, and its path with symlinks resolved.
    fn folders(&self) -> PoolFolders {
        PoolFolders(
            self.worktrees
                .iter()
                .map(|tracked| {
                    let path = tracked.worktree.path().to_string_lossy().into_owned();
                    (path, tracked.comparable.clone())
                })
                .collect(),
        )
    }

    fn jobs_document(&self, now: Timestamp, lists: &JobLists<'_>, work: &Work<'_>) -> Option<Jobs> {
        let folders = self.folders();
        let facts = JobsFacts {
            heavy: &lists.heavy,
            table: &work.table,
            owners: &work.owners,
            harnesses: &work.harnesses,
            folders: &folders,
            now,
        };
        jobs::jobs_document(&self.jobs, &facts)
    }
}

impl Tracked {
    fn status(
        &self,
        config: &Config,
        builds: Vec<Build>,
        placed: &[&ActorRecord],
        actors: Vec<Actor>,
    ) -> WorktreeStatus {
        let worktree = &self.worktree;
        let role = config.role_of(worktree);
        let missing = self.facts.is_none();
        let head = worktree.head();
        let git = facts::git_status(head, self.facts.as_ref());
        let reasons = facts::reasons(role, missing, head, &git);
        let free = reasons.is_empty();
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
