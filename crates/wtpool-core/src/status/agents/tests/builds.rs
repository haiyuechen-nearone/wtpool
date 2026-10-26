//! A build below its session's process keeps an actor working.

use rstest::rstest;

use super::*;

/// The state and action `subject` shows `after` seconds, while `cargo-nextest` runs below the
/// claude process.
fn with_own_build(subject: &Record, after: i64) -> (Option<ActorState>, Option<(String, String)>) {
    let processes = [claude(Some(START - 3600))];
    let below = BTreeMap::from([(CLAUDE, vec![below("cargo-nextest")])]);
    let records = [subject];
    let context = Context {
        now: Timestamp::from_seconds(START + after),
        processes: &processes,
        records: &records,
        building: &|_: &Path, _: &str| false,
        superseded: &BTreeSet::new(),
        builds_below: &below,
        jobs: &BTreeMap::new(),
    };
    let shown = actor(subject, &context);
    (
        shown.as_ref().map(|actor| actor.state),
        shown
            .and_then(|actor| actor.action)
            .map(|action| (action.tool, action.target)),
    )
}

fn build_action() -> Option<(String, String)> {
    Some((BUILD_TOOL.to_owned(), "cargo-nextest".to_owned()))
}

fn read_action() -> Option<(String, String)> {
    Some(("Read".to_owned(), "f.txt".to_owned()))
}

#[rstest]
#[case::idle(
    record("s1", RecordState::Idle),
    3600,
    Some(ActorState::Working),
    build_action()
)]
#[case::quiet(
    record("s1", RecordState::Working),
    300,
    Some(ActorState::Working),
    build_action()
)]
#[case::working(
    record("s1", RecordState::Working),
    10,
    Some(ActorState::Working),
    read_action()
)]
#[case::waiting(
    record("s1", RecordState::Waiting),
    10,
    Some(ActorState::Waiting),
    read_action()
)]
#[case::subagent_idle(
    record("s1.a1", RecordState::Idle),
    10,
    Some(ActorState::Idle),
    read_action()
)]
#[case::subagent_done(
    record("s1.a1", RecordState::Done),
    10,
    Some(ActorState::Done),
    read_action()
)]
#[case::ended(record("s1", RecordState::Ended), 10, None, None)]
fn actor__should_work_while_a_build_of_its_session_runs(
    #[case] mut subject: Record,
    #[case] after: i64,
    #[case] state: Option<ActorState>,
    #[case] action: Option<(String, String)>,
) {
    // Given
    subject.action = Some(tool_target("Read", "f.txt"));

    // When
    let shown = with_own_build(&subject, after);

    // Then
    assert_eq!(shown, (state, action));
}

#[test]
fn actor__should_not_work_on_a_build_of_another_process_or_without_one() {
    // Given
    let subject = record("s1", RecordState::Idle);
    let mut unplaced = record("s2", RecordState::Idle);
    unplaced.process = None;
    let processes = [claude(Some(START - 3600))];
    let below = BTreeMap::from([(CLAUDE + 1, vec![below("cargo")])]);
    let context = Context {
        now: Timestamp::from_seconds(START),
        processes: &processes,
        records: &[],
        building: &|_: &Path, _: &str| true,
        superseded: &BTreeSet::new(),
        builds_below: &below,
        jobs: &BTreeMap::new(),
    };

    // When
    let states = [&subject, &unplaced].map(|record| actor(record, &context).map(|a| a.state));

    // Then
    assert_eq!(states, [Some(ActorState::Idle), Some(ActorState::Idle)]);
}
