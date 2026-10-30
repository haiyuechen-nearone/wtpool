use std::path::PathBuf;

use super::*;
use crate::heavy::{HeavyList, Pattern};

fn process(pid: u32, parent: Option<u32>, name: &str, cwd: &str) -> RunningProcess {
    RunningProcess {
        pid,
        parent,
        started: None,
        name: name.to_owned(),
        cwd: PathBuf::from(cwd),
        cmd: Vec::new(),
    }
}

fn list(patterns: &[&str]) -> HeavyList {
    let patterns: Vec<Pattern> = patterns
        .iter()
        .map(|p| Pattern::parse(p).unwrap())
        .collect();
    HeavyList::new(&patterns, &[], &[])
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
        process(9, None, "cargo", "/p/wt1/target"),
        process(3, None, "rustc", "/p/wt1"),
        process(4, None, "cargo", "/p/fg/inner/src"),
        process(5, None, "rustc", "/p/fg"),
        process(6, None, "cargo", "/p/wt10"),
        process(7, None, "zsh", "/p/wt1"),
        process(8, None, "cargo", "/elsewhere"),
    ];
    let table = HeavyTable::new(&processes, &list(&["cargo", "rustc"]), &list(&[]), &[]);
    let builds = build_processes(&table, &BTreeMap::new());

    // When
    let placed = builds_by_worktree(&builds, &worktrees);

    // Then
    let pids: Vec<Vec<u32>> = placed
        .iter()
        .map(|list| list.iter().map(|build| build.process.pid).collect())
        .collect();
    assert_eq!(pids, vec![vec![5], vec![4], vec![3, 9]]);
    assert_eq!(
        shown(placed[2][0]),
        Build {
            pid: 3,
            name: "rustc".to_owned(),
            queued: false,
        }
    );
}

#[test]
fn build_processes__should_queue_suggested_builds() {
    // Given
    let processes = vec![
        process(1, None, "make", "/p"),
        process(2, None, "cargo", "/p"),
    ];
    let table = HeavyTable::new(&processes, &list(&["cargo"]), &list(&["make"]), &[]);

    // When
    let builds = build_processes(&table, &BTreeMap::new());

    // Then
    let found: Vec<(u32, &str, bool)> = builds
        .iter()
        .map(|build| (build.process.pid, build.name, build.queued))
        .collect();
    assert_eq!(found, vec![(1, "make", true), (2, "cargo", false)]);
}

fn child(pid: u32, parent: u32, name: &str) -> RunningProcess {
    process(pid, Some(parent), name, "/p/wt1")
}

#[test]
fn builds_below__should_order_the_builds_of_each_ancestor_nearest_first() {
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
        process(300, None, "cargo", "/p"),
    ];
    let table = HeavyTable::new(
        &processes,
        &list(&["cargo", "cargo-*", "rustc", "make"]),
        &list(&[]),
        &[],
    );
    let builds = build_processes(&table, &BTreeMap::new());

    // When
    let below = builds_below(&processes, &builds, &BTreeSet::new());

    // Then
    let first = |pid: u32| {
        below
            .get(&pid)
            .and_then(|builds| builds.first())
            .map(|build| build.name.as_str())
    };
    assert_eq!(first(100), Some("make"));
    assert_eq!(first(110), Some("cargo-nextest"));
    assert_eq!(first(112), Some("cargo"));
    assert_eq!(first(130), Some("make"));
    assert_eq!(first(1), Some("make"));
    assert_eq!(
        (first(114), first(120), first(200), first(300)),
        (None, None, None, None)
    );
    let names: Vec<&str> = below[&100].iter().map(|b| b.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["make", "cargo", "cargo-nextest", "cargo", "rustc"]
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
    let table = HeavyTable::new(&processes, &list(&["cargo"]), &list(&[]), &[]);
    let builds = build_processes(&table, &BTreeMap::new());

    // When
    let below = builds_below(&processes, &builds, &BTreeSet::new());

    // Then
    let cargo = vec![BuildBelow {
        name: "cargo".to_owned(),
        owner: None,
    }];
    let expected = BTreeMap::from([(6, cargo.clone()), (7, cargo)]);
    assert_eq!(below, expected);
}

#[test]
fn builds_below__should_end_at_the_nearest_harness_process() {
    // Given
    let processes = vec![
        child(100, 1, "claude"),
        child(110, 100, "zsh"),
        child(120, 110, "opencode"),
        child(121, 120, "zsh"),
        child(122, 121, "cargo"),
        child(130, 100, "cargo"),
    ];
    let table = HeavyTable::new(&processes, &list(&["cargo"]), &list(&[]), &[]);
    let builds = build_processes(&table, &BTreeMap::new());
    let harnesses = BTreeSet::from([100, 120]);

    // When
    let below = builds_below(&processes, &builds, &harnesses);

    // Then
    let pids: Vec<(u32, usize)> = below
        .iter()
        .map(|(pid, builds)| (*pid, builds.len()))
        .collect();
    assert_eq!(pids, vec![(100, 1), (120, 1), (121, 1)]);
}
