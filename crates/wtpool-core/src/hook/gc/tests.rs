use std::sync::atomic::{AtomicI64, Ordering};

use rstest::rstest;

use super::super::record::{HarnessProcess, Record};
use super::*;
use crate::activity::ActorId;
use crate::clock::SystemClock;
use crate::hook::fixture::{self, NOW, eventually, later, must_activity};
use crate::processes::ProcessFile;

const CLAUDE: &str =
    r#"{"pid":7001,"ppid":1,"started":"2026-10-20T11:00:00Z","name":"claude","cwd":"/"}"#;

/// An empty `wtpool-activity/` and a process table that lists the live claude.
fn must_activity_with_claude() -> (tempfile::TempDir, ActivityDir, ProcessFile) {
    let (dir, activity) = must_activity();
    let procs = dir.path().join("procs");
    std::fs::write(&procs, CLAUDE).unwrap();
    (dir, activity, ProcessFile::new(procs))
}

fn record(session: &str, agent: Option<&str>, state: State, age: i64, alive: bool) -> Record {
    let mut record = fixture::record(session);
    record.agent_id = agent.map(|agent| ActorId::try_from(agent).unwrap());
    record.state = state;
    record.updated = later(-age);
    record.process = Some(HarnessProcess {
        pid: if alive { 7001 } else { 7099 },
        started: "2026-10-20T11:00:00Z".to_owned(),
        shared: false,
    });
    record
}

/// `record` as one of a process that runs many sessions.
fn shared(mut record: Record) -> Record {
    if let Some(process) = &mut record.process {
        process.shared = true;
    }
    record
}

#[rstest]
#[case::ended_8_days(record("s", None, State::Ended, 8 * 86_400, true), false)]
#[case::ended_6_days(record("s", None, State::Ended, 6 * 86_400, true), true)]
#[case::dead_8_days(record("s", None, State::Idle, 8 * 86_400, false), false)]
#[case::live_8_days(record("s", None, State::Working, 8 * 86_400, true), true)]
#[case::done_16_minutes_alone(record("s", Some("a"), State::Done, 16 * 60, true), true)]
#[case::shared_live_8_days(shared(record("s", None, State::Idle, 8 * 86_400, true)), false)]
#[case::shared_live_6_days(shared(record("s", None, State::Idle, 6 * 86_400, true)), true)]
fn collect__should_delete_only_old_dead_or_ended_records(
    #[case] record: Record,
    #[case] kept: bool,
) {
    // Given
    let (dir, activity, table) = must_activity_with_claude();
    let stem = match &record.agent_id {
        Some(agent) => format!("{}.{agent}", record.session_id),
        None => record.session_id.to_string(),
    };
    activity.write(&stem, &record).unwrap();
    std::fs::write(dir.path().join("wtpool-activity/orphan.json.lock"), "").unwrap();

    // When
    let collected = collect(&activity, NOW, &table, None);

    // Then
    assert_eq!(activity.read(&stem).is_some(), kept);
    assert_eq!(collected.kept + collected.deleted, 1);
    assert!(activity.orphan_locks().is_empty());
}

#[test]
fn collect__should_keep_the_newest_done_subagent_of_each_worktree() {
    // Given
    let (_dir, activity, table) = must_activity_with_claude();
    for (agent, worktree, age) in [
        ("a1", "/p/wt1", 30),
        ("a2", "/p/wt1", 20),
        ("a3", "/p/wt2", 40),
    ] {
        let mut subagent = record("s1", Some(agent), State::Done, age * 60, true);
        subagent.worktree = worktree.to_owned();
        activity.write(&format!("s1.{agent}"), &subagent).unwrap();
    }

    // When
    let collected = collect(&activity, NOW, &table, None);

    // Then
    assert_eq!(
        activity.stems(),
        vec!["s1.a2".to_owned(), "s1.a3".to_owned()]
    );
    assert_eq!(
        collected,
        Collected {
            kept: 2,
            deleted: 1,
            stopped: false
        }
    );
}

/// A pool of `count` sessions that ended 8 days ago, and the table of their live claude.
fn ended_week_old(count: usize) -> (tempfile::TempDir, ActivityDir, ProcessFile) {
    let (dir, activity, table) = must_activity_with_claude();
    for n in 0..count {
        let session = format!("s{n}");
        let ended = record(&session, None, State::Ended, 8 * 86_400, true);
        activity.write(&session, &ended).unwrap();
    }
    (dir, activity, table)
}

/// A clock whose system time starts at 0 and moves a microsecond on every read.
#[derive(Default)]
struct Ticking(AtomicI64);

impl ReadClock for Ticking {
    fn now(&self) -> Timestamp {
        NOW
    }

    fn system_micros(&self) -> i64 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

/// A deadline the system clock never reaches.
fn far_ahead() -> Deadline<'static> {
    Deadline::new(&SystemClock, i64::MAX)
}

#[rstest]
#[case::passed(Some(0), 0, true)]
#[case::after_the_reads_and_one_delete(Some(4), 1, true)]
#[case::far_ahead(Some(i64::MAX), 3, false)]
#[case::none(None, 3, false)]
fn collect__should_stop_at_its_deadline(
    #[case] deadline: Option<i64>,
    #[case] deleted: usize,
    #[case] stopped: bool,
) {
    // Given
    let (_dir, activity, table) = ended_week_old(3);
    let clock = Ticking::default();

    // When
    let deadline = deadline.map(|micros| Deadline::new(&clock, micros));
    let collected = collect(&activity, NOW, &table, deadline);

    // Then
    assert_eq!(
        collected,
        Collected {
            kept: 0,
            deleted,
            stopped
        }
    );
    assert_eq!(activity.stems().len(), 3 - deleted);
}

#[test]
fn after_session_end__should_collect_once_a_day() {
    // Given
    let (_dir, activity, table) = ended_week_old(2);

    // When
    let first = after_session_end(&activity, NOW, &table, far_ahead());
    let ended = record("s9", None, State::Ended, 8 * 86_400, true);
    activity.write("s9", &ended).unwrap();
    let same_day = after_session_end(&activity, later(23 * 3600), &table, far_ahead());
    let mut next_day = None;
    let ran = eventually(|| {
        next_day = after_session_end(&activity, later(24 * 3600), &table, far_ahead());
        next_day.is_some()
    });

    // Then
    assert_eq!(first.map(|c| c.deleted), Some(2));
    assert_eq!(same_day, None);
    assert!(ran, "a day later the stamp gave no gc");
    assert_eq!(next_day.map(|c| c.deleted), Some(1));
    assert!(activity.stems().is_empty());
}

#[test]
fn after_session_end__should_leave_the_day_open_when_stopped() {
    // Given
    let (_dir, activity, table) = ended_week_old(2);

    // When
    let stopped = after_session_end(&activity, NOW, &table, Deadline::new(&SystemClock, 0));
    let again = after_session_end(&activity, later(60), &table, far_ahead());

    // Then
    assert_eq!(
        stopped,
        Some(Collected {
            kept: 0,
            deleted: 0,
            stopped: true
        })
    );
    assert_eq!(again.map(|c| c.deleted), Some(2));
}

#[cfg(unix)]
#[test]
fn collect__should_keep_a_record_whose_lock_cannot_be_taken() {
    // Given
    let (_dir, activity, table) = ended_week_old(1);
    let unreadable = activity.record_path("s9");
    std::fs::write(&unreadable, "{").unwrap();
    let old = std::time::UNIX_EPOCH
        + std::time::Duration::from_secs(u64::try_from(later(-8 * 86_400).seconds()).unwrap());
    std::fs::File::open(&unreadable)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let made = std::process::Command::new("mkfifo")
        .arg(activity.record_path("s9").with_extension("json.lock"))
        .status()
        .unwrap();
    assert!(made.success());

    // When
    let collected = collect(&activity, NOW, &table, None);

    // Then
    assert_eq!(
        collected,
        Collected {
            kept: 0,
            deleted: 1,
            stopped: false
        }
    );
    assert!(unreadable.symlink_metadata().is_ok());
}

#[cfg(unix)]
#[test]
fn collect__should_keep_an_expired_record_whose_lock_another_process_holds() {
    // Given
    let (_dir, activity, table) = ended_week_old(1);
    let held = activity.lock_now("s0").unwrap();
    let mut live = record("s0", None, State::Working, 8 * 86_400, true);
    live.updated = later(-8 * 86_400);
    live.state = State::Ended;
    activity.write("s0", &live).unwrap();

    // When
    let collected = collect(&activity, NOW, &table, None);

    // Then
    drop(held);
    assert_eq!(
        collected,
        Collected {
            kept: 1,
            deleted: 0,
            stopped: false
        }
    );
    assert!(activity.read("s0").is_some());
}

#[test]
fn collect_now__should_run_on_a_fresh_stamp_and_stamp_the_pool() {
    // Given
    let (_dir, activity, table) = ended_week_old(1);
    assert!(after_session_end(&activity, later(-60), &table, far_ahead()).is_some());
    let ended = record("s9", None, State::Ended, 8 * 86_400, true);
    activity.write("s9", &ended).unwrap();

    let stamp = || std::fs::read_to_string(activity.file_path("gc.stamp")).unwrap();

    // When
    let collected = collect_now(&activity, NOW, &table);
    let stamped = eventually(|| {
        let _ = collect_now(&activity, NOW, &table);
        stamp() == format!("{}\n", NOW.to_rfc3339())
    });

    // Then
    assert_eq!(collected.deleted, 1);
    assert!(stamped, "wtpool hook gc left the stamp at {}", stamp());
}

#[test]
fn collect_pool__should_find_nothing_without_the_folder() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let env = HookEnv::from_process(None, None);

    // When
    let collected = collect_pool(dir.path(), &env);

    // Then
    assert_eq!(collected, Collected::default());
}

/// Gives `path` the last write `seconds` before [`NOW`].
#[cfg(unix)]
fn written_before(path: &std::path::Path, seconds: i64) {
    let at = std::time::UNIX_EPOCH
        + std::time::Duration::from_secs(u64::try_from(later(-seconds).seconds()).unwrap());
    std::fs::File::open(path).unwrap().set_modified(at).unwrap();
}

#[cfg(unix)]
#[rstest]
#[case::unreadable_8_days("file", 8 * 86_400, false)]
#[case::unreadable_6_days("file", 6 * 86_400, true)]
#[case::folder_8_days("folder", 8 * 86_400, true)]
#[case::symlink_to_an_old_file("symlink", 8 * 86_400, true)]
fn collect__should_delete_an_unreadable_record_file_a_week_old(
    #[case] entry: &str,
    #[case] age: i64,
    #[case] kept: bool,
) {
    // Given
    let (dir, activity, table) = must_activity_with_claude();
    let path = activity.record_path("s9");
    let old = dir.path().join("old.json");
    std::fs::write(&old, "{").unwrap();
    written_before(&old, age);
    match entry {
        "file" => {
            std::fs::write(&path, r#"{"version":1,"updated":"noon"}"#).unwrap();
            written_before(&path, age);
        }
        "folder" => {
            std::fs::create_dir(&path).unwrap();
            written_before(&path, age);
        }
        _ => std::os::unix::fs::symlink(&old, &path).unwrap(),
    }

    // When
    let collected = collect(&activity, NOW, &table, None);

    // Then
    assert_eq!(path.symlink_metadata().is_ok(), kept, "{entry} {age} s old");
    assert_eq!(collected.deleted, usize::from(!kept));
    assert!(old.exists(), "the symlink's target was deleted");
}
