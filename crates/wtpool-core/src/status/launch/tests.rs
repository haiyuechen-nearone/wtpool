use std::path::PathBuf;

use rstest::rstest;

use super::*;
use crate::activity::PlacedBy;
use crate::clock::Timestamp;
use crate::processes::{RecordedProcess, StartTime};
use crate::status::record_time::RecordTime;
use crate::status::records::{RecordKey, RecordState};

const CLAUDE: u32 = 100;
const SERVER: u32 = 120;
const START: i64 = 1_792_497_600;

fn record(key: &str, pid: u32, shared: bool, updated: i64) -> ActorRecord {
    let at = RecordTime::parse(&Timestamp::from_seconds(START + updated).to_rfc3339());
    ActorRecord {
        key: RecordKey::parse(key).unwrap(),
        harness: "claude-code".to_owned(),
        label: key.to_owned(),
        worktree: PathBuf::from("/p/wt1"),
        placed_by: PlacedBy::Cwd,
        state: RecordState::Working,
        waiting: None,
        action: None,
        in_flight: Vec::new(),
        ended: None,
        error: None,
        since: at.clone().unwrap(),
        updated: at.unwrap(),
        writer_started: 0,
        process: Some(RecordedProcess { pid, started: None }),
        subagents: Vec::new(),
        tasks: Vec::new(),
        shared,
        model: None,
    }
}

fn process(pid: u32, parent: u32) -> RunningProcess {
    RunningProcess {
        pid,
        parent: Some(parent),
        started: None,
        name: String::new(),
        cwd: PathBuf::new(),
        cmd: Vec::new(),
    }
}

/// `claude` under pid 1, a shell under it, and an OpenCode server under that shell.
fn launch_table() -> Vec<RunningProcess> {
    vec![
        process(CLAUDE, 1),
        process(110, CLAUDE),
        process(SERVER, 110),
        process(130, SERVER),
        process(131, 130),
        process(140, 110),
    ]
}

/// The coordinator `s1` with an older session `s0` on its process, and the sessions `oc-a`
/// and `oc-b` of the server with a subagent of `oc-a`.
fn launch_records() -> Vec<ActorRecord> {
    vec![
        record("s0", CLAUDE, false, 0),
        record("s1", CLAUDE, false, 10),
        record("oc-a", SERVER, true, 20),
        record("oc-a.c1", SERVER, true, 30),
        record("oc-b", SERVER, true, 40),
    ]
}

#[test]
fn launchers__should_name_the_newest_session_of_the_first_harness_above() {
    // Given
    let table = launch_table();
    let records = launch_records();
    let live: Vec<&ActorRecord> = records.iter().collect();

    // When
    let launchers = HarnessTree::new(&table, &live).launchers(&live);

    // Then
    let expected = BTreeMap::from([
        ("oc-a".to_owned(), "s1".to_owned()),
        ("oc-b".to_owned(), "s1".to_owned()),
    ]);
    assert_eq!(launchers, expected);
}

#[rstest]
#[case::a_launcher_above("live", false, 1)]
#[case::under_a_shared_process("live", true, 1)]
#[case::a_launcher_of_another_start("restarted", false, 1)]
#[case::an_ended_launcher("ended", false, 1)]
#[case::a_launcher_at_16_levels("live", false, 16)]
#[case::a_launcher_at_17_levels("live", false, 17)]
fn launchers__should_name_none_past_a_shared_or_dead_process_or_16_levels(
    #[case] launcher: &str,
    #[case] launcher_shared: bool,
    #[case] levels: u32,
) {
    // Given
    let session = 500;
    let launcher_pid = 1000;
    let mut table = vec![RunningProcess {
        started: StartTime::from_second(Timestamp::from_seconds(START + 60)),
        ..process(launcher_pid, 1)
    }];
    let mut at = session;
    for _ in 1..levels {
        table.push(process(at, at + 1));
        at += 1;
    }
    table.push(process(at, launcher_pid));
    let mut launcher_record = record("launcher", launcher_pid, launcher_shared, 0);
    match launcher {
        "restarted" => {
            launcher_record.process = Some(RecordedProcess {
                pid: launcher_pid,
                started: Some(Timestamp::from_seconds(START)),
            });
        }
        "ended" => launcher_record.state = RecordState::Ended,
        _ => {}
    }
    let records = [launcher_record, record("launched", session, true, 10)];
    let counted: Vec<&ActorRecord> = records.iter().collect();

    // When
    let launchers = HarnessTree::new(&table, &counted).launchers(&counted);

    // Then
    let named = launchers.get("launched").map(String::as_str);
    let expected = (launcher == "live" && !launcher_shared && levels <= 16).then_some("launcher");
    assert_eq!(named, expected);
}

#[test]
fn launchers__should_never_go_to_pid_1() {
    // Given
    let table = [process(500, 1)];
    let records = [record("init", 1, false, 0), record("s1", 500, false, 10)];
    let live: Vec<&ActorRecord> = records.iter().collect();

    // When
    let launchers = HarnessTree::new(&table, &live).launchers(&live);

    // Then
    assert!(launchers.is_empty(), "{launchers:?}");
}

#[rstest]
#[case::coordinator(&[110, CLAUDE, 1], None, &["oc-a", "oc-b", "s0", "s1"])]
#[case::named_shared_session(&[131, 130, SERVER, 110, CLAUDE, 1], Some("oc-a"), &["oc-a"])]
#[case::shared_without_a_name(&[130, SERVER, 110, CLAUDE, 1], None, &[])]
#[case::named_subagent(&[130, SERVER, 110, CLAUDE, 1], Some("oc-a.c1"), &[])]
#[case::named_own_session(&[130, SERVER, 110, CLAUDE, 1], Some("s1"), &[])]
#[case::coordinator_with_a_name(&[140, 110, CLAUDE, 1], Some("oc-a"), &["oc-a", "oc-b", "s0", "s1"])]
#[case::named_from_another_process(&[7, 1], Some("oc-a"), &[])]
#[case::no_ancestry(&[], Some("oc-a"), &[])]
fn left_out__should_take_the_nearest_harness_and_only_the_named_shared_session(
    #[case] ancestors: &[u32],
    #[case] named_session: Option<&str>,
    #[case] expected: &[&str],
) {
    // Given
    let table = launch_table();
    let records = launch_records();
    let live: Vec<&ActorRecord> = records.iter().collect();
    let tree = HarnessTree::new(&table, &live);
    let launchers = tree.launchers(&live);
    let caller = Caller {
        ancestors,
        named_session,
    };

    // When
    let left_out = tree.left_out(&caller, &launchers);

    // Then
    let expected: BTreeSet<String> = expected.iter().map(|key| (*key).to_owned()).collect();
    assert_eq!(left_out, expected);
}

#[test]
fn left_out__should_take_a_named_session_only_within_16_levels() {
    // Given
    let records = [record("oc-a", 1000, true, 0)];
    let live: Vec<&ActorRecord> = records.iter().collect();
    let tree = HarnessTree::new(&[process(1000, 1)], &live);
    let at = |level: u32| {
        let mut ancestors: Vec<u32> = (2..=level).collect();
        ancestors.push(1000);
        ancestors
    };
    let (near, far) = (at(16), at(17));

    // When
    let left_out = [&near, &far].map(|ancestors| {
        let caller = Caller {
            ancestors,
            named_session: Some("oc-a"),
        };
        tree.left_out(&caller, &BTreeMap::new()).len()
    });

    // Then
    assert_eq!(left_out, [1, 0]);
}

#[rstest]
#[case::of_another_start("restarted")]
#[case::ended("ended")]
fn left_out__should_pass_a_harness_process_that_only_a_dead_record_names(#[case] dead: &str) {
    // Given
    let table = [
        process(200, 1),
        RunningProcess {
            started: StartTime::from_second(Timestamp::from_seconds(START + 60)),
            ..process(CLAUDE, 200)
        },
        process(110, CLAUDE),
    ];
    let mut old = record("old", CLAUDE, false, 0);
    match dead {
        "restarted" => {
            old.process = Some(RecordedProcess {
                pid: CLAUDE,
                started: Some(Timestamp::from_seconds(START)),
            });
        }
        _ => old.state = RecordState::Ended,
    }
    let records = [old, record("outer", 200, false, 10)];
    let counted: Vec<&ActorRecord> = records.iter().collect();
    let tree = HarnessTree::new(&table, &counted);
    let caller = Caller {
        ancestors: &[110, CLAUDE, 200, 1],
        named_session: None,
    };

    // When
    let left_out = tree.left_out(&caller, &tree.launchers(&counted));

    // Then
    assert_eq!(left_out, BTreeSet::from(["outer".to_owned()]));
    assert_eq!(tree.harness_pids(), BTreeSet::from([200]));
}
