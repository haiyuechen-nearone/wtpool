use rstest::rstest;

use super::*;
use crate::activity::{CallId, CallKind};
use crate::hook::fixture::later;

fn at(text: &str) -> Timestamp {
    Timestamp::parse(text).unwrap()
}

fn call_id(id: &str) -> CallId {
    CallId::try_from(id).unwrap()
}

mod merge;
mod neutral;
mod titles;

/// The tools of a harness that wait for the user: `ask` for an answer, `plan` for a plan's
/// approval.
const WAITING: WaitingTools =
    WaitingTools(&[("ask", WaitReason::Question), ("plan", WaitReason::Plan)]);

/// The kind of a call of a harness's tool: `run` runs a shell, `change` edits, `spawn` starts a
/// subagent.
fn kind_of(tool: &str) -> CallKind {
    match tool {
        "run" => CallKind::Shell,
        "change" => CallKind::Edit,
        "spawn" => CallKind::Agent,
        _ => CallKind::Other,
    }
}

fn change(now: &str) -> Change {
    change_at(at(now))
}

fn change_at(now: Timestamp) -> Change {
    Change {
        writer: Writer {
            harness: HarnessName::LEGACY,
            waiting_tools: WAITING,
            now,
            started_micros: 10,
            process: Some(HarnessProcess {
                pid: 7001,
                started: "2026-10-20T11:00:00Z".to_owned(),
                shared: false,
            }),
        },
        fresh: Fresh {
            session: ActorId::try_from("s1").unwrap(),
            agent: None,
            place: Place {
                worktree: "/p/fg".to_owned(),
                placed_by: PlacedBy::Cwd,
            },
            first_cwd: "/p/fg".to_owned(),
            label: "session 12:00".to_owned(),
        },
        found: Found::default(),
    }
}

fn call(id: &str, tool: &str, hash: &str) -> CallFacts {
    CallFacts {
        id: CallId::try_from(id).ok(),
        tool: tool.to_owned(),
        kind: kind_of(tool),
        target: "t".to_owned(),
        input_hash: hash.to_owned(),
    }
}

fn run(events: &[Event]) -> Record {
    let mut record: Option<Record> = None;
    for (second, event) in (0..).zip(events) {
        record = Some(apply(record.as_ref(), event, &change_at(later(second))));
    }
    record.unwrap()
}

fn permission_wait() -> Vec<Event> {
    vec![
        Event::TurnStart,
        Event::CallStart(call("c1", "run", "h")),
        Event::Wait(
            WaitReason::Permission,
            CallRef::Facts(CallFacts {
                id: None,
                ..call("", "run", "h")
            }),
        ),
    ]
}

fn with(mut events: Vec<Event>, more: &[Event]) -> Vec<Event> {
    events.extend_from_slice(more);
    events
}

fn subagent(agent: &str, title: &str) -> SubagentTitle {
    SubagentTitle {
        agent_id: ActorId::try_from(agent).unwrap(),
        title: title.to_owned(),
    }
}

#[rstest]
#[case::prompt(vec![Event::TurnStart], State::Working, None)]
#[case::permission(permission_wait(), State::Waiting, Some(WaitReason::Permission))]
#[case::pre_while_waiting(
    with(permission_wait(), &[Event::CallStart(call("c2", "read", "r"))]),
    State::Waiting,
    Some(WaitReason::Permission)
)]
#[case::other_call_ends(
    with(permission_wait(), &[Event::CallEnd(CallRef::Facts(call("c9", "read", "r")))]),
    State::Waiting,
    Some(WaitReason::Permission)
)]
#[case::bound_call_ends(
    with(permission_wait(), &[Event::CallEnd(CallRef::Facts(call("c1", "run", "h")))]),
    State::Working,
    None
)]
#[case::batch_lists_the_call(
    with(permission_wait(), &[Event::CallsEnd(vec![call("c1", "run", "h")])]),
    State::Working,
    None
)]
#[case::batch_of_another_call(
    with(permission_wait(), &[Event::CallsEnd(vec![call("c9", "read", "r")])]),
    State::Waiting,
    Some(WaitReason::Permission)
)]
#[case::request_before_pre(
    vec![
        Event::TurnStart,
        Event::Wait(WaitReason::Permission, CallRef::Facts(CallFacts { id: None, ..call("", "run", "h") })),
        Event::CallStart(call("c1", "run", "h")),
        Event::CallEnd(CallRef::Facts(call("c1", "run", "changed"))),
    ],
    State::Working,
    None
)]
#[case::unbound_wait_ends_by_batch_hash(
    vec![
        Event::TurnStart,
        Event::Wait(WaitReason::Permission, CallRef::Facts(CallFacts { id: None, ..call("", "run", "h") })),
        Event::CallsEnd(vec![CallFacts { id: None, ..call("", "run", "h") }]),
    ],
    State::Working,
    None
)]
#[case::question(vec![Event::CallStart(call("q", "ask", "h"))], State::Waiting, Some(WaitReason::Question))]
#[case::plan(vec![Event::CallStart(call("p", "plan", "h"))], State::Waiting, Some(WaitReason::Plan))]
#[case::notified(vec![Event::Notified(WaitReason::Input)], State::Waiting, Some(WaitReason::Input))]
#[case::notified_wait_ends_on_a_call_end(
    vec![Event::Notified(WaitReason::Permission), Event::CallEnd(CallRef::Facts(call("c", "read", "r")))],
    State::Working,
    None
)]
#[case::notified_wait_ends_on_a_batch(
    vec![Event::Notified(WaitReason::Permission), Event::CallsEnd(Vec::new())],
    State::Working,
    None
)]
#[case::notification_keeps_a_call_wait(
    with(permission_wait(), &[Event::Notified(WaitReason::Input)]),
    State::Waiting,
    Some(WaitReason::Permission)
)]
#[case::request_replaces_a_notified_wait(
    vec![
        Event::Notified(WaitReason::Input),
        Event::Wait(WaitReason::Permission, CallRef::Facts(CallFacts { id: None, ..call("", "run", "h") })),
    ],
    State::Waiting,
    Some(WaitReason::Permission)
)]
#[case::second_request_keeps_the_first(
    with(permission_wait(), &[Event::Wait(WaitReason::Permission, CallRef::Facts(CallFacts { id: None, ..call("", "change", "e") }))]),
    State::Waiting,
    Some(WaitReason::Permission)
)]
#[case::idle_prompt(with(permission_wait(), &[Event::TurnEnd {
        error: None,
        subagents_done: true,
    }]), State::Idle, None)]
#[case::stop(with(permission_wait(), &[Event::TurnEnd {
        error: None,
        subagents_done: false,
    }]), State::Idle, None)]
#[case::subagent_start(vec![Event::SubagentStart], State::Working, None)]
#[case::subagent_stop(vec![Event::SubagentStart, Event::SubagentEnd], State::Done, None)]
#[case::cwd_changed(vec![Event::TurnEnd {
        error: None,
        subagents_done: false,
    }, Event::CwdChange], State::Idle, None)]
#[case::session_end(vec![Event::SessionEnd], State::Ended, None)]
fn apply__should_follow_the_event_table(
    #[case] events: Vec<Event>,
    #[case] state: State,
    #[case] reason: Option<WaitReason>,
) {
    // When
    let record = run(&events);

    // Then
    assert_eq!(record.state, state);
    assert_eq!(record.waiting.map(|w| w.reason), reason);
}

#[rstest]
#[case::never_in_flight(vec![Event::CallEnd(CallRef::Facts(call("c1", "change", "w")))])]
#[case::after_its_pre(vec![Event::CallStart(call("c1", "change", "w")), Event::CallEnd(CallRef::Facts(call("c1", "change", "w")))])]
fn apply__should_name_the_ended_call(#[case] events: Vec<Event>) {
    // When
    let record = run(&events);

    // Then
    assert_eq!(
        record.ended,
        Some(Ended {
            id: call_id("c1"),
            tool: "change".to_owned(),
            kind: Some(CallKind::Edit),
        })
    );
    assert_eq!(record.done_ids, vec![call_id("c1")]);
}

#[test]
fn apply__should_name_no_ended_call_without_an_id() {
    // Given
    let started = run(&[Event::CallStart(call("c1", "change", "w"))]);

    // When
    let record = run(&[
        Event::CallStart(call("c1", "change", "w")),
        Event::CallEnd(CallRef::Facts(CallFacts {
            id: None,
            ..call("", "change", "w")
        })),
    ]);

    // Then
    assert_eq!(started.ended, None);
    assert_eq!(record.ended, None);
    assert_eq!(record.in_flight.len(), 1);
    assert!(record.done_ids.is_empty());
}

#[test]
fn apply__should_track_no_call_without_an_id() {
    // When
    let record = run(&[Event::CallStart(CallFacts {
        id: None,
        ..call("", "read", "r")
    })]);

    // Then
    assert!(record.in_flight.is_empty());
    assert_eq!(record.action.map(|a| a.tool), Some("read".to_owned()));
}

#[test]
fn apply__should_finish_a_subagent_whose_turn_failed() {
    // Given
    let mut start = change("2026-10-20T12:00:00Z");
    start.fresh.agent = Some(Agent {
        id: ActorId::try_from("a1").unwrap(),
        agent_type: Some("Explore".to_owned()),
    });
    let subagent = apply(None, &Event::SubagentStart, &start);

    // When
    let failed = apply(
        Some(&subagent),
        &Event::TurnEnd {
            error: Some("server_error".to_owned()),
            subagents_done: false,
        },
        &change("2026-10-20T12:00:05Z"),
    );

    // Then
    assert_eq!(
        (failed.state, failed.error.as_deref(), failed.since),
        (
            State::Done,
            Some("server_error"),
            at("2026-10-20T12:00:05Z")
        )
    );
}

#[test]
fn apply__should_keep_the_error_of_a_stop_failure() {
    // When
    let failed = run(&[Event::TurnEnd {
        error: Some("rate_limit".to_owned()),
        subagents_done: false,
    }]);
    let prompted = run(&[
        Event::TurnEnd {
            error: Some("rate_limit".to_owned()),
            subagents_done: false,
        },
        Event::TurnStart,
    ]);

    // Then
    assert_eq!(failed.error.as_deref(), Some("rate_limit"));
    assert_eq!(prompted.error, None);
}

#[test]
fn apply__should_keep_32_calls_and_64_ended_ids() {
    // Given
    let mut events: Vec<Event> = (0..40)
        .map(|n| Event::CallStart(call(&format!("p{n}"), "read", "r")))
        .collect();
    events.extend(
        (0..70).map(|n| Event::CallEnd(CallRef::Facts(call(&format!("q{n}"), "read", "r")))),
    );
    events.push(Event::CallStart(call("q69", "read", "r")));

    // When
    let record = run(&events);

    // Then
    assert_eq!(record.in_flight.len(), 32);
    assert_eq!(record.in_flight.first().map(|c| c.id.as_str()), Some("p8"));
    assert_eq!(record.done_ids.len(), 64);
    assert_eq!(record.done_ids.first().map(CallId::as_str), Some("q6"));
    assert!(!record.in_flight.iter().any(|c| c.id.as_str() == "q69"));
}

#[test]
fn apply__should_time_since_from_the_state_change() {
    // When
    let record = run(&[
        Event::TurnStart,
        Event::CallStart(call("c", "read", "r")),
        Event::TurnEnd {
            error: None,
            subagents_done: false,
        },
    ]);
    let working = run(&[Event::TurnStart, Event::CallStart(call("c", "read", "r"))]);

    // Then
    assert_eq!(record.since, at("2026-10-20T12:00:02Z"));
    assert_eq!(working.since, at("2026-10-20T12:00:00Z"));
    assert_eq!(working.updated, at("2026-10-20T12:00:01Z"));
    assert_eq!(
        working.action,
        Some(Action {
            tool: "read".to_owned(),
            target: "t".to_owned()
        })
    );
}

#[rstest]
#[case::other_worktree(Some("/p/wt1"), "/p/wt1", PlacedBy::Path)]
#[case::same_worktree(Some("/p/fg"), "/p/fg", PlacedBy::Cwd)]
#[case::no_path(None, "/p/fg", PlacedBy::Cwd)]
fn apply__should_move_only_to_another_worktree(
    #[case] moved_to: Option<&str>,
    #[case] worktree: &str,
    #[case] placed_by: PlacedBy,
) {
    // Given
    let first = apply(None, &Event::TurnStart, &change("2026-10-20T12:00:00Z"));
    let mut next = change("2026-10-20T12:00:01Z");
    next.found.moved_to = moved_to.map(str::to_owned);
    next.writer.process = None;
    next.found.label = Some("Fix".to_owned());

    // When
    let record = apply(Some(&first), &Event::CwdChange, &next);

    // Then
    assert_eq!(record.worktree, worktree);
    assert_eq!(record.placed_by, placed_by);
    assert_eq!(record.label, "Fix");
    assert_eq!(record.process, first.process);
}

#[rstest]
#[case::question_during_a_permission_wait(
    with(permission_wait(), &[Event::CallStart(call("q", "ask", "a"))]),
    State::Waiting,
    Some(WaitReason::Permission),
    Some("c1")
)]
#[case::question_during_a_permission_wait_then_its_post(
    with(
        permission_wait(),
        &[Event::CallStart(call("q", "ask", "a")), Event::CallEnd(CallRef::Facts(call("q", "ask", "a")))]
    ),
    State::Waiting,
    Some(WaitReason::Permission),
    Some("c1")
)]
#[case::late_question(
    vec![Event::CallEnd(CallRef::Facts(call("q", "ask", "a"))), Event::CallStart(call("q", "ask", "a"))],
    State::Working,
    None,
    None
)]
#[case::question_during_a_notification_wait(
    vec![Event::Notified(WaitReason::Permission), Event::CallStart(call("q", "ask", "a"))],
    State::Waiting,
    Some(WaitReason::Permission),
    None
)]
#[case::standing_call_ends_while_the_question_flies(
    with(permission_wait(), &[Event::CallStart(call("q", "ask", "a")), Event::CallEnd(CallRef::Facts(call("c1", "run", "h")))]),
    State::Waiting,
    Some(WaitReason::Question),
    Some("q")
)]
#[case::batch_ends_the_standing_call_while_a_plan_flies(
    with(permission_wait(), &[Event::CallStart(call("p", "plan", "a")), Event::CallsEnd(vec![call("c1", "run", "h")])]),
    State::Waiting,
    Some(WaitReason::Plan),
    Some("p")
)]
#[case::notification_wait_ends_while_the_question_flies(
    vec![
        Event::CallStart(call("r", "read", "r")),
        Event::Notified(WaitReason::Permission),
        Event::CallStart(call("q", "ask", "a")),
        Event::CallEnd(CallRef::Facts(call("r", "read", "r"))),
    ],
    State::Waiting,
    Some(WaitReason::Question),
    Some("q")
)]
#[case::plan_after_its_post(
    vec![Event::CallEnd(CallRef::Facts(call("p", "plan", "a"))), Event::CallStart(call("p", "plan", "a"))],
    State::Working,
    None,
    None
)]
fn apply__should_leave_a_standing_wait_and_ended_calls_alone(
    #[case] events: Vec<Event>,
    #[case] state: State,
    #[case] reason: Option<WaitReason>,
    #[case] call: Option<&str>,
) {
    // When
    let record = run(&events);

    // Then
    assert_eq!(record.state, state);
    assert_eq!(record.waiting.as_ref().map(|w| w.reason), reason);
    assert_eq!(
        record
            .waiting
            .and_then(|w| w.call)
            .as_ref()
            .map(CallId::as_str),
        call
    );
}

#[test]
fn finish_subagent__should_mark_done() {
    // Given
    let mut start = change("2026-10-20T12:00:00Z");
    start.fresh.agent = Some(Agent {
        id: ActorId::try_from("a1").unwrap(),
        agent_type: Some("Explore".to_owned()),
    });
    let subagent = apply(None, &Event::SubagentStart, &start);

    // When
    let done = finish_subagent(&subagent, &change("2026-10-20T12:00:05Z").writer).unwrap();

    // Then
    assert_eq!(done.state, State::Done);
    assert_eq!(done.since, at("2026-10-20T12:00:05Z"));
    assert_eq!(done.updated, at("2026-10-20T12:00:05Z"));
    assert_eq!(
        (
            done.agent_id.as_ref().map(ActorId::as_str),
            done.agent_type.as_deref()
        ),
        (Some("a1"), Some("Explore"))
    );
}

#[test]
fn finish_subagent__should_leave_a_done_subagent_unchanged() {
    // Given
    let mut start = change("2026-10-20T12:00:00Z");
    start.fresh.agent = Some(Agent {
        id: ActorId::try_from("a1").unwrap(),
        agent_type: Some("Explore".to_owned()),
    });
    let subagent = apply(None, &Event::SubagentStart, &start);
    let done = apply(
        Some(&subagent),
        &Event::SubagentEnd,
        &change("2026-10-20T12:00:05Z"),
    );

    // When
    let again = finish_subagent(&done, &change("2026-10-20T12:30:00Z").writer);

    // Then
    assert_eq!(again, None);
}
