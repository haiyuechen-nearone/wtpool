//! The pool lock every command that changes the pool takes, and the git environment of the
//! tests.

use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use rstest::rstest;
use wtpool_core::{AgentGuard, Pool, SyncMode};

use crate::support::{FakePr, Fixture, PoolCommand, Recorder, must_hold_pool_lock};

const WAITING: &str = "waiting for another wtpool command to finish";

#[rstest]
#[case::fg(
    |pool: &Pool<FakePr>,
    report: &mut Recorder| pool.foreground("feature", SyncMode::No, AgentGuard::Force, report).map(drop)
)]
#[case::bg(
    |pool: &Pool<FakePr>,
    report: &mut Recorder| pool.background("feature", SyncMode::No, report).map(drop)
)]
#[case::free(|pool: &Pool<FakePr>, report: &mut Recorder| pool.free("app-wt1", AgentGuard::Force, report))]
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
    let (waited_while_held, (result, report)) = std::thread::scope(|scope| {
        let (notes_to, notes) = mpsc::channel();
        let pool = &pool;
        let run = scope.spawn(move || {
            let mut report = Recorder {
                notes_to: Some(notes_to),
                ..Recorder::default()
            };
            let result = command(pool, &mut report);
            report.notes_to = None;
            (result, report)
        });
        let waited = saw_note(&notes, WAITING);
        let waited_while_held = waited && !run.is_finished();
        drop(held);
        (waited_while_held, run.join().unwrap())
    });

    // Then
    assert!(waited_while_held, "{}", report.err);
    assert!(result.is_ok(), "{result:?}");
    assert!(
        report.err.contains(&format!("{WAITING}\n")),
        "{}",
        report.err
    );
}

/// `true` once `notes` gives `line`, `false` when the command ends first or a minute passes.
fn saw_note(notes: &Receiver<String>, line: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match notes.recv_timeout(left) {
            Ok(note) if note == line => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
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
