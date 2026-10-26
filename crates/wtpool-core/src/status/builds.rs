//! Build processes, the worktrees they work in and the processes they descend from.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::document::Build;
use crate::jobs::HeavyTable;
use crate::processes::{ProcessQuery, RunningProcess};

/// The facts `wtpool status` reads of every process: names, arguments and folders for the
/// builds, parents for the builds below an actor and the jobs that hold them, and start times
/// for liveness.
#[must_use]
pub fn query() -> ProcessQuery<'static> {
    ProcessQuery::all()
        .with_parent()
        .with_started()
        .with_name()
        .with_cwd()
        .with_cmd()
}

/// A process a heavy pattern names, and the job that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildProcess<'a> {
    pub process: &'a RunningProcess,
    /// The words of the pattern it matches.
    pub name: &'a str,
    /// A job holds it, or only a suggestion names it.
    pub queued: bool,
    /// The actor of the job that holds it.
    pub owner: Option<&'a str>,
}

/// A build below a process, and the actor of the job that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildBelow {
    pub name: String,
    pub owner: Option<String>,
}

/// Each process of `table` a heavy pattern names, with `owners`, the actor of each job by its
/// index in the job list.
#[must_use]
pub fn build_processes<'a>(
    table: &'a HeavyTable<'a>,
    owners: &'a [Option<String>],
) -> Vec<BuildProcess<'a>> {
    table
        .processes()
        .iter()
        .filter_map(|process| {
            let name = table.name(process.pid)?;
            let holder = table.holder(process.pid);
            Some(BuildProcess {
                process,
                name: &name.name,
                queued: holder.is_some() || name.suggested,
                owner: holder
                    .and_then(|index| owners.get(index))
                    .and_then(Option::as_deref),
            })
        })
        .collect()
}

/// The builds per worktree of `worktrees`, in that order, sorted by pid. A process belongs to
/// the worktree that holds its working folder, the deepest one when worktrees nest.
#[must_use]
pub fn builds_by_worktree<'a, 'b>(
    builds: &'b [BuildProcess<'a>],
    worktrees: &[PathBuf],
) -> Vec<Vec<&'b BuildProcess<'a>>> {
    let mut placed: Vec<Vec<&BuildProcess<'a>>> = vec![Vec::new(); worktrees.len()];
    for build in builds {
        let owner = worktrees
            .iter()
            .enumerate()
            .filter(|(_, worktree)| build.process.cwd.starts_with(worktree))
            .max_by_key(|(_, worktree)| worktree.components().count());
        if let Some((index, _)) = owner
            && let Some(list) = placed.get_mut(index)
        {
            list.push(build);
        }
    }
    for list in &mut placed {
        list.sort_by_key(|build| build.process.pid);
    }
    placed
}

/// The document's form of `build`.
#[must_use]
pub fn shown(build: &BuildProcess<'_>) -> Build {
    Build {
        pid: build.process.pid,
        name: build.name.to_owned(),
        queued: build.queued,
    }
}

/// The builds below each process, by that process's pid: of the builds whose chain of
/// parents passes through the pid, the nearest first, then the lowest pid. A chain that comes
/// back to a pid it passed, as a table read while pids are reused can give, stops there.
#[must_use]
pub fn builds_below(
    processes: &[RunningProcess],
    builds: &[BuildProcess<'_>],
) -> BTreeMap<u32, Vec<BuildBelow>> {
    let parents: BTreeMap<u32, u32> = processes
        .iter()
        .filter_map(|process| Some((process.pid, process.parent?)))
        .collect();
    let mut found: BTreeMap<u32, Vec<(usize, u32, &BuildProcess<'_>)>> = BTreeMap::new();
    for build in builds {
        let pid = build.process.pid;
        let mut passed = BTreeSet::from([pid]);
        let (mut at, mut depth) = (pid, 0_usize);
        while let Some(&parent) = parents.get(&at)
            && passed.insert(parent)
        {
            depth = depth.saturating_add(1);
            found.entry(parent).or_default().push((depth, pid, build));
            at = parent;
        }
    }
    found
        .into_iter()
        .map(|(pid, mut below)| {
            below.sort_by_key(|(depth, pid, _)| (*depth, *pid));
            let below = below
                .into_iter()
                .map(|(_, _, build)| BuildBelow {
                    name: build.name.to_owned(),
                    owner: build.owner.map(str::to_owned),
                })
                .collect();
            (pid, below)
        })
        .collect()
}

#[cfg(test)]
mod tests;
