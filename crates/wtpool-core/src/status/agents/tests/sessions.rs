//! A newer main session of a process hides the older ones and their subagents.

use super::*;

fn session(
    key: &str,
    pid: u32,
    started: Option<i64>,
    updated: i64,
    state: RecordState,
) -> ActorRecord {
    let mut record = record(key, state);
    record.updated = at(updated);
    record.process = Some(RecordedProcess {
        pid,
        started: started.map(Timestamp::from_seconds),
    });
    record
}

#[test]
fn superseded__should_keep_the_newest_main_session_of_each_process() {
    // Given
    let mut unplaced = record("s7", RecordState::Idle);
    unplaced.process = None;
    let records = [
        session("s1", 7001, Some(START), 10, RecordState::Idle),
        session("s2", 7001, Some(START), 20, RecordState::Working),
        session("s2.a1", 7001, Some(START), 30, RecordState::Working),
        session("s3", 7001, Some(START + 5), 5, RecordState::Idle),
        session("s4", 7002, None, 1, RecordState::Idle),
        session("s5", 7002, None, 1, RecordState::Idle),
        session("s6", 7002, None, 9, RecordState::Ended),
        unplaced.clone(),
        ActorRecord {
            key: RecordKey::parse("s8").unwrap(),
            ..unplaced
        },
    ];
    let refs: Vec<&ActorRecord> = records.iter().collect();

    // When
    let hidden = superseded(&refs);

    // Then
    let expected: BTreeSet<String> = ["s1", "s5"].map(str::to_owned).into();
    assert_eq!(hidden, expected);
}

#[test]
fn superseded__should_break_ties_by_the_writer_start() {
    // Given
    let mut newer = session("s1", 7001, None, 10, RecordState::Idle);
    newer.writer_started = 2;
    let mut older = session("s2", 7001, None, 10, RecordState::Idle);
    older.writer_started = 1;

    // When
    let hidden = superseded(&[&older, &newer]);

    // Then
    assert_eq!(hidden, ["s2".to_owned()].into());
}

#[test]
fn actor__should_hide_a_replaced_session_and_its_subagents() {
    // Given
    let processes = [claude(None)];
    let hidden: BTreeSet<String> = ["s1".to_owned()].into();
    let context = Context {
        now: Timestamp::from_seconds(START),
        processes: &processes,
        records: &[],
        building: &|_: &Path, _: &str| false,
        superseded: &hidden,
        builds_below: &BTreeMap::new(),
        jobs: &BTreeMap::new(),
    };

    // When
    let shown: Vec<bool> = ["s1", "s1.a1", "s10", "s2"]
        .map(|key| actor(&record(key, RecordState::Idle), &context).is_some())
        .to_vec();

    // Then
    assert_eq!(shown, vec![false, false, true, true]);
}
