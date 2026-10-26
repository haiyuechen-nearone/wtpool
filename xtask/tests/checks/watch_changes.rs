//! `wtpool status --watch` reacts only to what changed: W3, W4, W5 and W7 to W9.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use xtask::pool::WorktreeSpec;

use super::watch::{
    GIT_CHANGE, HEARTBEAT, MISSED_CHANGE, count, detached, must_setup, record, replace_file,
    statuses,
};
use crate::support::{GitLog, is_alive, is_status, must_make_unreadable, worktree};

#[test]
fn watch__should_ignore_lock_and_temp_files() {
    // Given
    let setup = must_setup("watch-ignore", &[]);
    let mut log: GitLog = setup.sandbox.must_log_git();
    let mut watch = setup.watch();
    watch.settle_after_heartbeat();
    let before = log.starts().len();
    let common = &setup.pool.common;
    let files: Vec<PathBuf> = vec![
        common.join("refs/heads/ignored.lock"),
        common.join("worktrees/wt1/HEAD.lock"),
        common.join("packed-refs.lock"),
        common.join("wtpool-activity/s-ignored.json.lock"),
        common.join("wtpool-activity/.s-ignored.json.tmp"),
        common.join("wtpool-activity/notes.txt"),
        common.join("wtpool-activity/gc.stamp"),
        common.join("wtpool-verify-other-name"),
    ];

    // When
    for file in &files {
        fs::write(file, "x\n").expect("setup: write an ignored file");
    }
    std::thread::sleep(Duration::from_millis(300));
    for file in &files {
        let _ = fs::remove_file(file);
    }
    let lines = watch.lines_for(Duration::from_millis(1500));
    let after = log.starts().len();

    // Then
    let printed: Vec<&String> = lines
        .iter()
        .map(|(_, l)| l)
        .filter(|l| is_status(l))
        .collect();
    assert!(
        printed.is_empty(),
        "documents after ignored files: {printed:?}"
    );
    assert_eq!(
        after - before,
        0,
        "wtpool started git after events for lock, temp and other files"
    );
}

#[test]
fn watch__should_recompute_git_after_a_change() {
    // Given
    let setup = must_setup("watch-recompute", &[]);
    let mut log = setup.sandbox.must_log_git();
    let mut watch = setup.watch();
    watch.settle_after_heartbeat();
    let before = log.starts().len();
    let wt1 = setup.pool.worktree("wt1");
    let wt1_admin = setup.pool.git_dir("wt1");

    // When
    setup.git("wt1", &["commit", "--quiet", "--allow-empty", "-m", "one"]);
    let committed = Instant::now();
    let seen = watch.status_where(MISSED_CHANGE, |d| {
        count(d, "wt1", "unreferenced") == Some(1)
    });
    watch.lines_for(Duration::from_millis(500));
    let starts: Vec<String> = log
        .starts()
        .iter()
        .skip(before)
        .map(|(_, line)| line.clone())
        .collect();

    // Then
    let (at, _) = seen.expect("no document showed the commit in wt1");
    let delay = at.saturating_duration_since(committed);
    assert!(delay <= GIT_CHANGE, "the commit showed after {delay:?}");
    let foreign: Vec<&String> = starts
        .iter()
        .filter(|line| {
            let wt1_text = wt1.to_string_lossy();
            let admin_text = wt1_admin.to_string_lossy();
            let targets_wt1 = line.starts_with(wt1_text.as_ref())
                || line.contains(&format!("-C {wt1_text}"))
                || line.contains(admin_text.as_ref());
            !targets_wt1
        })
        .collect();
    assert!(
        foreign.is_empty(),
        "after a commit in wt1 alone, git ran for other worktrees too:\n{}",
        foreign
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn watch__should_read_only_the_changed_record() {
    // Given
    let setup = must_setup("watch-record", &[]);
    let activity = setup.pool.activity();
    let kept = activity.join("s-kept.json");
    let changed = activity.join("s-changed.json");
    fs::write(
        &kept,
        record(
            "s-kept",
            &setup.pool.worktree("wt1"),
            "2026-10-20T10:00:00Z",
        ),
    )
    .expect("setup: write a record");
    fs::write(
        &changed,
        record(
            "s-changed",
            &setup.pool.worktree("wt2"),
            "2026-10-20T10:00:00Z",
        ),
    )
    .expect("setup: write a record");
    let mut log = setup.sandbox.must_log_git();
    let mut watch = setup.watch();
    let first = watch.settle_after_heartbeat();
    let kept_before = worktree(&first, "wt1")["last_event"].clone();
    let skipped_before = first["pools"][0]["skipped_records"].clone();
    must_make_unreadable(&kept);
    let before = log.starts().len();

    // When
    replace_file(
        &changed,
        &record(
            "s-changed",
            &setup.pool.worktree("wt2"),
            "2026-10-20T11:00:00Z",
        ),
    );
    let written = Instant::now();
    let seen = watch.status_where(Duration::from_secs(3), |d| {
        worktree(d, "wt2")["last_event"] == "2026-10-20T11:00:00Z"
    });
    let after = log.starts().len();

    // Then
    assert_eq!(
        kept_before, "2026-10-20T10:00:00Z",
        "the first document must show wt1's record"
    );
    let (at, document) = seen.expect("no document showed the changed record");
    let delay = at.saturating_duration_since(written);
    assert!(
        delay <= GIT_CHANGE,
        "the changed record showed after {delay:?}"
    );
    assert_eq!(
        worktree(&document, "wt1")["last_event"],
        kept_before,
        "wt1's unchanged record, now unreadable, was read again"
    );
    assert_eq!(
        document["pools"][0]["skipped_records"], skipped_before,
        "the unchanged record, now unreadable, was read again and skipped"
    );
    assert_eq!(after - before, 0, "a record change started git");
}

#[test]
fn watch__should_merge_a_burst_into_one_document() {
    // Given
    let setup = must_setup(
        "watch-burst",
        &[(
            "wt1",
            WorktreeSpec {
                unreferenced: 10,
                ..detached()
            },
        )],
    );
    let log = setup.git("wt1", &["rev-list", "--reverse", "HEAD", "--not", "main"]);
    let commits: Vec<String> = log.lines().map(str::to_owned).collect();
    assert_eq!(commits.len(), 10, "setup: 10 unreferenced commits");
    let mut watch = setup.watch();
    watch.settle_after_heartbeat();
    let tags = setup.pool.common.join("refs/tags");

    // When
    let started = Instant::now();
    for (number, commit) in commits.iter().enumerate() {
        replace_file(
            &tags.join(format!("burst-{number}")),
            &format!("{commit}\n"),
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let burst = started.elapsed();
    let lines = watch.lines_for(Duration::from_millis(1500));

    // Then
    assert!(
        burst < Duration::from_millis(200),
        "setup: the burst took {burst:?}"
    );
    let documents = statuses(&lines);
    let counts: Vec<Option<u64>> = documents
        .iter()
        .map(|d| count(d, "wt1", "unreferenced"))
        .collect();
    assert_eq!(
        counts,
        vec![Some(0)],
        "a burst of 10 ref writes within 200 ms must print one document, the final one"
    );
}

#[test]
fn watch__should_print_only_changes() {
    // Given
    let setup = must_setup("watch-changes", &[]);
    let mut watch = setup.watch();
    watch.settle_after_heartbeat();
    let main = setup.git("fg", &["rev-parse", "main"]).trim().to_owned();

    // When
    setup.git("fg", &["update-ref", "refs/tags/same-commit", &main]);
    let lines = watch.lines_for(HEARTBEAT + Duration::from_secs(2));

    // Then
    let printed: Vec<&String> = lines
        .iter()
        .map(|(_, l)| l)
        .filter(|l| is_status(l))
        .collect();
    assert!(
        printed.is_empty(),
        "a tag on a referenced commit and a heartbeat changed nothing, yet wtpool printed {printed:?}"
    );
    assert!(
        lines.iter().any(|(_, l)| is_alive(l)),
        "no heartbeat in {HEARTBEAT:?}"
    );
}

#[test]
fn watch__should_show_a_missed_change_within_one_heartbeat() {
    // Given
    let setup = must_setup("watch-missed", &[]);
    let mut watch = setup.watch();
    watch.settle_after_heartbeat();

    // When
    fs::write(setup.pool.worktree("wt1").join("f.txt"), "edited\n").expect("setup: edit");
    let edited = Instant::now();
    let seen = watch.status_where(MISSED_CHANGE + Duration::from_secs(1), |d| {
        count(d, "wt1", "dirty") == Some(1)
    });

    // Then
    let (at, _) = seen.expect("no document showed the edited file of wt1 within a heartbeat");
    let delay = at.saturating_duration_since(edited);
    assert!(delay <= MISSED_CHANGE, "the edit showed after {delay:?}");
}

#[test]
fn watch__should_recompute_on_rescan() {
    // Given
    let setup = must_setup("watch-rescan", &[]);
    let activity = setup.pool.activity();
    let mut watch = setup.watch();
    watch.settle_after_heartbeat();
    let moved = setup.pool.common.join("wtpool-activity.old");

    // When
    fs::rename(&activity, &moved).expect("setup: move wtpool-activity away");
    fs::create_dir(&activity).expect("setup: create a new wtpool-activity");
    fs::write(activity.join(".verify-sandbox"), "").expect("setup: mark the sandbox");
    std::thread::sleep(Duration::from_millis(300));
    fs::write(
        activity.join("s-new.json"),
        record("s-new", &setup.pool.worktree("wt3"), "2026-10-20T11:30:00Z"),
    )
    .expect("setup: write a record");
    let written = Instant::now();
    let seen = watch.status_where(Duration::from_secs(3), |d| {
        worktree(d, "wt3")["last_event"] == "2026-10-20T11:30:00Z"
    });

    // Then
    let (at, _) = seen.expect("no document showed a record in the replaced wtpool-activity folder");
    let delay = at.saturating_duration_since(written);
    assert!(
        delay <= GIT_CHANGE,
        "a record in the replaced watched folder showed after {delay:?}"
    );
}

#[test]
fn watch__should_exit_on_stdin_eof() {
    // Given
    let setup = must_setup("watch-eof", &[]);
    let mut watch = setup.watch();
    let first = watch.next_line(Duration::from_secs(5));

    // When
    watch.close_stdin();
    let status = watch.exit_within(Duration::from_secs(2));

    // Then
    assert!(first.is_some(), "wtpool status --watch printed nothing");
    let status = status.expect("wtpool status --watch still runs 2 s after stdin EOF");
    assert_eq!(
        status.code(),
        Some(0),
        "exit status after stdin EOF: {status:?}"
    );
}

#[test]
fn watch__should_exit_zero_on_a_closed_pipe() {
    // Given
    let setup = must_setup("watch-pipe", &[]);
    let mut watch = setup.watch();
    let first = watch.next_line(Duration::from_secs(5));

    // When
    watch.close_stdout_reader();
    std::thread::sleep(Duration::from_millis(200));
    setup.git(
        "wt1",
        &["commit", "--quiet", "--allow-empty", "-m", "change"],
    );
    let status = watch.exit_within(HEARTBEAT + Duration::from_secs(3));

    // Then
    assert!(first.is_some(), "wtpool status --watch printed nothing");
    let status = status.expect("wtpool status --watch still runs after its stdout closed");
    assert_eq!(
        status.code(),
        Some(0),
        "exit status after a stdout write error: {status:?}"
    );
}

#[test]
fn watch__should_stay_quiet_on_an_idle_pool() {
    // Given
    let setup = must_setup("watch-idle", &[]);
    let mut log = setup.sandbox.must_log_git();
    let mut watch = setup.watch();
    let first = watch.status_where(Duration::from_secs(5), |_| true);
    std::thread::sleep(Duration::from_millis(500));
    let first_round = log.starts().len();

    // When
    let lines = watch.lines_for(Duration::from_secs(25));
    let starts: Vec<Instant> = log
        .starts()
        .iter()
        .skip(first_round)
        .map(|(at, _)| *at)
        .collect();

    // Then
    assert!(first.is_some(), "wtpool status --watch printed no document");
    let printed: Vec<&String> = lines
        .iter()
        .map(|(_, l)| l)
        .filter(|l| !is_alive(l))
        .collect();
    assert!(
        printed.is_empty(),
        "an idle pool printed more than alive lines: {printed:?}"
    );
    let alive: Vec<Instant> = lines
        .iter()
        .filter(|(_, l)| is_alive(l))
        .map(|(at, _)| *at)
        .collect();
    let near_heartbeat = |at: &Instant| {
        alive.iter().any(|beat| {
            let gap = if *at > *beat {
                *at - *beat
            } else {
                *beat - *at
            };
            gap <= Duration::from_secs(2)
        })
    };
    let stray: Vec<&Instant> = starts.iter().filter(|at| !near_heartbeat(at)).collect();
    assert!(
        stray.is_empty(),
        "{} git starts away from any heartbeat on an idle pool",
        stray.len()
    );
    for beat in &alive {
        let round = starts
            .iter()
            .filter(|at| {
                let gap = if **at > *beat {
                    **at - *beat
                } else {
                    *beat - **at
                };
                gap <= Duration::from_secs(2)
            })
            .count();
        assert!(
            round <= first_round,
            "a heartbeat started {round} git processes, more than the {first_round} of the first document"
        );
    }
}
