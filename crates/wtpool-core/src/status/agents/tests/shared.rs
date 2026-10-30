//! A process that runs many sessions: none replaces another, none works on a build, and an idle
//! or quiet one hides after an hour or behind a newer one in its worktree.

use rstest::rstest;

use super::*;

fn shared(mut record: ActorRecord) -> ActorRecord {
    record.shared = true;
    record
}

fn updated_at(mut record: ActorRecord, seconds: i64) -> ActorRecord {
    record.updated = at(seconds);
    record
}

#[rstest]
#[case::idle_at_59_min_59_s(
    shared(record("s1", RecordState::Idle)),
    3599,
    false,
    Some(ActorState::Idle)
)]
#[case::idle_at_an_hour(shared(record("s1", RecordState::Idle)), 3600, false, None)]
#[case::quiet_at_59_min_59_s(
    shared(record("s1", RecordState::Working)),
    3599,
    false,
    Some(ActorState::Quiet)
)]
#[case::quiet_at_an_hour(shared(record("s1", RecordState::Working)), 3600, false, None)]
#[case::subagent_at_an_hour(shared(record("s1.a1", RecordState::Working)), 3600, false, None)]
#[case::waiting_kept(
    shared(record("s1", RecordState::Waiting)),
    7200,
    false,
    Some(ActorState::Waiting)
)]
#[case::shell_during_a_build_kept(shared(with_calls(record("s1", RecordState::Working), &[CallKind::Shell])), 7200, true, Some(ActorState::Working))]
#[case::own_process_kept(record("s1", RecordState::Idle), 7200, false, Some(ActorState::Idle))]
fn actor__should_hide_a_shared_actor_an_hour_after_its_last_event(
    #[case] subject: ActorRecord,
    #[case] after: i64,
    #[case] build: bool,
    #[case] expected: Option<ActorState>,
) {
    // When
    let state = state_after(&subject, &[], after, build);

    // Then
    assert_eq!(state, expected);
}

#[rstest]
#[case::newer_beside(
    shared(record("s2", RecordState::Working)),
    "/p/wt1",
    RecordState::Idle,
    None
)]
#[case::newer_idle_beside(
    shared(record("s2", RecordState::Idle)),
    "/p/wt1",
    RecordState::Idle,
    None
)]
#[case::newer_in_another_worktree(
    shared(record("s2", RecordState::Working)),
    "/p/wt2",
    RecordState::Idle,
    Some(ActorState::Idle)
)]
#[case::older_one_working(
    shared(record("s2", RecordState::Working)),
    "/p/wt1",
    RecordState::Working,
    Some(ActorState::Working)
)]
#[case::newer_subagent(
    shared(record("s1.a1", RecordState::Working)),
    "/p/wt1",
    RecordState::Idle,
    Some(ActorState::Idle)
)]
#[case::own_process(
    record("s2", RecordState::Working),
    "/p/wt1",
    RecordState::Idle,
    Some(ActorState::Idle)
)]
fn actor__should_hide_an_older_idle_shared_session_behind_a_newer_one(
    #[case] newer: ActorRecord,
    #[case] worktree: &str,
    #[case] older_state: RecordState,
    #[case] expected: Option<ActorState>,
) {
    // Given
    let mut newer = updated_at(newer, 10);
    newer.worktree = PathBuf::from(worktree);
    let older = record("s1", older_state);
    let older = if newer.shared { shared(older) } else { older };

    // When
    let state = state_after(&older, &[newer], 20, false);

    // Then
    assert_eq!(state, expected);
}

#[test]
fn actor__should_keep_the_newer_shared_session_and_tell_newer_by_writer() {
    // Given
    let older = shared(record("s1", RecordState::Idle));
    let mut newer = shared(record("s2", RecordState::Idle));
    newer.writer_started = 1;

    // When
    let states = (
        state_after(&older, std::slice::from_ref(&newer), 20, false),
        state_after(&newer, std::slice::from_ref(&older), 20, false),
    );

    // Then
    assert_eq!(states, (None, Some(ActorState::Idle)));
}

#[test]
fn superseded__should_replace_no_shared_session() {
    // Given
    let records = [
        updated_at(shared(record("s1", RecordState::Idle)), 0),
        updated_at(shared(record("s2", RecordState::Working)), 10),
    ];
    let refs: Vec<&ActorRecord> = records.iter().collect();

    // When
    let hidden = superseded(&refs);

    // Then
    assert!(hidden.is_empty(), "{hidden:?}");
}

#[test]
fn actor__should_never_work_on_a_build_of_a_shared_process() {
    // Given
    let subject = shared(record("s1", RecordState::Idle));
    let processes = [claude(Some(START - 3600))];
    let below = BTreeMap::from([(CLAUDE, vec![below("cargo")])]);
    let records = [&subject];
    let context = Context {
        now: Timestamp::from_seconds(START + 10),
        processes: &processes,
        records: &records,
        building: &|_: &Path, _: &str| false,
        superseded: &BTreeSet::new(),
        builds_below: &below,
        jobs: &BTreeMap::new(),
    };

    // When
    let shown = actor(&subject, &context).unwrap();

    // Then
    assert_eq!((shown.state, shown.action), (ActorState::Idle, None));
}

#[test]
fn next_change__should_give_the_hour_a_shared_actor_hides_at() {
    // Given
    let idle = shared(record("s1", RecordState::Idle));
    let working = updated_at(shared(record("s2", RecordState::Working)), 30);
    let waiting = shared(record("s3", RecordState::Waiting));
    let own = record("s4", RecordState::Idle);
    let records = [&idle, &working, &waiting, &own];

    // When
    let first = next_change(&records, Timestamp::from_seconds(START + 100));
    let second = next_change(&records, Timestamp::from_seconds(START + 200));
    let third = next_change(&records, Timestamp::from_seconds(START + 3600));
    let none = next_change(&records, Timestamp::from_seconds(START + 3630));

    // Then
    assert_eq!(
        [first, second, third, none],
        [
            Some(Timestamp::from_seconds(START + 150)),
            Some(Timestamp::from_seconds(START + 3600)),
            Some(Timestamp::from_seconds(START + 3630)),
            None
        ]
    );
}

#[test]
fn actor__should_name_the_model_of_its_record() {
    // Given
    let mut subject = record("s1", RecordState::Idle);
    subject.model = Some("opencode/glm-5.3-flash-free".to_owned());
    let processes = [claude(Some(START - 3600))];
    let records = [&subject];
    let context = Context {
        now: Timestamp::from_seconds(START + 10),
        processes: &processes,
        records: &records,
        building: &|_: &Path, _: &str| false,
        superseded: &BTreeSet::new(),
        builds_below: &BTreeMap::new(),
        jobs: &BTreeMap::new(),
    };

    // When
    let shown = actor(&subject, &context).unwrap();

    // Then
    assert_eq!(shown.model.as_deref(), Some("opencode/glm-5.3-flash-free"));
    assert_eq!(shown.launched_by, None);
}
