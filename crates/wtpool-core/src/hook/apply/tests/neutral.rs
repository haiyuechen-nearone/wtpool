//! The events every harness's adapter may send: a session's start, an answer, the model's
//! progress and the names an event gives, and the waits on calls named by id.

use rstest::rstest;

use super::*;

const STOP: Event = Event::TurnEnd {
    error: None,
    subagents_done: false,
};

fn by_id(id: &str) -> CallRef {
    CallRef::Id(call_id(id))
}

fn asked(call: CallRef) -> Event {
    Event::Wait(WaitReason::Permission, call)
}

/// A working record at noon with the calls `ids` in flight, each of tool `run` and input `h`.
fn working(ids: &[&str]) -> Record {
    let mut events = vec![Event::TurnStart];
    events.extend(ids.iter().map(|id| Event::CallStart(call(id, "run", "h"))));
    let mut record = None;
    for event in &events {
        record = Some(apply(
            record.as_ref(),
            event,
            &change("2026-10-20T12:00:00Z"),
        ));
    }
    record.unwrap()
}

fn with_state(state: State) -> Record {
    Record {
        state,
        ..working(&[])
    }
}

fn waiting_on(id: &str) -> Record {
    apply(
        Some(&working(&[id])),
        &asked(by_id(id)),
        &change("2026-10-20T12:00:00Z"),
    )
}

#[rstest]
#[case::start_of_a_known_session(Event::SessionStart, Some(working(&[])), 0, false, true)]
#[case::start_of_a_new_session(Event::SessionStart, None, 0, false, false)]
#[case::info_of_an_unknown_subagent(Event::Info, None, 0, true, true)]
#[case::info_of_an_unknown_session(Event::Info, None, 0, false, false)]
#[case::info_of_a_known_subagent(Event::Info, Some(working(&[])), 0, true, false)]
#[case::progress_of_an_unknown_actor(Event::Progress, None, 30, false, true)]
#[case::progress_within_the_gap(Event::Progress, Some(working(&[])), 9, false, true)]
#[case::progress_after_the_gap(Event::Progress, Some(working(&[])), 10, false, false)]
#[case::progress_while_idle(Event::Progress, Some(with_state(State::Idle)), 30, false, true)]
#[case::progress_while_done(Event::Progress, Some(with_state(State::Done)), 30, false, true)]
#[case::progress_while_waiting(Event::Progress, Some(waiting_on("c1")), 30, false, true)]
#[case::progress_once_ended(Event::Progress, Some(with_state(State::Ended)), 30, false, true)]
#[case::answer_of_an_unknown_actor(Event::WaitEnd(None), None, 0, false, true)]
#[case::answer_without_a_wait(Event::WaitEnd(None), Some(working(&[])), 0, false, true)]
#[case::answer_to_another_call(
    Event::WaitEnd(Some(call_id("c2"))),
    Some(waiting_on("c1")),
    0,
    false,
    true
)]
#[case::answer_to_its_call(
    Event::WaitEnd(Some(call_id("c1"))),
    Some(waiting_on("c1")),
    0,
    false,
    false
)]
#[case::answer_without_a_call(Event::WaitEnd(None), Some(waiting_on("c1")), 0, false, false)]
#[case::any_other_event(STOP, None, 0, false, false)]
fn writes_nothing__should_skip_what_changes_nothing(
    #[case] event: Event,
    #[case] current: Option<Record>,
    #[case] seconds: i64,
    #[case] subagent: bool,
    #[case] skips: bool,
) {
    // When
    let skipped = writes_nothing(current.as_ref(), &event, later(seconds), subagent);

    // Then
    assert_eq!(skipped, skips);
}

#[rstest]
#[case::start(Event::SessionStart)]
#[case::info(Event::Info)]
fn session_start__should_create_an_idle_record_once(#[case] event: Event) {
    // When
    let record = apply(None, &event, &change("2026-10-20T12:00:00Z"));

    // Then
    assert_eq!(
        (record.state, record.placed_by, record.worktree.as_str()),
        (State::Idle, PlacedBy::Cwd, "/p/fg")
    );
    assert_eq!(record.label, "session 12:00");
}

#[test]
fn info__should_set_title_and_model_only() {
    // Given
    let current = working(&["c1"]);
    let mut named = change("2026-10-20T12:00:30Z");
    named.found.titles.generated = Some("Fix the build".to_owned());
    named.found.model = Some("provider/model-1".to_owned());
    named.writer.started_micros = 99;

    // When
    let record = apply(Some(&current), &Event::Info, &named);

    // Then
    let expected = Record {
        label: "Fix the build".to_owned(),
        title: Some(Title {
            text: "Fix the build".to_owned(),
            by: TitleBy::Harness,
        }),
        model: Some("provider/model-1".to_owned()),
        ..current
    };
    assert_eq!(record, expected);
}

#[rstest]
#[case::named(Some("model-2"), Some("model-2"))]
#[case::unnamed(None, Some("model-1"))]
fn apply__should_replace_the_model_only_when_named(
    #[case] named: Option<&str>,
    #[case] expected: Option<&str>,
) {
    // Given
    let mut first = change("2026-10-20T12:00:00Z");
    first.found.model = Some("model-1".to_owned());
    let record = apply(None, &Event::TurnStart, &first);
    let mut next = change("2026-10-20T12:00:01Z");
    next.found.model = named.map(str::to_owned);

    // When
    let record = apply(Some(&record), &STOP, &next);

    // Then
    assert_eq!(record.model.as_deref(), expected);
}

#[rstest]
#[case::no_call(&[], "generating")]
#[case::call_in_flight(&["c1"], "run")]
fn progress__should_keep_only_working_records_busy(#[case] calls: &[&str], #[case] tool: &str) {
    // Given
    let current = working(calls);

    // When
    let record = apply(
        Some(&current),
        &Event::Progress,
        &change("2026-10-20T12:00:10Z"),
    );

    // Then
    let action = record.action.unwrap();
    assert_eq!(
        (action.tool.as_str(), action.target.is_empty()),
        (tool, calls.is_empty())
    );
    assert_eq!(
        (record.state, record.since, record.updated),
        (State::Working, current.since, later(10))
    );
}

#[rstest]
#[case::its_id(asked(CallRef::Facts(call("c2", "run", "h"))), Some("c2"))]
#[case::id_alone(asked(by_id("c2")), Some("c2"))]
#[case::id_not_in_flight(asked(CallRef::Facts(call("c9", "run", "h"))), Some("c1"))]
#[case::tool_and_input(asked(CallRef::Facts(CallFacts { id: None, ..call("", "run", "h") })), Some("c1"))]
#[case::unknown_id_alone(asked(by_id("c9")), Some("c9"))]
fn wait__should_bind_by_call_id_first(#[case] event: Event, #[case] bound: Option<&str>) {
    // Given
    let current = working(&["c1", "c2"]);

    // When
    let record = apply(Some(&current), &event, &change("2026-10-20T12:00:01Z"));

    // Then
    let wait = record.waiting.unwrap();
    assert_eq!(wait.call.as_ref().map(CallId::as_str), bound);
    assert_eq!(record.state, State::Waiting);
}

#[test]
fn wait__should_take_the_tool_of_the_call_its_id_names() {
    // Given
    let current = working(&["c1"]);

    // When
    let record = apply(
        Some(&current),
        &Event::Wait(WaitReason::Question, by_id("c1")),
        &change("2026-10-20T12:00:01Z"),
    );

    // Then
    let wait = record.waiting.unwrap();
    assert_eq!(
        (wait.reason, wait.tool.as_deref(), wait.target.as_deref()),
        (WaitReason::Question, Some("run"), Some("t"))
    );
    assert_eq!(record.action.unwrap().tool, "run");
}

#[test]
fn wait__should_bind_a_call_that_starts_after_its_wait() {
    // Given
    let events = vec![
        Event::TurnStart,
        asked(by_id("c1")),
        Event::CallStart(call("c2", "read", "r")),
        Event::CallEnd(CallRef::Id(call_id("c2"))),
    ];
    let before = run(&events);

    // When
    let started = run(&with(events, &[Event::CallStart(call("c1", "run", "h"))]));
    let ended = apply(
        Some(&started),
        &Event::CallEnd(by_id("c1")),
        &change("2026-10-20T12:00:09Z"),
    );

    // Then
    assert_eq!(before.state, State::Waiting);
    let wait = started.waiting.unwrap();
    assert_eq!(
        (wait.call.as_ref().map(CallId::as_str), wait.tool.as_deref()),
        (Some("c1"), Some("run"))
    );
    assert_eq!((ended.state, ended.waiting), (State::Working, None));
}

#[rstest]
#[case::its_call(Some("c1"), State::Working, true)]
#[case::without_a_call(None, State::Working, true)]
#[case::another_call(Some("c2"), State::Waiting, false)]
fn wait_end__should_end_only_its_wait(
    #[case] call: Option<&str>,
    #[case] state: State,
    #[case] ended: bool,
) {
    // Given
    let current = waiting_on("c1");

    // When
    let record = apply(
        Some(&current),
        &Event::WaitEnd(call.map(call_id)),
        &change("2026-10-20T12:00:01Z"),
    );

    // Then
    assert_eq!((record.state, record.waiting.is_none()), (state, ended));
}

#[test]
fn wait_end__should_not_wait_again_on_its_question() {
    // Given
    let question = call("q1", "ask", "h");
    let current = run(&[Event::TurnStart, Event::CallStart(question)]);

    // When
    let answered = apply(
        Some(&current),
        &Event::WaitEnd(Some(call_id("q1"))),
        &change("2026-10-20T12:00:05Z"),
    );
    let ended = apply(
        Some(&answered),
        &Event::CallEnd(by_id("q1")),
        &change("2026-10-20T12:00:06Z"),
    );

    // Then
    assert_eq!(current.state, State::Waiting);
    assert_eq!((answered.state, answered.waiting), (State::Working, None));
    assert_eq!((ended.state, ended.in_flight.len()), (State::Working, 0));
}

#[rstest]
#[case::in_flight(&["c1"], Some(("run", Some(CallKind::Shell))))]
#[case::unknown(&[], None)]
fn call_end__should_take_the_tool_of_a_call_named_by_id(
    #[case] calls: &[&str],
    #[case] expected: Option<(&str, Option<CallKind>)>,
) {
    // Given
    let current = working(calls);

    // When
    let record = apply(
        Some(&current),
        &Event::CallEnd(by_id("c1")),
        &change("2026-10-20T12:00:01Z"),
    );

    // Then
    let ended = record.ended.as_ref().map(|e| (e.tool.as_str(), e.kind));
    assert_eq!(ended, expected);
    assert_eq!(record.done_ids, vec![call_id("c1")]);
}

#[test]
fn apply__should_label_a_subagent_with_its_title_over_its_type() {
    // Given
    let mut start = change("2026-10-20T12:00:00Z");
    start.fresh.agent = Some(Agent {
        id: ActorId::try_from("a1").unwrap(),
        agent_type: Some("explore".to_owned()),
    });
    start.fresh.label = "explore".to_owned();
    start.found.label = Some("explore".to_owned());
    start.found.titles.generated = Some("Read the notes".to_owned());
    let mut typed = change("2026-10-20T12:00:01Z");
    typed.found.label = Some("explore".to_owned());
    let mut renamed = change("2026-10-20T12:00:02Z");
    renamed.found.titles.generated = Some("Read the plan".to_owned());

    // When
    let started = apply(None, &Event::SubagentStart, &start);
    let worked = apply(Some(&started), &Event::SubagentStart, &typed);
    let named = apply(Some(&worked), &Event::Info, &renamed);

    // Then
    let labels = [&started, &worked, &named].map(|record| record.label.as_str());
    assert_eq!(
        labels,
        ["Read the notes", "Read the notes", "Read the plan"]
    );
}
