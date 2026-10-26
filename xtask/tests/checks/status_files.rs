//! `wtpool status` leaves the pool as it found it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use xtask::pool::{Pool, WorktreeSpec, git_in};

use super::status::{detached, no_procs, on_branch, spec};
use crate::support::{FileState, Sandbox, Watch, file_state, must_status_json, must_wtpool_bin};

/// Size, modification time and mode of every entry below `dir`.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, FileState> {
    let mut found = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path.clone());
            }
            found.insert(path, file_state(&meta));
        }
    }
    found
}

fn changed(
    before: &BTreeMap<PathBuf, FileState>,
    after: &BTreeMap<PathBuf, FileState>,
) -> Vec<String> {
    let mut found: Vec<String> = before
        .iter()
        .filter(|(path, meta)| after.get(*path) != Some(*meta))
        .map(|(path, _)| format!("changed or removed: {}", path.display()))
        .collect();
    found.extend(
        after
            .keys()
            .filter(|path| !before.contains_key(*path))
            .map(|path| format!("created: {}", path.display())),
    );
    found
}

/// Makes the index of `name` stat dirty, so a git that may write the index refreshes it.
fn make_index_stale(pool: &Pool, name: &str) {
    let file = pool.worktree(name).join("f.txt");
    let text = fs::read_to_string(&file).expect("setup: read f.txt");
    std::thread::sleep(Duration::from_millis(1100));
    fs::write(&file, text).expect("setup: rewrite f.txt");
}

#[test]
fn status__should_leave_every_file_unchanged() {
    // Given
    let sandbox = Sandbox::must_new("unchanged");
    let pool = sandbox.must_pool(&spec(&[
        (
            "wt2",
            WorktreeSpec {
                upstream: true,
                ahead: 1,
                ..on_branch("feature-a")
            },
        ),
        (
            "wt3",
            WorktreeSpec {
                dirty: 1,
                ..detached()
            },
        ),
    ]));
    for name in ["fg", "wt1", "wt2"] {
        make_index_stale(&pool, name);
    }
    let procs = no_procs(&sandbox);
    let before = snapshot(&sandbox.root);

    // When
    let output = sandbox.run_in(
        &must_wtpool_bin(),
        &["status", "--json"],
        &pool.fg,
        &[("WTPOOL_VERIFY_PROCS", procs.as_os_str())],
    );
    let mut watch = Watch::must_start(&sandbox, &[("WTPOOL_VERIFY_PROCS", procs.as_os_str())]);
    let first = watch.status_where(Duration::from_secs(5), |_| true);
    watch.lines_for(Duration::from_secs(2));
    watch.close_stdin();
    let exited = watch.exit_within(Duration::from_secs(5));
    let after = snapshot(&sandbox.root);

    // Then
    assert!(
        output.status.success(),
        "wtpool status --json exited {:?}",
        output.status
    );
    assert!(first.is_some(), "wtpool status --watch printed no document");
    assert!(
        exited.is_some(),
        "wtpool status --watch did not exit on stdin EOF"
    );
    let differences = changed(&before, &after);
    assert!(
        differences.is_empty(),
        "wtpool status changed files:\n{}",
        differences.join("\n")
    );
}

#[test]
fn status__should_not_start_fsmonitor() {
    // Given
    let sandbox = Sandbox::must_new("fsmonitor");
    let pool = sandbox.must_pool(&spec(&[]));
    git_in(&pool.fg, &["config", "core.fsmonitor", "true"]).expect("setup: enable fsmonitor");
    let mut log = sandbox.must_log_git();
    let procs = no_procs(&sandbox);

    // When
    let document = must_status_json(
        &sandbox,
        &pool.fg,
        &[("WTPOOL_VERIFY_PROCS", procs.as_os_str())],
    );
    let starts: Vec<String> = log.starts().iter().map(|(_, line)| line.clone()).collect();
    let daemons: Vec<String> = ["fg", "wt1", "wt2", "wt3", "x"]
        .iter()
        .filter_map(|name| {
            let state =
                git_in(&pool.worktree(name), &["fsmonitor--daemon", "status"]).unwrap_or_default();
            let _ = git_in(&pool.worktree(name), &["fsmonitor--daemon", "stop"]);
            (!state.contains("not watching") && !state.trim().is_empty())
                .then(|| format!("{name}: {}", state.trim()))
        })
        .collect();

    // Then
    assert!(document["pools"].is_array(), "no document");
    assert!(!starts.is_empty(), "wtpool status ran no git from PATH");
    let without_flags: Vec<&String> = starts
        .iter()
        .filter(|line| {
            !line.contains("--no-optional-locks") || !line.contains("-c core.fsmonitor=false")
        })
        .collect();
    assert!(
        without_flags.is_empty(),
        "git ran without --no-optional-locks and -c core.fsmonitor=false:\n{}",
        without_flags
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(daemons.is_empty(), "fsmonitor daemons run: {daemons:?}");
    let sockets: Vec<PathBuf> = ["fg", "wt1", "wt2", "wt3", "x"]
        .iter()
        .map(|name| pool.git_dir(name).join("fsmonitor--daemon.ipc"))
        .filter(|path| path.exists())
        .collect();
    assert!(sockets.is_empty(), "fsmonitor sockets exist: {sockets:?}");
}

#[test]
fn status__should_never_create_index_lock() {
    // Given
    let sandbox = Sandbox::must_new("index-lock");
    let pool = sandbox.must_pool(&spec(&[("wt2", on_branch("feature-a"))]));
    let names = ["fg", "wt1", "wt2", "wt3", "x"];
    for name in names {
        make_index_stale(&pool, name);
    }
    let indexes: Vec<PathBuf> = names
        .iter()
        .map(|n| pool.git_dir(n).join("index"))
        .collect();
    let locks: Vec<PathBuf> = names
        .iter()
        .map(|n| pool.git_dir(n).join("index.lock"))
        .collect();
    let before: Vec<FileState> = indexes
        .iter()
        .map(|p| file_state(&fs::metadata(p).expect("setup: stat an index")))
        .collect();
    let seen = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let watcher = {
        let (seen, stop, locks) = (seen.clone(), stop.clone(), locks.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                if locks.iter().any(|lock| fs::symlink_metadata(lock).is_ok()) {
                    seen.store(true, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_micros(500));
            }
        })
    };
    let procs = no_procs(&sandbox);

    // When
    let outputs: Vec<bool> = (0..5)
        .map(|_| {
            sandbox
                .run_in(
                    &must_wtpool_bin(),
                    &["status", "--json"],
                    &pool.fg,
                    &[("WTPOOL_VERIFY_PROCS", procs.as_os_str())],
                )
                .status
                .success()
        })
        .collect();
    stop.store(true, Ordering::SeqCst);
    let _ = watcher.join();

    // Then
    assert!(outputs.iter().all(|ok| *ok), "wtpool status --json failed");
    assert!(!seen.load(Ordering::SeqCst), "an index.lock appeared");
    let after: Vec<FileState> = indexes
        .iter()
        .map(|p| file_state(&fs::metadata(p).expect("setup: stat an index")))
        .collect();
    assert_eq!(before, after, "wtpool status rewrote an index");
}
