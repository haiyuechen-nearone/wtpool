//! The tests of [`Live`] and of `watch`, on a pool in a temporary folder.

use std::ffi::OsString;
use std::path::Path;
use std::thread;

use rstest::rstest;

use super::*;
use crate::clock::Timestamp;
use crate::jobs::JobPaths;
use crate::status::document::GitStatus;
use crate::status::fixture::{
    FixedClock, FixedProcesses, fixed_sources, https_path, must_fixture, must_git,
};
use crate::status::watch::support::{SharedOutput, must_wait};
use crate::status::watch::{Message, message_of, watch};

#[test]
fn message_of__should_pass_paths_and_ask_for_a_rescan_when_events_were_lost() {
    // Given
    let path = PathBuf::from("/repo/.git/HEAD");
    let event = notify::Event::new(notify::EventKind::Any).add_path(path.clone());
    let lost = notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan);

    // When
    let messages = [
        message_of(Ok(event)),
        message_of(Ok(lost)),
        message_of(Err(notify::Error::generic("gone"))),
    ];

    // Then
    let rescan = Message::Paths {
        paths: Vec::new(),
        rescan: true,
    };
    let expected = [
        Message::Paths {
            paths: vec![path],
            rescan: false,
        },
        rescan.clone(),
        rescan,
    ];
    assert_eq!(messages, expected);
}

/// The facts a refresh may have read again, as the document shows them.
#[derive(Debug, PartialEq, Eq)]
struct Seen {
    /// Only a full refresh reads the process table.
    builds: bool,
    wt1_unreferenced: Option<u64>,
    main_ahead: Option<u64>,
    wt1_event: bool,
    x_event: bool,
    worktrees: usize,
}

const NOTHING: Seen = Seen {
    builds: false,
    wt1_unreferenced: Some(0),
    main_ahead: Some(0),
    wt1_event: false,
    x_event: false,
    worktrees: 3,
};

const EVERYTHING: Seen = Seen {
    builds: true,
    wt1_unreferenced: Some(1),
    main_ahead: Some(1),
    wt1_event: true,
    x_event: true,
    worktrees: 3,
};

fn pending(build: impl FnOnce(&mut Pending, &Path)) -> impl FnOnce(&Path) -> Pending {
    move |common| {
        let mut pending = Pending::default();
        build(&mut pending, common);
        pending
    }
}

type MakePending = Box<dyn FnOnce(&Path) -> Pending>;

#[rstest]
#[case::nothing(Box::new(pending(|_, _| {})), false, NOTHING)]
#[case::all(Box::new(pending(|p, _| p.all = true)), false, EVERYTHING)]
#[case::added_worktree(Box::new(pending(|p, _| p.worktrees = true)), true, Seen { worktrees: 4, ..EVERYTHING })]
#[case::same_worktrees(Box::new(pending(|p, _| p.worktrees = true)), false, NOTHING)]
#[case::added_worktree_unannounced(Box::new(pending(|p, _| p.refs = true)), true, Seen { main_ahead: Some(1), ..NOTHING })]
#[case::slot_git_dir(Box::new(pending(|p, c| { p.git_dirs.insert(c.join("worktrees/wt1")); })), false, Seen { wt1_unreferenced: Some(1), ..NOTHING })]
#[case::main_git_dir(Box::new(pending(|p, c| { p.git_dirs.insert(c.to_path_buf()); })), false, Seen { main_ahead: Some(1), ..NOTHING })]
#[case::unknown_git_dir(Box::new(pending(|p, c| { p.git_dirs.insert(c.join("worktrees/nope")); })), false, EVERYTHING)]
#[case::two_unknown_git_dirs(Box::new(pending(|p, c| { p.git_dirs.insert(c.join("worktrees/a")); p.git_dirs.insert(c.join("worktrees/b")); })), false, EVERYTHING)]
#[case::refs(Box::new(pending(|p, _| p.refs = true)), false, Seen { main_ahead: Some(1), ..NOTHING })]
#[case::one_record(Box::new(pending(|p, _| { p.records.insert(OsString::from("s-1.json")); })), false, Seen { wt1_event: true, ..NOTHING })]
#[case::records_folder(Box::new(pending(|p, _| { p.rescan_records = true; p.records.insert(OsString::from("s-1.json")); })), false, Seen { wt1_event: true, x_event: true, ..NOTHING })]
fn live_refresh__should_read_again_only_what_changed(
    #[case] make: MakePending,
    #[case] add_worktree: bool,
    #[case] expected: Seen,
) {
    // Given
    let fixture = must_fixture();
    let processes = FixedProcesses::default();
    let sources = fixed_sources(FixedClock, processes.clone(), None);
    let state = PoolState::load(fixture.config(), fixture.common.clone(), &sources).unwrap();
    let mut live = Live::new(state, &sources);
    let git = must_git;
    git(
        &fixture.root.join("wt1"),
        &["commit", "--quiet", "--allow-empty", "-m", "one"],
    );
    git(
        &fixture.main,
        &["commit", "--quiet", "--allow-empty", "-m", "two"],
    );
    processes.set(vec![fixture.process(9, "make", "x")]);
    fixture.record("s-1.json", "wt1", "2026-10-20T10:00:00Z");
    fixture.record("s-2.json", "x", "2026-10-20T10:00:00Z");
    if add_worktree {
        git(
            &fixture.main,
            &["worktree", "add", "--quiet", "--detach", "../wt2", "main"],
        );
    }
    let pending = make(&fixture.common);

    // When
    live.refresh(&pending, Instant::now());

    // Then
    let document = live.document();
    let worktrees = &document.pools[0].worktrees;
    let find = |name: &str| worktrees.iter().find(|w| w.name == name).unwrap();
    let seen = Seen {
        builds: !find("x").builds.is_empty(),
        wt1_unreferenced: find("wt1").git.unreferenced,
        main_ahead: find("main").git.ahead,
        wt1_event: find("wt1").last_event.is_some(),
        x_event: find("x").last_event.is_some(),
        worktrees: worktrees.len(),
    };
    assert_eq!(seen, expected);
    assert_eq!(document.generated, Timestamp::from_seconds(42));
}

#[test]
fn live_refresh_all__should_keep_what_was_known_when_listing_fails() {
    // Given
    let fixture = must_fixture();
    let processes = FixedProcesses::default();
    let sources = fixed_sources(FixedClock, processes.clone(), None);
    let state = PoolState::load(fixture.config(), fixture.common.clone(), &sources).unwrap();
    let mut live = Live::new(state, &sources);
    let git = |live: &Live<'_, FixedClock, FixedProcesses>| -> Vec<GitStatus> {
        live.document().pools[0]
            .worktrees
            .iter()
            .map(|worktree| worktree.git)
            .collect()
    };
    let before = git(&live);
    std::fs::rename(&fixture.common, fixture.root.join("moved.git")).unwrap();

    // When
    live.refresh_all();

    // Then
    assert_eq!(git(&live), before);
}

fn unreferenced_in(line: &str, name: &str) -> Option<u64> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    value["pools"][0]["worktrees"]
        .as_array()?
        .iter()
        .find(|w| w["name"] == name)?["git"]["unreferenced"]
        .as_u64()
}

#[test]
fn watch__should_print_a_document_per_change_until_input_closes() {
    // Given
    let fixture = must_fixture();
    let (reader, writer) = std::io::pipe().unwrap();
    let output = SharedOutput::default();
    let (config, common) = (fixture.config(), fixture.common.clone());
    let (done, finished) = std::sync::mpsc::channel();
    thread::spawn({
        let mut output = output.clone();
        move || {
            let sources = fixed_sources(FixedClock, FixedProcesses::default(), None);
            done.send(watch(config, common, &sources, reader, &mut output))
        }
    });
    must_wait(|| output.lines().len() == 1);

    // When
    must_git(
        &fixture.root.join("wt1"),
        &["commit", "--quiet", "--allow-empty", "-m", "one"],
    );
    must_wait(|| output.lines().len() == 2);
    drop(writer);
    let result = finished
        .recv_timeout(Duration::from_secs(5))
        .expect("watch still runs after its input closed");

    // Then
    assert!(result.is_ok(), "{result:?}");
    let lines = output.lines();
    assert_eq!(unreferenced_in(&lines[0], "wt1"), Some(0));
    assert_eq!(unreferenced_in(&lines[1], "wt1"), Some(1));
}

#[test]
fn watch__should_fail_without_the_common_git_directory() {
    // Given
    let fixture = must_fixture();
    let gone = fixture.root.join("gone/.git");

    // When
    let result = watch(
        fixture.config(),
        gone.clone(),
        &Sources::system(|_| None, https_path),
        std::io::empty(),
        &mut Vec::new(),
    );

    // Then
    let message = result.unwrap_err().to_string();
    assert!(
        message.starts_with(&format!("could not watch {}: ", gone.display())),
        "{message}"
    );
}

#[test]
fn watch__should_fail_when_the_foreground_is_no_worktree() {
    // Given
    let fixture = must_fixture();
    let mut config = fixture.config();
    config.foreground.value = crate::paths::AbsolutePath::try_new(&fixture.root).unwrap();
    let mut output = Vec::new();

    // When
    let result = watch(
        config,
        fixture.common.clone(),
        &Sources::system(|_| None, https_path),
        std::io::empty(),
        &mut output,
    );

    // Then
    assert!(result.is_err() && output.is_empty(), "{result:?}");
}

/// A record of `s1` in wt1 whose last ended call is the Write `id`, or none.
fn write_call(fixture: &crate::status::fixture::Fixture, ended: Option<&str>, updated: &str) {
    let ended = ended.map_or("null".to_owned(), |id| {
        format!(r#"{{"id":"{id}","tool":"Write"}}"#)
    });
    let text = format!(
        r#"{{"version":1,"worktree":"{}","state":"working","updated":"{updated}","ended":{ended}}}"#,
        fixture.root.join("wt1").display()
    );
    std::fs::write(
        fixture
            .common
            .join(crate::activity::ACTIVITY_DIR)
            .join("s1.json"),
        text,
    )
    .unwrap();
}

fn dirty(live: &Live<'_, FixedClock, FixedProcesses>) -> Option<u64> {
    live.document().pools[0]
        .worktrees
        .iter()
        .find(|w| w.name == "wt1")
        .and_then(|w| w.git.dirty)
}

#[test]
fn live_refresh__should_read_a_made_folder_once_it_waits_for_one() {
    // Given
    let fixture = must_fixture();
    let processes = FixedProcesses::default();
    let paths = JobPaths {
        config: fixture.root.join("home/.config/wtpool/machine.toml"),
        folder: fixture.root.join("home/.state/wtpool/jobs"),
    };
    let sources = fixed_sources(FixedClock, processes.clone(), Some(paths.clone()));
    let state = PoolState::load(fixture.config(), fixture.common.clone(), &sources).unwrap();
    let (sender, _events) = std::sync::mpsc::channel();
    let watcher =
        notify::recommended_watcher(move |event| drop(sender.send(message_of(event)))).unwrap();
    let mut live = Live::new(state, &sources).with_folders(JobFolders::new(watcher, Some(&paths)));
    live.folders.as_mut().unwrap().watch_new();
    let mut pending = Pending::default();
    pending.machine_config = true;
    pending.records.insert(OsString::from("none.json"));

    // When
    live.refresh(&pending, Instant::now());
    let document = live.document();
    let waited = live.folders.as_ref().is_some_and(JobFolders::waits);
    std::fs::create_dir_all(&paths.folder).unwrap();
    std::fs::create_dir_all(paths.config.parent().unwrap()).unwrap();
    let mut jobs = Pending::default();
    jobs.jobs.insert(crate::jobs::JobId::new(1));
    live.refresh(&jobs, Instant::now());

    // Then
    assert!(document.machine.jobs.is_some());
    assert!(waited);
    assert!(!live.folders.as_ref().is_some_and(JobFolders::waits));
}

#[test]
fn live_refresh__should_read_the_repository_config_and_the_jobs_again() {
    // Given
    let fixture = must_fixture();
    let processes = FixedProcesses::default();
    let paths = JobPaths {
        config: fixture.root.join("home/.config/wtpool/machine.toml"),
        folder: fixture.root.join("home/.state/wtpool/jobs"),
    };
    let sources = fixed_sources(FixedClock, processes.clone(), Some(paths.clone()));
    let state = PoolState::load(fixture.config(), fixture.common.clone(), &sources).unwrap();
    let mut live = Live::new(state, &sources);
    std::fs::write(fixture.common.join("wtpool.toml"), "heavy = [\"just\"]\n").unwrap();
    let mut pending = Pending::default();
    pending.repository_config = true;
    pending.all_jobs = true;

    // When
    live.refresh(&pending, Instant::now());

    // Then
    let document = live.document();
    let jobs = document.machine.jobs.unwrap_or_else(|| panic!("no jobs"));
    assert_eq!(
        jobs.heavy
            .iter()
            .map(|entry| entry.pattern.as_str())
            .collect::<Vec<_>>(),
        ["just"]
    );
    assert_eq!(
        document.pools[0]
            .worktrees
            .iter()
            .flat_map(|worktree| worktree.builds.iter())
            .count(),
        0
    );
}

fn records(name: &str) -> Pending {
    let mut pending = Pending::default();
    pending.records.insert(OsString::from(name));
    pending
}

#[test]
fn live_refresh__should_refresh_git_after_an_edit_at_most_once_a_second() {
    // Given
    let fixture = must_fixture();
    let processes = FixedProcesses::default();
    let sources = fixed_sources(FixedClock, processes.clone(), None);
    write_call(&fixture, None, "2026-10-20T12:00:00Z");
    let state = PoolState::load(fixture.config(), fixture.common.clone(), &sources).unwrap();
    let mut live = Live::new(state, &sources);
    let wt1 = fixture.root.join("wt1");
    let start = Instant::now();
    let next = start + EDIT_REFRESH;

    // When
    std::fs::write(wt1.join("a.txt"), "x").unwrap();
    write_call(&fixture, Some("t1"), "2026-10-20T12:00:01Z");
    live.refresh(&records("s1.json"), start);
    let first = dirty(&live);
    std::fs::write(wt1.join("b.txt"), "x").unwrap();
    write_call(&fixture, Some("t2"), "2026-10-20T12:00:03Z");
    live.refresh(&records("s1.json"), start + EDIT_REFRESH / 2);
    let held = dirty(&live);
    let due = live.waiting.get(&wt1).copied();
    live.tick(next - Duration::from_millis(1));
    let early = dirty(&live);
    live.tick(next);
    let later = dirty(&live);

    // Then
    assert_eq!(
        (first, held, early, later),
        (Some(1), Some(1), Some(1), Some(2))
    );
    assert_eq!(due, Some(next));
    assert_eq!(live.edited.get(&wt1), Some(&next));
    assert!(live.waiting.is_empty());
}

#[test]
fn live_deadline__should_be_the_next_timer_or_waiting_refresh() {
    // Given
    let fixture = must_fixture();
    let processes = FixedProcesses::default();
    let sources = fixed_sources(FixedClock, processes.clone(), None);
    let state = PoolState::load(fixture.config(), fixture.common.clone(), &sources).unwrap();
    let mut live = Live::new(state, &sources);
    let now = Instant::now();
    let none = live.deadline(now);
    write_call(&fixture, None, "1970-01-01T00:00:30Z");
    live.refresh(&records("s1.json"), now);

    // When
    let timer = live.deadline(now);
    let soon = now + Duration::from_secs(3);
    live.waiting.insert(fixture.root.join("wt1"), soon);
    let waiting = live.deadline(now);

    // Then
    assert_eq!(none, None);
    assert_eq!(timer, Some(now + Duration::from_secs(108)));
    assert_eq!(waiting, Some(soon));
}

fn state_of(line: &str, key: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    value["pools"][0]["worktrees"]
        .as_array()?
        .iter()
        .flat_map(|w| w["actors"].as_array().cloned().unwrap_or_default())
        .find(|a| a["key"] == key)?["state"]
        .as_str()
        .map(str::to_owned)
}

#[test]
fn watch__should_fire_a_timer_when_the_clock_file_moves() {
    // Given
    let fixture = must_fixture();
    let clock = fixture.root.join("clock.txt");
    std::fs::write(&clock, "2026-10-20T12:01:59Z").unwrap();
    let text = format!(
        r#"{{"version":1,"worktree":"{}","state":"working","updated":"2026-10-20T12:00:00Z"}}"#,
        fixture.root.join("wt1").display()
    );
    std::fs::write(
        fixture
            .common
            .join(crate::activity::ACTIVITY_DIR)
            .join("s1.json"),
        text,
    )
    .unwrap();
    let (reader, writer) = std::io::pipe().unwrap();
    let output = SharedOutput::default();
    let (config, common) = (fixture.config(), fixture.common.clone());
    let (done, finished) = std::sync::mpsc::channel();
    thread::spawn({
        let mut output = output.clone();
        let clock = clock.clone();
        move || {
            let sources = fixed_sources(
                crate::clock::FileClock::new(clock),
                FixedProcesses::default(),
                None,
            );
            done.send(watch(config, common, &sources, reader, &mut output))
        }
    });
    must_wait(|| output.lines().len() == 1);

    // When
    std::fs::write(&clock, "2026-10-20T12:02:01Z").unwrap();
    must_wait(|| output.lines().len() == 2);
    drop(writer);
    let result = finished.recv_timeout(Duration::from_secs(5)).unwrap();

    // Then
    assert!(result.is_ok(), "{result:?}");
    let lines = output.lines();
    assert_eq!(state_of(&lines[0], "s1").as_deref(), Some("working"));
    assert_eq!(state_of(&lines[1], "s1").as_deref(), Some("quiet"));
}
