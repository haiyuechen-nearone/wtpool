use rstest::rstest;

use super::*;
use crate::activity::CallKind;

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
    Change {
        harness: Harness::ClaudeCode,
        waiting_tools: WAITING,
        now: now.to_owned(),
        started_micros: 10,
        process: Some(Process {
            pid: 7001,
            started: "2026-10-20T11:00:00Z".to_owned(),
        }),
        first_place: ("/p/fg".to_owned(), PlacedBy::Cwd),
        first_cwd: "/p/fg".to_owned(),
        moved_to: None,
        new_label: "session 12:00".to_owned(),
        label: None,
        titles: Titles::default(),
        transcript_offset: None,
        subagent: None,
        session_id: "s1".to_owned(),
        agent: None,
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
    for (second, event) in events.iter().enumerate() {
        let now = format!("2026-10-20T12:00:{second:02}Z");
        record = Some(apply(record.as_ref(), event, &change(&now)));
    }
    record.unwrap()
}

fn permission_wait() -> Vec<Event> {
    vec![
        Event::TurnStart,
        Event::CallStart(call("c1", "run", "h")),
        Event::PermissionAsked(CallFacts {
            id: None,
            ..call("", "run", "h")
        }),
    ]
}

fn with(mut events: Vec<Event>, more: &[Event]) -> Vec<Event> {
    events.extend_from_slice(more);
    events
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
    with(permission_wait(), &[Event::CallEnd(call("c9", "read", "r"))]),
    State::Waiting,
    Some(WaitReason::Permission)
)]
#[case::bound_call_ends(
    with(permission_wait(), &[Event::CallEnd(call("c1", "run", "h"))]),
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
        Event::PermissionAsked(CallFacts { id: None, ..call("", "run", "h") }),
        Event::CallStart(call("c1", "run", "h")),
        Event::CallEnd(call("c1", "run", "changed")),
    ],
    State::Working,
    None
)]
#[case::unbound_wait_ends_by_batch_hash(
    vec![
        Event::TurnStart,
        Event::PermissionAsked(CallFacts { id: None, ..call("", "run", "h") }),
        Event::CallsEnd(vec![CallFacts { id: None, ..call("", "run", "h") }]),
    ],
    State::Working,
    None
)]
#[case::question(vec![Event::CallStart(call("q", "ask", "h"))], State::Waiting, Some(WaitReason::Question))]
#[case::plan(vec![Event::CallStart(call("p", "plan", "h"))], State::Waiting, Some(WaitReason::Plan))]
#[case::notified(vec![Event::Notified(WaitReason::Input)], State::Waiting, Some(WaitReason::Input))]
#[case::notified_wait_ends_on_a_call_end(
    vec![Event::Notified(WaitReason::Permission), Event::CallEnd(call("c", "read", "r"))],
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
        Event::PermissionAsked(CallFacts { id: None, ..call("", "run", "h") }),
    ],
    State::Waiting,
    Some(WaitReason::Permission)
)]
#[case::second_request_keeps_the_first(
    with(permission_wait(), &[Event::PermissionAsked(CallFacts { id: None, ..call("", "change", "e") })]),
    State::Waiting,
    Some(WaitReason::Permission)
)]
#[case::idle_prompt(with(permission_wait(), &[Event::Idle]), State::Idle, None)]
#[case::stop(with(permission_wait(), &[Event::TurnEnd(None)]), State::Idle, None)]
#[case::subagent_start(vec![Event::SubagentStart], State::Working, None)]
#[case::subagent_stop(vec![Event::SubagentStart, Event::SubagentEnd], State::Done, None)]
#[case::cwd_changed(vec![Event::TurnEnd(None), Event::CwdChange], State::Idle, None)]
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
#[case::never_in_flight(vec![Event::CallEnd(call("c1", "change", "w"))])]
#[case::after_its_pre(vec![Event::CallStart(call("c1", "change", "w")), Event::CallEnd(call("c1", "change", "w"))])]
fn apply__should_name_the_ended_call(#[case] events: Vec<Event>) {
    // When
    let record = run(&events);

    // Then
    assert_eq!(
        record.ended,
        Some(Ended {
            id: "c1".to_owned(),
            tool: "change".to_owned(),
            kind: Some(CallKind::Edit),
        })
    );
    assert_eq!(record.done_ids, vec!["c1".to_owned()]);
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
fn apply__should_keep_the_error_of_a_stop_failure() {
    // When
    let failed = run(&[Event::TurnEnd(Some("rate_limit".to_owned()))]);
    let prompted = run(&[
        Event::TurnEnd(Some("rate_limit".to_owned())),
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
    events.extend((0..70).map(|n| Event::CallEnd(call(&format!("q{n}"), "read", "r"))));
    events.push(Event::CallStart(call("q69", "read", "r")));

    // When
    let record = run(&events);

    // Then
    assert_eq!(record.in_flight.len(), 32);
    assert_eq!(record.in_flight.first().map(|c| c.id.as_str()), Some("p8"));
    assert_eq!(record.done_ids.len(), 64);
    assert_eq!(record.done_ids.first().map(String::as_str), Some("q6"));
    assert!(!record.in_flight.iter().any(|c| c.id == "q69"));
}

#[test]
fn apply__should_time_since_from_the_state_change() {
    // When
    let record = run(&[
        Event::TurnStart,
        Event::CallStart(call("c", "read", "r")),
        Event::TurnEnd(None),
    ]);
    let working = run(&[Event::TurnStart, Event::CallStart(call("c", "read", "r"))]);

    // Then
    assert_eq!(record.since, "2026-10-20T12:00:02Z");
    assert_eq!(working.since, "2026-10-20T12:00:00Z");
    assert_eq!(working.updated, "2026-10-20T12:00:01Z");
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
    next.moved_to = moved_to.map(str::to_owned);
    next.process = None;
    next.label = Some("Fix".to_owned());

    // When
    let record = apply(Some(&first), &Event::CwdChange, &next);

    // Then
    assert_eq!(record.worktree, worktree);
    assert_eq!(record.placed_by, placed_by);
    assert_eq!(record.label, "Fix");
    assert_eq!(record.process, first.process);
}

#[test]
fn merge_older__should_keep_the_state_and_take_the_calls() {
    // Given
    let newer = run(&[Event::CallStart(call("q", "ask", "h"))]);
    let older_result = apply(
        Some(&newer),
        &Event::TurnEnd(None),
        &change("2026-10-20T12:00:09Z"),
    );

    // When
    let merged = merge_older(&newer, older_result, WAITING);

    // Then
    assert_eq!(merged.state, State::Waiting);
    assert!(merged.in_flight.is_empty());
    assert_eq!(merged.updated, "2026-10-20T12:00:09Z");
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
        &[Event::CallStart(call("q", "ask", "a")), Event::CallEnd(call("q", "ask", "a"))]
    ),
    State::Waiting,
    Some(WaitReason::Permission),
    Some("c1")
)]
#[case::late_question(
    vec![Event::CallEnd(call("q", "ask", "a")), Event::CallStart(call("q", "ask", "a"))],
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
    with(permission_wait(), &[Event::CallStart(call("q", "ask", "a")), Event::CallEnd(call("c1", "run", "h"))]),
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
        Event::CallEnd(call("r", "read", "r")),
    ],
    State::Waiting,
    Some(WaitReason::Question),
    Some("q")
)]
#[case::plan_after_its_post(
    vec![Event::CallEnd(call("p", "plan", "a")), Event::CallStart(call("p", "plan", "a"))],
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
    assert_eq!(record.waiting.and_then(|w| w.call).as_deref(), call);
}

#[rstest]
#[case::wait_on_the_ended_call(call("c1", "run", "h"), false, State::Working, None)]
#[case::wait_on_another_call(
    call("c2", "read", "r"),
    false,
    State::Waiting,
    Some(WaitReason::Permission)
)]
#[case::question_still_in_flight(
    call("c1", "run", "h"),
    true,
    State::Waiting,
    Some(WaitReason::Question)
)]
fn merge_older__should_end_a_wait_whose_call_the_older_writer_ended(
    #[case] ended: CallFacts,
    #[case] question: bool,
    #[case] state: State,
    #[case] reason: Option<WaitReason>,
) {
    // Given
    let before = run(&[
        Event::CallStart(call("c1", "run", "h")),
        Event::CallStart(call("c2", "read", "r")),
    ]);
    let mut newer = apply(
        Some(&before),
        &Event::PermissionAsked(CallFacts {
            id: None,
            ..call("", "run", "h")
        }),
        &change("2026-10-20T12:00:05Z"),
    );
    if question {
        let pre = Event::CallStart(call("q", "ask", "a"));
        newer = apply(Some(&newer), &pre, &change("2026-10-20T12:00:05Z"));
    }
    let older = apply(
        Some(&newer),
        &Event::CallEnd(ended.clone()),
        &change("2026-10-20T12:00:06Z"),
    );

    // When
    let merged = merge_older(&newer, older, WAITING);

    // Then
    assert_eq!(merged.state, state);
    assert_eq!(merged.waiting.map(|w| w.reason), reason);
    assert!(merged.done_ids.contains(&ended.id.unwrap().to_string()));
    let since = if state == State::Waiting && !question {
        "2026-10-20T12:00:05Z"
    } else {
        "2026-10-20T12:00:06Z"
    };
    assert_eq!(merged.since, since);
}

#[test]
fn finish_subagent__should_mark_done() {
    // Given
    let mut start = change("2026-10-20T12:00:00Z");
    start.agent = Some(("a1".to_owned(), Some("Explore".to_owned())));
    let subagent = apply(None, &Event::SubagentStart, &start);

    // When
    let done = finish_subagent(&subagent, &change("2026-10-20T12:00:05Z"));

    // Then
    assert_eq!(done.state, State::Done);
    assert_eq!(done.since, "2026-10-20T12:00:05Z");
    assert_eq!(done.agent_id.as_deref(), Some("a1"));
}

fn titles(user: Option<&str>, generated: Option<&str>) -> Titles {
    Titles {
        user: user.map(str::to_owned),
        generated: generated.map(str::to_owned),
    }
}

#[rstest]
#[case::none(&[], "session 12:00", None)]
#[case::generated(&[titles(None, Some("Bind"))], "Bind", Some(TitleBy::Harness))]
#[case::generated_again(&[titles(None, Some("Bind")), titles(None, Some("Bound"))], "Bound", Some(TitleBy::Harness))]
#[case::given(&[titles(None, Some("Bind")), titles(Some("Mine"), None)], "Mine", Some(TitleBy::User))]
#[case::given_over_generated_at_once(&[titles(Some("Mine"), Some("Bind"))], "Mine", Some(TitleBy::User))]
#[case::generated_after_given(&[titles(Some("Mine"), None), titles(None, Some("Bind"))], "Mine", Some(TitleBy::User))]
#[case::given_again(&[titles(Some("Mine"), None), titles(Some("Ours"), None)], "Ours", Some(TitleBy::User))]
#[case::nothing_found_keeps(&[titles(None, Some("Bind")), titles(None, None)], "Bind", Some(TitleBy::Harness))]
fn apply__should_label_a_session_with_its_title(
    #[case] found: &[Titles],
    #[case] label: &str,
    #[case] by: Option<TitleBy>,
) {
    // Given
    let mut record = apply(None, &Event::TurnStart, &change("2026-10-20T12:00:00Z"));

    // When
    for titles in found {
        let mut next = change("2026-10-20T12:00:01Z");
        next.titles = titles.clone();
        record = apply(Some(&record), &Event::TurnEnd(None), &next);
    }

    // Then
    assert_eq!(record.label, label);
    assert_eq!(record.title.map(|title| title.by), by);
}

#[test]
fn apply__should_keep_the_transcript_offset_until_a_read_moves_it() {
    // Given
    let mut read = change("2026-10-20T12:00:00Z");
    read.transcript_offset = Some(4096);
    let first = apply(None, &Event::TurnStart, &read);

    // When
    let unread = apply(
        Some(&first),
        &Event::TurnEnd(None),
        &change("2026-10-20T12:00:01Z"),
    );

    // Then
    assert_eq!(
        (first.transcript_offset, unread.transcript_offset),
        (4096, 4096)
    );
}

fn subagent(agent: &str, title: &str) -> SubagentTitle {
    SubagentTitle {
        agent_id: agent.to_owned(),
        title: title.to_owned(),
    }
}

#[test]
fn apply__should_keep_the_newest_16_subagent_titles() {
    // Given
    let mut record = apply(None, &Event::TurnStart, &change("2026-10-20T12:00:00Z"));

    // When
    for n in 0..20 {
        let mut next = change("2026-10-20T12:00:01Z");
        next.subagent = Some(subagent(&format!("a{n}"), &format!("Task {n}")));
        record = apply(
            Some(&record),
            &Event::CallEnd(call("c", "spawn", "h")),
            &next,
        );
    }
    let mut again = change("2026-10-20T12:00:02Z");
    again.subagent = Some(subagent("a10", "Task ten"));
    record = apply(
        Some(&record),
        &Event::CallEnd(call("c", "spawn", "h")),
        &again,
    );

    // Then
    let ids: Vec<&str> = record
        .subagents
        .iter()
        .map(|known| known.agent_id.as_str())
        .collect();
    assert_eq!(ids.len(), MAX_SUBAGENTS);
    assert_eq!(ids.first(), Some(&"a4"));
    assert_eq!(record.subagents.last(), Some(&subagent("a10", "Task ten")));
}

#[test]
fn merge_older__should_keep_the_subagent_titles_of_both_writers() {
    // Given
    let mut learnt = change("2026-10-20T12:00:00Z");
    learnt.subagent = Some(subagent("a1", "Find"));
    let base = apply(None, &Event::TurnStart, &learnt);
    let mut newer_change = change("2026-10-20T12:00:02Z");
    newer_change.subagent = Some(subagent("a2", "read"));
    let newer = apply(Some(&base), &Event::TurnEnd(None), &newer_change);
    let mut older_change = change("2026-10-20T12:00:01Z");
    older_change.subagent = Some(subagent("a3", "Test"));
    let older = apply(
        Some(&base),
        &Event::CallEnd(call("c", "spawn", "h")),
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
