//! The pool lock every command that changes the pool takes, and the git environment of the
//! tests.

use std::process::Command;
use std::time::Duration;

use rstest::rstest;
use wtpool_core::{Pool, SyncMode};

use crate::support::{FakePr, Fixture, PoolCommand, Recorder, must_hold_pool_lock};

#[rstest]
#[case::fg(
    |pool: &Pool<FakePr>,
    report: &mut Recorder| pool.foreground("feature", SyncMode::No, report).map(drop)
)]
#[case::bg(
    |pool: &Pool<FakePr>,
    report: &mut Recorder| pool.background("feature", SyncMode::No, report).map(drop)
)]
#[case::free(|pool: &Pool<FakePr>, report: &mut Recorder| pool.free("app-wt1", report))]
#[case::new(|pool: &Pool<FakePr>, report: &mut Recorder| pool.new_slot(None, report).map(drop))]
#[case::rm(|pool: &Pool<FakePr>, report: &mut Recorder| pool.remove("app-wt1", report))]
#[case::setup(|pool: &Pool<FakePr>, report: &mut Recorder| pool.setup(report).map(drop))]
fn pool_commands__should_wait_for_the_pool_lock(#[case] command: PoolCommand) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    fixture.must_add_worktree("app-wt1", None);
    let pool = fixture.pool(3);
    let held = must_hold_pool_lock(&fixture);

    // When
    let (finished_while_held, (result, report)) = std::thread::scope(|scope| {
        let (done, finished) = std::sync::mpsc::channel();
        let pool = &pool;
        let run = scope.spawn(move || {
            let mut report = Recorder::default();
            let result = command(pool, &mut report);
            done.send(()).unwrap();
            (result, report)
        });
        let finished_while_held = finished.recv_timeout(Duration::from_millis(300)).is_ok();
        drop(held);
        (finished_while_held, run.join().unwrap())
    });

    // Then
    assert!(!finished_while_held);
    assert!(result.is_ok(), "{result:?}");
    assert!(
        report
            .err
            .contains("waiting for another wtpool command to finish\n"),
        "{}",
        report.err
    );
}

#[test]
fn git_subprocesses__should_not_read_the_global_or_system_config() {
    // Given
    let dir = tempfile::tempdir().unwrap();

    // When
    let output = Command::new("git")
        .args(["config", "--list", "--show-scope"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Then
    let listed = String::from_utf8(output.stdout).unwrap();
    let leaked: Vec<&str> = listed
        .lines()
        .filter(|line| line.starts_with("global\t") || line.starts_with("system\t"))
        .collect();
    assert!(leaked.is_empty(), "{leaked:?}");
}
