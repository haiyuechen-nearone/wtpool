//! A background task an actor started keeps it working once its turn ended.

use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::status::outputs;

/// A task `id` running `summary`, started `started` seconds after START and ended `ended`
/// seconds after it, when given.
fn task(id: &str, summary: &str, started: i64, ended: Option<i64>) -> Value {
    let time = |seconds: i64| Timestamp::from_seconds(START + seconds).to_rfc3339();
    json!({
        "id": id,
        "path": format!("/t/claude-501/p/s1/tasks/{id}.output"),
        "summary": summary,
        "started": time(started),
        "ended": ended.map(time),
    })
}

fn running(id: &str, summary: &str, started: i64) -> Value {
    task(id, summary, started, None)
}

/// `record` with the action `Read f.txt` and `tasks`.
fn with_tasks(mut record: ActorRecord, tasks: &[Value]) -> ActorRecord {
    record.action = Some(tool_target("Read", "f.txt"));
    record.tasks = outputs::tasks(serde_json::from_value(Value::from(tasks.to_vec())).unwrap());
    record
}

/// The state and action `subject` shows `after` seconds, while `cargo` runs below the claude
/// process when `build`.
fn shown(
    subject: &ActorRecord,
    after: i64,
    build: bool,
) -> (Option<ActorState>, Option<(String, String)>) {
    let processes = [claude(Some(START - 3600))];
    let below = if build {
        BTreeMap::from([(CLAUDE, vec![below("cargo")])])
    } else {
        BTreeMap::new()
    };
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

fn action(tool: &str, target: &str) -> Option<(String, String)> {
    Some((tool.to_owned(), target.to_owned()))
}

#[rstest]
#[case::session_idle(
    with_tasks(record("s1", RecordState::Idle), &[running("b1", "cargo test", 0)]),
    3600,
    false,
    Some(ActorState::Working),
    action(BACKGROUND_TOOL, "cargo test")
)]
#[case::session_quiet(
    with_tasks(record("s1", RecordState::Working), &[running("b1", "cargo test", 0)]),
    300,
    false,
    Some(ActorState::Working),
    action(BACKGROUND_TOOL, "cargo test")
)]
#[case::session_working_keeps_its_action(
    with_tasks(record("s1", RecordState::Working), &[running("b1", "cargo test", 0)]),
    10,
    false,
    Some(ActorState::Working),
    action("Read", "f.txt")
)]
#[case::session_waiting(
    with_tasks(record("s1", RecordState::Waiting), &[running("b1", "cargo test", 0)]),
    10,
    false,
    Some(ActorState::Waiting),
    action("Read", "f.txt")
)]
#[case::subagent_idle(
    with_tasks(record("s1.a1", RecordState::Idle), &[running("b1", "cargo test", 0)]),
    10,
    false,
    Some(ActorState::Working),
    action(BACKGROUND_TOOL, "cargo test")
)]
#[case::subagent_done(
    with_tasks(record("s1.a1", RecordState::Done), &[running("b1", "cargo test", 0)]),
    10,
    false,
    Some(ActorState::Working),
    action(BACKGROUND_TOOL, "cargo test")
)]
#[case::subagent_done_past_its_minute(
    with_tasks(record("s1.a1", RecordState::Done), &[running("b1", "cargo test", 0)]),
    3600,
    false,
    Some(ActorState::Working),
    action(BACKGROUND_TOOL, "cargo test")
)]
#[case::session_after_its_task_ended(
    with_tasks(record("s1", RecordState::Idle), &[task("b1", "cargo test", 0, Some(5))]),
    3600,
    false,
    Some(ActorState::Idle),
    action("Read", "f.txt")
)]
#[case::subagent_after_its_task_ended(
    with_tasks(record("s1.a1", RecordState::Done), &[task("b1", "cargo test", 0, Some(5))]),
    3600,
    false,
    None,
    None
)]
#[case::newest_running_task(
    with_tasks(
        record("s1", RecordState::Idle),
        &[
            running("b1", "cargo build", 0),
            running("b2", "cargo test", 5),
            task("b3", "npm ci", 9, Some(10)),
        ]
    ),
    60,
    false,
    Some(ActorState::Working),
    action(BACKGROUND_TOOL, "cargo test")
)]
#[case::own_build_first(
    with_tasks(record("s1", RecordState::Idle), &[running("b1", "cargo test", 0)]),
    10,
    true,
    Some(ActorState::Working),
    action(BUILD_TOOL, "cargo")
)]
#[case::ended(
    with_tasks(record("s1", RecordState::Ended), &[running("b1", "cargo test", 0)]),
    10,
    false,
    None,
    None
)]
fn actor__should_work_while_a_background_task_runs(
    #[case] subject: ActorRecord,
    #[case] after: i64,
    #[case] build: bool,
    #[case] state: Option<ActorState>,
    #[case] action: Option<(String, String)>,
) {
    // When
    let found = shown(&subject, after, build);

    // Then
    assert_eq!(found, (state, action));
}
