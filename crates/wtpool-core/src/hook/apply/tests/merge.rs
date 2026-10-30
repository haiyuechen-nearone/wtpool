//! A record a writer that started earlier writes over a newer one.

use rstest::rstest;

use super::*;

#[test]
fn merge_older__should_keep_the_state_and_take_the_calls() {
    // Given
    let newer = run(&[Event::CallStart(call("q", "ask", "h"))]);
    let older_result = apply(
        Some(&newer),
        &Event::TurnEnd {
            error: None,
            subagents_done: false,
        },
        &change("2026-10-20T12:00:09Z"),
    );

    // When
    let merged = merge_older(&newer, older_result, WAITING);

    // Then
    assert_eq!(merged.state, State::Waiting);
    assert!(merged.in_flight.is_empty());
    assert_eq!(merged.updated, at("2026-10-20T12:00:09Z"));
}

#[rstest]
#[case::wait_on_the_ended_call(
    call("c1", "run", "h"),
    false,
    State::Working,
    None,
    "2026-10-20T12:00:06Z"
)]
#[case::wait_on_another_call(
    call("c2", "read", "r"),
    false,
    State::Waiting,
    Some(WaitReason::Permission),
    "2026-10-20T12:00:05Z"
)]
#[case::question_still_in_flight(
    call("c1", "run", "h"),
    true,
    State::Waiting,
    Some(WaitReason::Question),
    "2026-10-20T12:00:06Z"
)]
fn merge_older__should_end_a_wait_whose_call_the_older_writer_ended(
    #[case] ended: CallFacts,
    #[case] question: bool,
    #[case] state: State,
    #[case] reason: Option<WaitReason>,
    #[case] since: &str,
) {
    // Given
    let before = run(&[
        Event::CallStart(call("c1", "run", "h")),
        Event::CallStart(call("c2", "read", "r")),
    ]);
    let mut newer = apply(
        Some(&before),
        &Event::Wait(
            WaitReason::Permission,
            CallRef::Facts(CallFacts {
                id: None,
                ..call("", "run", "h")
            }),
        ),
        &change("2026-10-20T12:00:05Z"),
    );
    if question {
        let pre = Event::CallStart(call("q", "ask", "a"));
        newer = apply(Some(&newer), &pre, &change("2026-10-20T12:00:05Z"));
    }
    let older = apply(
        Some(&newer),
        &Event::CallEnd(CallRef::Facts(ended.clone())),
        &change("2026-10-20T12:00:06Z"),
    );

    // When
    let merged = merge_older(&newer, older, WAITING);

    // Then
    assert_eq!(merged.state, state);
    assert_eq!(merged.waiting.map(|w| w.reason), reason);
    assert!(merged.done_ids.contains(&ended.id.unwrap()));
    assert_eq!(merged.since, at(since));
}

#[test]
fn merge_older__should_keep_the_subagent_titles_of_both_writers() {
    // Given
    let mut learnt = change("2026-10-20T12:00:00Z");
    learnt.found.subagent = Some(subagent("a1", "Find"));
    let base = apply(None, &Event::TurnStart, &learnt);
    let mut newer_change = change("2026-10-20T12:00:02Z");
    newer_change.found.subagent = Some(subagent("a2", "read"));
    let newer = apply(
        Some(&base),
        &Event::TurnEnd {
            error: None,
            subagents_done: false,
        },
        &newer_change,
    );
    let mut older_change = change("2026-10-20T12:00:01Z");
    older_change.found.subagent = Some(subagent("a3", "Test"));
    let older = apply(
        Some(&base),
        &Event::CallEnd(CallRef::Facts(call("c", "spawn", "h"))),
        &older_change,
    );

    // When
    let merged = merge_older(&newer, older, WAITING);

    // Then
    assert_eq!(
        merged.subagents,
        vec![
            subagent("a1", "Find"),
            subagent("a2", "read"),
            subagent("a3", "Test")
        ]
    );
    assert_eq!(merged.state, State::Idle);
}
