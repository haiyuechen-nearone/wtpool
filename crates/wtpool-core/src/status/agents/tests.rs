use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rstest::rstest;

use super::*;
use crate::activity::CallKind;
use crate::activity::PlacedBy;
use crate::processes::{RecordedProcess, StartTime};
use crate::status::record_time::RecordTime;
use crate::status::records::{Call, SubagentTitle, ToolTarget, Waiting};

mod background;
mod builds;
mod sessions;

const START: i64 = 1_792_497_600;
const CLAUDE: u32 = 7001;

fn at(seconds: i64) -> RecordTime {
    RecordTime::parse(&Timestamp::from_seconds(START + seconds).to_rfc3339()).unwrap()
}

fn record(key: &str, state: RecordState) -> Record {
    Record {
        key: key.to_owned(),
        harness: "claude-code".to_owned(),
        label: "label".to_owned(),
        worktree: PathBuf::from("/p/wt1"),
        placed_by: PlacedBy::Path,
        state,
        waiting: None,
        action: None,
        in_flight: Vec::new(),
        ended: None,
        error: None,
        since: at(0),
        updated: at(0),
        writer_started: 0,
        process: Some(RecordedProcess {
            pid: CLAUDE,
            started: Some(Timestamp::from_seconds(START - 3600)),
        }),
        subagents: Vec::new(),
        tasks: Vec::new(),
    }
}

fn claude(started: Option<i64>) -> RunningProcess {
    RunningProcess {
        pid: CLAUDE,
        parent: Some(1),
        started: started
            .map(Timestamp::from_seconds)
            .and_then(StartTime::from_second),
        name: "claude".to_owned(),
        cwd: PathBuf::from("/p/fg"),
    }
}

fn call(kind: CallKind) -> Call {
    Call {
        id: format!("toolu_{kind:?}"),
        kind,
    }
}

/// The state `subject` shows `after` seconds, with `others` and a build in `/p/wt1` when
/// `build`.
fn state_after(subject: &Record, others: &[Record], after: i64, build: bool) -> Option<ActorState> {
    let processes = [claude(Some(START - 3600))];
    let records: Vec<&Record> = others.iter().chain([subject]).collect();
    let building = move |path: &Path| build && path == Path::new("/p/wt1");
    let context = Context {
        now: Timestamp::from_seconds(START + after),
        processes: &processes,
        records: &records,
        building: &building,
        superseded: &BTreeSet::new(),
        builds_below: &BTreeMap::new(),
    };
    actor(subject, &context).map(|actor| actor.state)
}

fn with_calls(mut record: Record, calls: &[CallKind]) -> Record {
    record.in_flight = calls.iter().map(|kind| call(*kind)).collect();
    record
}

#[rstest]
#[case::working_under_120_s(record("s1", RecordState::Working), vec![], 119, false, Some(ActorState::Working))]
#[case::working_at_120_s(record("s1", RecordState::Working), vec![], 120, false, Some(ActorState::Quiet))]
#[case::bash_during_a_build(with_calls(record("s1", RecordState::Working), &[CallKind::Shell]), vec![], 300, true, Some(ActorState::Working))]
#[case::bash_without_a_build(with_calls(record("s1", RecordState::Working), &[CallKind::Shell]), vec![], 300, false, Some(ActorState::Quiet))]
#[case::edit_during_a_build(with_calls(record("s1", RecordState::Working), &[CallKind::Edit]), vec![], 300, true, Some(ActorState::Quiet))]
#[case::agent_during_a_build(with_calls(record("s1", RecordState::Working), &[CallKind::Agent]), vec![], 300, true, Some(ActorState::Quiet))]
#[case::shell_with_its_subagent_working(with_calls(record("s1", RecordState::Working), &[CallKind::Shell]), vec![record("s1.a1", RecordState::Working)], 300, false, Some(ActorState::Quiet))]
#[case::read_during_a_build(with_calls(record("s1", RecordState::Working), &[CallKind::Other]), vec![], 300, true, Some(ActorState::Quiet))]
#[case::agent_while_its_subagent_works(with_calls(record("s1", RecordState::Working), &[CallKind::Agent]), vec![record("s1.a1", RecordState::Working)], 300, false, Some(ActorState::Working))]
#[case::agent_while_its_subagent_waits(with_calls(record("s1", RecordState::Working), &[CallKind::Agent]), vec![record("s1.a1", RecordState::Waiting)], 300, false, Some(ActorState::Working))]
#[case::agent_after_its_subagent_is_done(with_calls(record("s1", RecordState::Working), &[CallKind::Agent]), vec![record("s1.a1", RecordState::Done)], 300, false, Some(ActorState::Quiet))]
#[case::agent_with_another_sessions_subagent(with_calls(record("s1", RecordState::Working), &[CallKind::Agent]), vec![record("s10.a1", RecordState::Working)], 300, false, Some(ActorState::Quiet))]
#[case::subagent_without_an_agent_call(record("s1", RecordState::Working), vec![record("s1.a1", RecordState::Working)], 300, false, Some(ActorState::Quiet))]
#[case::waiting_never_decays(record("s1", RecordState::Waiting), vec![], 3600, false, Some(ActorState::Waiting))]
#[case::idle_stays_idle(record("s1", RecordState::Idle), vec![], 3600, false, Some(ActorState::Idle))]
#[case::done_under_a_minute(record("s1.a1", RecordState::Done), vec![], 59, false, Some(ActorState::Done))]
#[case::done_at_a_minute(record("s1.a1", RecordState::Done), vec![], 60, false, None)]
#[case::ended(record("s1", RecordState::Ended), vec![], 1, false, None)]
fn actor__should_follow_the_decay_table(
    #[case] subject: Record,
    #[case] others: Vec<Record>,
    #[case] after: i64,
    #[case] build: bool,
    #[case] expected: Option<ActorState>,
) {
    // When
    let state = state_after(&subject, &others, after, build);

    // Then
    assert_eq!(state, expected);
}

#[test]
fn actor__should_not_count_a_dead_subagent_as_working() {
    // Given
    let subject = with_calls(record("s1", RecordState::Working), &[CallKind::Agent]);
    let mut subagent = record("s1.a1", RecordState::Working);
    subagent.process = Some(RecordedProcess {
        pid: 9,
        started: None,
    });

    // When
    let state = state_after(&subject, &[subagent], 300, false);

    // Then
    assert_eq!(state, Some(ActorState::Quiet));
}

fn waiting(reason: Option<&str>, tool: Option<&str>) -> Waiting {
    Waiting {
        reason: reason.map(str::to_owned),
        tool: tool.map(str::to_owned),
        target: Some("git push".to_owned()),
    }
}

#[rstest]
#[case::permission(Some("permission"), Some(WaitReason::Permission))]
#[case::question(Some("question"), Some(WaitReason::Question))]
#[case::plan(Some("plan"), Some(WaitReason::Plan))]
#[case::plan_approval(Some("plan approval"), Some(WaitReason::Plan))]
#[case::input(Some("input"), Some(WaitReason::Input))]
#[case::other(Some("elsewhere"), None)]
#[case::none(None, None)]
fn actor__should_name_the_reason_of_a_wait(
    #[case] reason: Option<&str>,
    #[case] expected: Option<WaitReason>,
) {
    // Given
    let mut subject = record("s1", RecordState::Waiting);
    subject.waiting = Some(waiting(reason, Some("Bash")));
    let processes = [claude(None)];
    let context = Context {
        now: Timestamp::from_seconds(START),
        processes: &processes,
        records: &[],
        building: &|_: &Path| false,
        superseded: &BTreeSet::new(),
        builds_below: &BTreeMap::new(),
    };

    // When
    let shown = actor(&subject, &context).unwrap();

    // Then
    assert_eq!(shown.reason, expected);
}

fn tool_target(tool: &str, target: &str) -> ToolTarget {
    ToolTarget {
        tool: tool.to_owned(),
        target: target.to_owned(),
    }
}

#[rstest]
#[case::working_action(RecordState::Working, Some(tool_target("Read", "f.txt")), None, Some(("Read", "f.txt")))]
#[case::no_action(RecordState::Idle, None, None, None)]
#[case::empty_tool(RecordState::Idle, Some(tool_target("", "x")), None, None)]
#[case::waiting_call(RecordState::Waiting, Some(tool_target("Read", "f.txt")), Some(waiting(Some("permission"), Some("Bash"))), Some(("Bash", "git push")))]
#[case::waiting_without_tool(
    RecordState::Waiting,
    Some(tool_target("Read", "f.txt")),
    Some(waiting(Some("input"), None)),
    None
)]
#[case::working_after_a_wait(RecordState::Working, Some(tool_target("Read", "f.txt")), Some(waiting(Some("permission"), Some("Bash"))), Some(("Read", "f.txt")))]
fn actor__should_show_the_call_it_waits_for_else_its_action(
    #[case] state: RecordState,
    #[case] action: Option<ToolTarget>,
    #[case] wait: Option<Waiting>,
    #[case] expected: Option<(&str, &str)>,
) {
    // Given
    let mut subject = record("s1", state);
    subject.action = action;
    subject.waiting = wait;
    let processes = [claude(None)];
    let context = Context {
        now: Timestamp::from_seconds(START),
        processes: &processes,
        records: &[],
        building: &|_: &Path| false,
        superseded: &BTreeSet::new(),
        builds_below: &BTreeMap::new(),
    };

    // When
    let shown = actor(&subject, &context).unwrap();

    // Then
    let got = shown
        .action
        .as_ref()
        .map(|a| (a.tool.as_str(), a.target.as_str()));
    assert_eq!(got, expected);
    assert_eq!(shown.reason.is_some(), state == RecordState::Waiting);
}

#[test]
fn actor__should_copy_the_record() {
    // Given
    let mut subject = record("s1.a1", RecordState::Idle);
    subject.error = Some("rate_limit".to_owned());
    subject.since = at(-5);
    let processes = [claude(None)];
    let context = Context {
        now: Timestamp::from_seconds(START),
        processes: &processes,
        records: &[],
        building: &|_: &Path| false,
        superseded: &BTreeSet::new(),
        builds_below: &BTreeMap::new(),
    };

    // When
    let shown = actor(&subject, &context).unwrap();

    // Then
    assert_eq!(
        shown,
        Actor {
            key: "s1.a1".to_owned(),
            kind: ActorKind::Subagent,
            label: "label".to_owned(),
            state: ActorState::Idle,
            reason: None,
            action: None,
            placed_by: PlacedBy::Path,
            error: Some("rate_limit".to_owned()),
            since: at(-5),
            updated: at(0),
            harness: "claude-code".to_owned(),
            outputs: Vec::new(),
        }
    );
    let session = actor(&record("s1", RecordState::Idle), &context).unwrap();
    assert_eq!(session.kind, ActorKind::Session);
}

#[test]
fn actor__should_list_its_outputs_and_omit_none() {
    // Given
    let mut subject = record("s1", RecordState::Working);
    let task = |id: &str, ended: Option<&str>| {
        serde_json::json!({"id": id, "path": format!("/t/claude-501/p/s1/tasks/{id}.output"),
                           "summary": "cargo build", "started": "2026-10-20T12:00:00Z", "ended": ended})
    };
    subject.tasks = crate::status::outputs::tasks(
        serde_json::from_value(serde_json::json!([
            task("b1", Some("2026-10-20T12:01:00Z")),
            task("b2", None)
        ]))
        .unwrap(),
    );
    let processes = [claude(None)];
    let context = Context {
        now: Timestamp::from_seconds(START),
        processes: &processes,
        records: &[],
        building: &|_: &Path| false,
        superseded: &BTreeSet::new(),
        builds_below: &BTreeMap::new(),
    };

    // When
    let with = serde_json::to_value(actor(&subject, &context).unwrap()).unwrap();
    let without =
        serde_json::to_value(actor(&record("s2", RecordState::Idle), &context).unwrap()).unwrap();

    // Then
    assert_eq!(
        with["outputs"],
        serde_json::json!([
            {"task": "b2", "path": "/t/claude-501/p/s1/tasks/b2.output", "summary": "cargo build",
             "running": true, "started": "2026-10-20T12:00:00Z", "ended": null},
            {"task": "b1", "path": "/t/claude-501/p/s1/tasks/b1.output", "summary": "cargo build",
             "running": false, "started": "2026-10-20T12:00:00Z", "ended": "2026-10-20T12:01:00Z"},
        ])
    );
    assert!(without.get("outputs").is_none(), "{without}");
}

#[test]
fn next_change__should_give_the_first_future_quiet_or_leaving_time() {
    // Given
    let mut working = record("s1", RecordState::Working);
    working.updated = at(10);
    let mut done = record("s2.a1", RecordState::Done);
    done.updated = at(40);
    let stale = record("s3", RecordState::Working);
    let records = [
        &working,
        &done,
        &stale,
        &record("s4", RecordState::Waiting),
        &record("s5", RecordState::Idle),
        &record("s6", RecordState::Ended),
    ];

    // When
    let soon = next_change(&records, Timestamp::from_seconds(START + 99));
    let later = next_change(&records, Timestamp::from_seconds(START + 100));
    let none = next_change(&records, Timestamp::from_seconds(START + 130));

    // Then
    assert_eq!(soon, Some(Timestamp::from_seconds(START + 100)));
    assert_eq!(later, Some(Timestamp::from_seconds(START + 120)));
    assert_eq!(none, None);
}

#[test]
fn sort__should_put_the_newest_first_then_by_key() {
    // Given
    let processes = [claude(None)];
    let context = Context {
        now: Timestamp::from_seconds(START),
        processes: &processes,
        records: &[],
        building: &|_: &Path| false,
        superseded: &BTreeSet::new(),
        builds_below: &BTreeMap::new(),
    };
    let make = |key: &str, updated: i64| {
        let mut subject = record(key, RecordState::Idle);
        subject.updated = at(updated);
        actor(&subject, &context).unwrap()
    };
    let mut actors = vec![make("b", 1), make("a", 1), make("c", 5)];

    // When
    sort(&mut actors);

    // Then
    let keys: Vec<&str> = actors.iter().map(|a| a.key.as_str()).collect();
    assert_eq!(keys, vec!["c", "a", "b"]);
}

#[rstest]
#[case::session_title("s1", &["s1"], "label")]
#[case::subagent_with_a_title("s1.a1", &["s1", "s1.a1"], "Find the parser tests")]
#[case::title_in_a_subagent_record("s1.a2", &["s1.a1", "s1.a2"], "Read the docs")]
#[case::newest_title_wins("s1.a3", &["s1", "s1.a3"], "Newer")]
#[case::subagent_without_a_title("s1.a9", &["s1", "s1.a9"], "label")]
#[case::other_session("s2.a1", &["s1", "s2.a1"], "label")]
fn actor__should_label_a_subagent_with_its_title(
    #[case] key: &str,
    #[case] keys: &[&str],
    #[case] expected: &str,
) {
    // Given
    let titled = |agent: &str, title: &str| SubagentTitle {
        agent_id: agent.to_owned(),
        title: title.to_owned(),
    };
    let records: Vec<Record> = keys
        .iter()
        .map(|key| {
            let mut record = record(key, RecordState::Working);
            record.subagents = match *key {
                "s1" => vec![
                    titled("a1", "Find the parser tests"),
                    titled("a3", "Older"),
                    titled("a3", "Newer"),
                ],
                "s1.a1" => vec![titled("a2", "Read the docs")],
                _ => Vec::new(),
            };
            record
        })
        .collect();
    let refs: Vec<&Record> = records.iter().collect();
    let subject = records.iter().find(|record| record.key == key).unwrap();
    let processes = [claude(None)];
    let context = Context {
        now: Timestamp::from_seconds(START),
        processes: &processes,
        records: &refs,
        building: &|_: &Path| false,
        superseded: &BTreeSet::new(),
        builds_below: &BTreeMap::new(),
    };

    // When
    let shown = actor(subject, &context).unwrap();

    // Then
    assert_eq!(shown.label, expected);
}
