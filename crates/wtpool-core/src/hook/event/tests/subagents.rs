//! The changes a session's events make to the records of its subagents.

use super::*;

/// A shell call of the subagent `agent` of `s1`, which makes its working record.
fn subagent_call(pool: &Pool, agent: &str) -> HookInput {
    HookInput {
        agent_id: Some(ActorId::try_from(agent).unwrap()),
        ..pool.input(Event::CallStart(call("c1", "run", CallKind::Shell)))
    }
}

/// Rewrites the record of the subagent `a1` of `s1` as another harness's, and returns it.
fn must_make_a1_foreign(pool: &Pool) -> Record {
    let activity = ActivityDir::open(&pool.root.join("fg/.git")).unwrap();
    let mut foreign = activity.read("s1.a1").unwrap();
    foreign.harness = HarnessName::new("other-harness");
    activity.write("s1.a1", &foreign).unwrap();
    foreign
}

const SUBAGENTS_DONE: Event = Event::TurnEnd {
    error: None,
    subagents_done: true,
};

#[test]
fn handle__should_log_a_finished_subagent_it_cannot_write() {
    // Given
    let pool = Pool::must_new();
    pool.send(subagent_call(&pool, "a1"));
    let activity = ActivityDir::open(&pool.root.join("fg/.git")).unwrap();
    let temp = activity
        .record_path("s1.a1")
        .with_file_name(format!(".s1.a1.json.{}.tmp", std::process::id()));
    std::fs::create_dir(&temp).unwrap();
    std::fs::write(temp.join("x"), "").unwrap();

    // When
    pool.send(pool.input(SUBAGENTS_DONE));

    // Then
    let log = pool.log();
    assert!(
        log.starts_with(
            "wtpool hook: skipped a change to the record s1.a1: it cannot be written: "
        ),
        "{log:?}"
    );
    assert_eq!(pool.record("s1.a1").unwrap().state, State::Working);
}

#[test]
fn handle__should_leave_a_foreign_subagent_as_it_is_at_a_turn_end() {
    // Given
    let pool = Pool::must_new();
    pool.send(subagent_call(&pool, "a1"));
    let foreign = must_make_a1_foreign(&pool);

    // When
    pool.send(pool.input(SUBAGENTS_DONE));

    // Then
    assert_eq!(pool.record("s1.a1"), Some(foreign));
    assert_eq!(
        pool.log(),
        "wtpool hook: skipped a change to the record s1.a1: it names another harness\n"
    );
}

#[test]
fn handle__should_keep_a_subagent_whose_lock_cannot_be_taken_at_the_session_end() {
    // Given
    let pool = Pool::must_new();
    pool.send(subagent_call(&pool, "a1"));
    let lock = pool.root.join("fg/.git/wtpool-activity/s1.a1.json.lock");
    let _ = std::fs::remove_file(&lock);
    std::fs::create_dir(&lock).unwrap();

    // When
    pool.send(pool.input(Event::SessionEnd));

    // Then
    assert!(pool.record("s1.a1").is_some());
    assert_eq!(pool.log(), "");
}

#[test]
fn handle__should_delete_the_subagents_but_a_foreign_one_at_the_session_end() {
    // Given
    let pool = Pool::must_new();
    pool.send(subagent_call(&pool, "a1"));
    must_make_a1_foreign(&pool);
    pool.send(subagent_call(&pool, "a2"));

    // When
    pool.send(pool.input(Event::SessionEnd));

    // Then
    assert!(pool.record("s1.a1").is_some());
    assert!(pool.record("s1.a2").is_none());
    let log = pool.log();
    assert!(
        log.contains("skipped a change to the record s1.a1: it names another harness"),
        "{log}"
    );
}
