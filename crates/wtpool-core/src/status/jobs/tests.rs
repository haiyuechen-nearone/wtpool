use std::path::PathBuf;

use super::*;
use crate::activity::PlacedBy;
use crate::processes::{RecordedProcess, StartTime};
use crate::status::record_time::RecordTime;
use crate::status::records::{RecordKey, RecordState, RecordedCall, ToolTarget};

const START: &str = "2026-10-20T11:00:00Z";

fn at(seconds: i64) -> RecordTime {
    let base = Timestamp::parse(START).unwrap().seconds();
    RecordTime::parse(&Timestamp::from_seconds(base + seconds).to_rfc3339()).unwrap()
}

/// The record `key` of the harness process `harness`, with a shell call on `target` in flight
/// when there is one, last changed `since` seconds after the start.
fn record(key: &str, harness: u32, target: Option<&str>, since: i64) -> ActorRecord {
    ActorRecord {
        key: RecordKey::parse(key).unwrap(),
        harness: "claude-code".to_owned(),
        label: "label".to_owned(),
        worktree: PathBuf::from("/p/wt1"),
        placed_by: PlacedBy::Cwd,
        state: RecordState::Working,
        waiting: None,
        action: target.map(|target| ToolTarget {
            tool: "Bash".to_owned(),
            target: target.to_owned(),
        }),
        in_flight: target
            .map(|_| RecordedCall {
                id: format!("toolu_{key}"),
                kind: CallKind::Shell,
            })
            .into_iter()
            .collect(),
        ended: None,
        error: None,
        since: at(since),
        updated: at(since),
        writer_started: 0,
        process: Some(RecordedProcess {
            pid: harness,
            started: Timestamp::parse(START),
        }),
        subagents: Vec::new(),
        tasks: Vec::new(),
    }
}

fn process(pid: u32, parent: u32, cmd: &[&str]) -> RunningProcess {
    RunningProcess {
        pid,
        parent: Some(parent),
        started: StartTime::parse(START),
        name: cmd.first().copied().unwrap_or_default().to_owned(),
        cwd: PathBuf::from("/p/wt1"),
        cmd: cmd.iter().map(|word| (*word).to_owned()).collect(),
    }
}

fn job(pid: u32, ticket: u64, state: JobState, label: Option<&str>) -> JobRecord {
    JobRecord {
        summary: "cargo test".to_owned(),
        label: label.map(str::to_owned),
        cwd: PathBuf::from("/p/wt1"),
        ticket,
        state,
        ..JobRecord::sample(pid)
    }
}

#[test]
fn harnesses_actor_of__should_take_the_nearest_harness_and_its_call_on_the_summary() {
    // Given
    let table = [
        process(100, 1, &["claude"]),
        process(110, 100, &["zsh"]),
        process(111, 110, &["wtpool", "job", "--", "cargo", "build"]),
        process(120, 100, &["wtpool", "job", "--", "cargo", "check"]),
        process(200, 100, &["claude"]),
        process(201, 200, &["wtpool", "job", "--", "cargo", "test"]),
        process(300, 1, &["wtpool", "job", "--", "cargo", "test"]),
    ];
    let records = [
        record("s1", 100, Some("cargo test"), 0),
        record("s1.a1", 100, Some("cargo build"), 10),
        record("s1.a2", 100, Some("cargo build"), 20),
        record("s2", 200, None, 30),
    ];
    let records: Vec<&ActorRecord> = records.iter().collect();
    let heavy = HeavyTable::new(&table, &HeavyList::default(), &HeavyList::default(), &[]);
    let harnesses = Harnesses::new(&records);

    // When
    let actors = [
        harnesses.actor_of(111, "cargo build", &heavy),
        harnesses.actor_of(120, "cargo check", &heavy),
        harnesses.actor_of(201, "cargo test", &heavy),
        harnesses.actor_of(300, "cargo test", &heavy),
    ];

    // Then
    assert_eq!(
        actors,
        [
            Some("s1.a2".to_owned()),
            Some("s1".to_owned()),
            Some("s2".to_owned()),
            None
        ]
    );
}

#[test]
fn harnesses_actor_of__should_skip_a_harness_of_another_start() {
    // Given
    let mut restarted = process(100, 1, &["claude"]);
    restarted.started = StartTime::parse("2026-10-20T12:00:00Z");
    let table = [restarted, process(101, 100, &["cargo", "test"])];
    let records = [record("s1", 100, Some("cargo test"), 0)];
    let records: Vec<&ActorRecord> = records.iter().collect();
    let heavy = HeavyTable::new(&table, &HeavyList::default(), &HeavyList::default(), &[]);

    // When
    let actor = Harnesses::new(&records).actor_of(101, "cargo test", &heavy);

    // Then
    assert_eq!(actor, None);
}

#[test]
fn actor_jobs__should_name_the_first_waiting_job_by_its_label_else_its_summary() {
    // Given
    let jobs = [
        job(1, 3, JobState::Waiting, None),
        job(2, 2, JobState::Waiting, Some("unit tests")),
        job(3, 1, JobState::Running, None),
        job(4, 4, JobState::Waiting, None),
        job(5, 5, JobState::Running, None),
    ];
    let jobs: Vec<&JobRecord> = jobs.iter().collect();
    let owners = [
        Some("s1".to_owned()),
        Some("s1".to_owned()),
        Some("s1".to_owned()),
        Some("s2".to_owned()),
        None,
    ];

    // When
    let held = actor_jobs(&jobs, &owners);

    // Then
    let s1 = ActorJob {
        waiting: Some("unit tests".to_owned()),
        running: true,
    };
    let s2 = ActorJob {
        waiting: Some("cargo test".to_owned()),
        running: false,
    };
    assert_eq!(
        held,
        BTreeMap::from([("s1".to_owned(), s1), ("s2".to_owned(), s2)])
    );
}

#[test]
fn pool_folders_worktree_of__should_take_the_deepest_worktree() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join("fg/inner/src")).unwrap();
    std::fs::create_dir_all(root.join("fg10")).unwrap();
    let folders = PoolFolders(vec![
        ("/listed/fg".to_owned(), root.join("fg")),
        ("/listed/inner".to_owned(), root.join("fg/inner")),
    ]);

    // When
    let found = [
        folders.worktree_of(&root.join("fg/inner/src")),
        folders.worktree_of(&root.join("fg")),
        folders.worktree_of(&root.join("fg10")),
    ];

    // Then
    assert_eq!(
        found,
        [
            Some("/listed/inner".to_owned()),
            Some("/listed/fg".to_owned()),
            None
        ]
    );
}

#[test]
fn jobs_state_build_suggestions__should_stand_in_only_while_the_repository_lists_none() {
    // Given
    let suggestion = |pattern: &str, from: &str| Suggestion {
        pattern: Pattern::parse(pattern).unwrap(),
        from: from.to_owned(),
    };
    let mut state = JobsState {
        machine: None,
        repository: RepositoryPatterns::default(),
        suggested: vec![
            suggestion("make", "Makefile"),
            suggestion("cargo test", "Cargo.toml"),
        ],
        markers: PathBuf::from("/p/fg"),
    };
    let listed = |list: &HeavyList| -> Vec<String> {
        list.patterns()
            .iter()
            .map(|listed| listed.pattern.as_str().to_owned())
            .collect()
    };

    // When
    let before = listed(&state.build_suggestions(&state.heavy()));
    state.repository.build_processes = vec!["protoc".to_owned()];
    let after = listed(&state.build_suggestions(&state.heavy()));

    // Then
    assert_eq!(before, vec!["make".to_owned(), "cargo test".to_owned()]);
    assert!(after.is_empty());
}
