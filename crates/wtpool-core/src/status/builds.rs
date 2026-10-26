//! Build processes, the worktrees they work in and the processes they descend from.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::document::Build;
use crate::processes::{ProcessQuery, RunningProcess};

/// The facts `wtpool status` reads of every process: names and folders for the builds, parents
/// for the builds below an actor, and start times for liveness.
#[must_use]
pub fn query() -> ProcessQuery<'static> {
    ProcessQuery::all()
        .with_parent()
        .with_started()
        .with_name()
        .with_cwd()
}

#[must_use]
pub fn is_build(name: &str, build_processes: &[String]) -> bool {
    name == "cargo"
        || name.starts_with("cargo-")
        || name == "rustc"
        || build_processes.iter().any(|listed| listed == name)
}

/// The builds per worktree of `worktrees`, in that order, sorted by pid. A process belongs to
/// the worktree that holds its working folder, the deepest one when worktrees nest.
#[must_use]
pub fn builds_by_worktree(
    processes: &[RunningProcess],
    worktrees: &[PathBuf],
    build_processes: &[String],
) -> Vec<Vec<Build>> {
    let mut builds: Vec<Vec<Build>> = vec![Vec::new(); worktrees.len()];
    for process in processes {
        if !is_build(&process.name, build_processes) {
            continue;
        }
        let owner = worktrees
            .iter()
            .enumerate()
            .filter(|(_, worktree)| process.cwd.starts_with(worktree))
            .max_by_key(|(_, worktree)| worktree.components().count());
        if let Some((index, _)) = owner
            && let Some(list) = builds.get_mut(index)
        {
            list.push(Build {
                pid: process.pid,
                name: process.name.clone(),
            });
        }
    }
    for list in &mut builds {
        list.sort_by_key(|build| build.pid);
    }
    builds
}

/// The outermost build below each process, by that process's pid: of the build processes
/// whose chain of parents passes through the pid, the name of the one nearest to it, then of the
/// lowest pid. A chain that comes back to a pid it passed, as a table read while pids are reused
/// can give, stops there.
#[must_use]
pub fn builds_below(
    processes: &[RunningProcess],
    build_processes: &[String],
) -> BTreeMap<u32, String> {
    let parents: BTreeMap<u32, u32> = processes
        .iter()
        .filter_map(|process| Some((process.pid, process.parent?)))
        .collect();
    let mut nearest: BTreeMap<u32, Below<'_>> = BTreeMap::new();
    for build in processes
        .iter()
        .filter(|process| is_build(&process.name, build_processes))
    {
        let mut passed = BTreeSet::from([build.pid]);
        let (mut at, mut depth) = (build.pid, 0_usize);
        while let Some(&parent) = parents.get(&at)
            && passed.insert(parent)
        {
            depth = depth.saturating_add(1);
            let candidate = Below {
                depth,
                pid: build.pid,
                name: &build.name,
            };
            let best = nearest.entry(parent).or_insert(candidate);
            if (candidate.depth, candidate.pid) < (best.depth, best.pid) {
                *best = candidate;
            }
            at = parent;
        }
    }
    nearest
        .into_iter()
        .map(|(pid, below)| (pid, below.name.to_owned()))
        .collect()
}

/// A build below a process, `depth` parents down.
#[derive(Debug, Clone, Copy)]
struct Below<'a> {
    depth: usize,
    pid: u32,
    name: &'a str,
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::cargo("cargo", true)]
    #[case::cargo_subcommand("cargo-nextest", true)]
    #[case::cargo_dash("cargo-", true)]
    #[case::rustc("rustc", true)]
    #[case::listed("make", true)]
    #[case::cargo_prefix_without_dash("cargox", false)]
    #[case::cargo_inside_a_name("xcargo", false)]
    #[case::rustc_prefix("rustc-wrapper", false)]
    #[case::rustdoc("rustdoc", false)]
    #[case::unlisted("ninja", false)]
    fn is_build__should_match_cargo_star_rustc_and_the_setting(
        #[case] name: &str,
        #[case] expected: bool,
    ) {
        // When
        let build = is_build(name, &["make".to_owned()]);

        // Then
        assert_eq!(build, expected);
    }

    fn process(pid: u32, name: &str, cwd: &str) -> RunningProcess {
        RunningProcess {
            pid,
            parent: None,
            started: None,
            name: name.to_owned(),
            cwd: PathBuf::from(cwd),
        }
    }

    #[test]
    fn builds_by_worktree__should_place_builds_in_the_deepest_worktree_by_pid() {
        // Given
        let worktrees = vec![
            PathBuf::from("/p/fg"),
            PathBuf::from("/p/fg/inner"),
            PathBuf::from("/p/wt1"),
        ];
        let processes = vec![
            process(9, "cargo", "/p/wt1/target"),
            process(3, "rustc", "/p/wt1"),
            process(4, "cargo", "/p/fg/inner/src"),
            process(5, "rustc", "/p/fg"),
            process(6, "cargo", "/p/wt10"),
            process(7, "zsh", "/p/wt1"),
            process(8, "cargo", "/elsewhere"),
        ];

        // When
        let builds = builds_by_worktree(&processes, &worktrees, &[]);

        // Then
        let pids: Vec<Vec<u32>> = builds
            .iter()
            .map(|list| list.iter().map(|build| build.pid).collect())
            .collect();
        assert_eq!(pids, vec![vec![5], vec![4], vec![3, 9]]);
        assert_eq!(builds[2][0].name, "rustc");
    }

    fn child(pid: u32, parent: u32, name: &str) -> RunningProcess {
        RunningProcess {
            parent: Some(parent),
            ..process(pid, name, "/p/wt1")
        }
    }

    #[test]
    fn builds_below__should_name_the_outermost_build_of_each_ancestor() {
        // Given
        let processes = vec![
            child(100, 1, "claude"),
            child(110, 100, "zsh"),
            child(111, 110, "bash"),
            child(112, 111, "cargo-nextest"),
            child(113, 112, "cargo"),
            child(114, 113, "rustc"),
            child(120, 100, "node"),
            child(130, 100, "zsh"),
            child(131, 130, "make"),
            child(132, 130, "cargo"),
            child(200, 1, "zsh"),
            child(201, 200, "ninja"),
            process(300, "cargo", "/p"),
        ];

        // When
        let below = builds_below(&processes, &["make".to_owned()]);

        // Then
        let named = |pid: u32| below.get(&pid).map(String::as_str);
        assert_eq!(named(100), Some("make"));
        assert_eq!(named(110), Some("cargo-nextest"));
        assert_eq!(named(112), Some("cargo"));
        assert_eq!(named(130), Some("make"));
        assert_eq!(named(1), Some("make"));
        assert_eq!(
            (named(114), named(120), named(200), named(300)),
            (None, None, None, None)
        );
    }

    #[test]
    fn builds_below__should_stop_where_a_chain_comes_back() {
        // Given
        let processes = vec![
            child(5, 6, "cargo"),
            child(6, 7, "zsh"),
            child(7, 6, "claude"),
        ];

        // When
        let below = builds_below(&processes, &[]);

        // Then
        let expected = BTreeMap::from([(6, "cargo".to_owned()), (7, "cargo".to_owned())]);
        assert_eq!(below, expected);
    }
}
