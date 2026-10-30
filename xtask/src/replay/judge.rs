//! The wtpool asserts of each recorded session, judged over the states the replay measured.

use serde_json::Value;

use super::{ReplayState, event};

/// The asserts of one session over its states. `late` is the status 125 s after its last event
/// before SessionEnd.
type Judge = fn(&[ReplayState], &Value) -> Result<(), String>;

/// Session name to judge. The names are the sessions of `vscode/test/cc/sessions.mjs`.
const JUDGES: &[(&str, Judge)] = &[
    ("cc_read_edit_bash", read_edit_bash),
    ("cc_subagent_elsewhere", subagent_elsewhere),
    ("cc_subagent_permission", subagent_permission),
    ("cc_cd_into_slot", cd_into_slot),
    ("cc_permission", permission),
    ("cc_permission_deny", permission_deny),
    ("cc_permission_deny_interrupt", permission_deny_interrupt),
    ("cc_interrupt", interrupt),
    ("cc_idle_prompt", idle_prompt),
    ("cc_stop_failure", stop_failure),
    ("cc_session_end", session_end),
];

fn is_subagent(payload: &Value) -> bool {
    payload.get("agent_id").is_some_and(|a| !a.is_null())
}

pub(super) fn actors(status: &Value) -> Vec<(String, Value)> {
    status
        .get("pools")
        .and_then(|pools| pools.get(0))
        .and_then(|pool| pool.get("worktrees"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|w| {
            let name = w["name"].as_str().unwrap_or_default().to_owned();
            w["actors"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(move |a| (name.clone(), a))
        })
        .collect()
}

fn session_actor(status: &Value) -> Option<(String, Value)> {
    actors(status)
        .into_iter()
        .find(|(_, a)| a["kind"] == "session")
}

fn state_of(state: &ReplayState) -> Option<String> {
    session_actor(&state.status).and_then(|(_, a)| a.get("state")?.as_str().map(str::to_owned))
}

/// The last state before SessionEnd whose event is `name`, of the main session.
fn last_main<'a>(states: &'a [ReplayState], name: &str) -> Option<&'a ReplayState> {
    states
        .iter()
        .take_while(|s| event(&s.payload) != "SessionEnd")
        .filter(|s| event(&s.payload) == name && !is_subagent(&s.payload))
        .last()
}

fn before_end(states: &[ReplayState]) -> Option<&ReplayState> {
    states
        .iter()
        .take_while(|s| event(&s.payload) != "SessionEnd")
        .last()
}

pub(super) fn expect(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

/// The wtpool asserts of `session` over its states.
pub(super) fn judge(session: &str, states: &[ReplayState], late: &Value) -> Result<(), String> {
    match JUDGES.iter().find(|(name, _)| *name == session) {
        Some((_, judge)) => judge(states, late),
        None => Err(format!("no wtpool asserts for {session}")),
    }
}

fn idle_after_stop(states: &[ReplayState], worktree: &str) -> Result<(), String> {
    let at = last_main(states, "Stop").ok_or("no Stop recorded")?;
    let actor = session_actor(&at.status);
    expect(
        actor
            .as_ref()
            .is_some_and(|(w, a)| w == worktree && a["state"] == "idle"),
        format!("after Stop the session is {actor:?}, not idle on {worktree}"),
    )
}

fn read_edit_bash(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    idle_after_stop(states, "wt1")
}

fn cd_into_slot(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    idle_after_stop(states, "wt2")
}

fn subagent_elsewhere(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    expect(
        states.iter().any(|s| {
            actors(&s.status)
                .iter()
                .any(|(w, a)| w == "wt2" && a["kind"] == "subagent")
        }),
        "the subagent never showed on wt2",
    )
}

fn subagent_permission(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let at = states
        .iter()
        .find(|s| event(&s.payload) == "PermissionRequest" && is_subagent(&s.payload))
        .ok_or("no PermissionRequest of the subagent recorded")?;
    let waiting: Vec<String> = actors(&at.status)
        .into_iter()
        .filter(|(_, a)| a["state"] == "waiting")
        .map(|(_, a)| {
            a.get("kind")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    expect(
        waiting == ["subagent"],
        format!("waiting during the request: {waiting:?}"),
    )
}

fn permission(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let from = states
        .iter()
        .position(|s| event(&s.payload) == "PermissionRequest")
        .ok_or("no PermissionRequest recorded")?;
    let held: Vec<Option<String>> = states
        .iter()
        .skip(from)
        .take_while(|s| {
            !(event(&s.payload) == "PostToolUse"
                && s.payload.get("tool_name").and_then(Value::as_str) == Some("Bash"))
        })
        .map(state_of)
        .collect();
    expect(
        held.iter().all(|s| s.as_deref() == Some("waiting")),
        format!("the session during the hold: {held:?}"),
    )
}

fn permission_deny(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let at = last_main(states, "PostToolBatch").ok_or("no PostToolBatch recorded")?;
    let state = state_of(at);
    expect(
        state.is_some() && state.as_deref() != Some("waiting"),
        format!("after the PostToolBatch the session is {state:?}"),
    )
}

fn permission_deny_interrupt(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let idle_prompt = states
        .iter()
        .any(|s| s.payload.get("notification_type").and_then(Value::as_str) == Some("idle_prompt"));
    let state = before_end(states).and_then(state_of);
    let wanted = if idle_prompt { "idle" } else { "waiting" };
    expect(
        state.as_deref() == Some(wanted),
        format!("at the end the session is {state:?}, not {wanted}"),
    )
}

fn interrupt(_states: &[ReplayState], late: &Value) -> Result<(), String> {
    let state = session_actor(late).and_then(|(_, a)| a.get("state")?.as_str().map(str::to_owned));
    expect(
        state.as_deref() == Some("quiet"),
        format!("125 s after the interrupt the session is {state:?}"),
    )
}

fn idle_prompt(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let waited = states
        .iter()
        .any(|s| state_of(s).as_deref() == Some("waiting"));
    let at = last_main(states, "Stop").ok_or("no Stop recorded")?;
    expect(
        !waited && state_of(at).as_deref() == Some("idle"),
        format!("waited: {waited}; after Stop: {:?}", state_of(at)),
    )
}

fn stop_failure(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let at = last_main(states, "StopFailure").ok_or("no StopFailure recorded")?;
    let actor = session_actor(&at.status);
    expect(
        actor
            .as_ref()
            .is_some_and(|(_, a)| a["state"] == "idle" && !a["error"].is_null()),
        format!("after StopFailure the session is {actor:?}"),
    )
}

fn session_end(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let at = states
        .iter()
        .rev()
        .find(|s| event(&s.payload) == "SessionEnd")
        .ok_or("no SessionEnd recorded")?;
    let ended = at
        .records
        .iter()
        .any(|r| r["agent_id"].is_null() && r["state"] == "ended");
    let subagents = at
        .records
        .iter()
        .filter(|r| !r["agent_id"].is_null())
        .count();
    expect(
        ended && subagents == 0,
        format!("after SessionEnd: ended {ended}, {subagents} subagent records"),
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use rstest::rstest;
    use serde_json::json;

    /// The status document with the given `(worktree, actor)` pairs.
    fn status(actors: &[(&str, Value)]) -> Value {
        let worktrees: Vec<Value> = actors
            .iter()
            .map(|(name, actor)| json!({"name": name, "actors": [actor]}))
            .collect();
        json!({"pools": [{"worktrees": worktrees}]})
    }

    fn session(state: &str) -> Value {
        json!({"kind": "session", "state": state})
    }

    fn subagent(state: &str) -> Value {
        json!({"kind": "subagent", "state": state})
    }

    fn at(payload: Value, status: Value) -> ReplayState {
        ReplayState {
            payload,
            status,
            records: Vec::new(),
        }
    }

    fn named(name: &str, status: Value) -> ReplayState {
        at(json!({"hook_event_name": name}), status)
    }

    fn sub_named(name: &str, status: Value) -> ReplayState {
        at(json!({"hook_event_name": name, "agent_id": "a1"}), status)
    }

    fn ended(records: Vec<Value>) -> ReplayState {
        ReplayState {
            payload: json!({"hook_event_name": "SessionEnd"}),
            status: Value::Null,
            records,
        }
    }

    fn judged(session: &str, states: &[ReplayState], late: &Value) -> bool {
        judge(session, states, late).is_ok()
    }

    #[rstest]
    #[case::pass("wt1", "idle", true)]
    #[case::other_slot("wt2", "idle", false)]
    #[case::working("wt1", "working", false)]
    fn read_edit_bash__should_want_idle_on_wt1_after_stop(
        #[case] worktree: &str,
        #[case] state: &str,
        #[case] passes: bool,
    ) {
        // Given
        let states = [named("Stop", status(&[(worktree, session(state))]))];

        // When
        let verdict = judged("cc_read_edit_bash", &states, &Value::Null);

        // Then
        assert_eq!(verdict, passes);
    }

    #[rstest]
    #[case::pass("wt2", true)]
    #[case::other_slot("wt1", false)]
    fn cd_into_slot__should_want_idle_on_wt2_after_stop(
        #[case] worktree: &str,
        #[case] passes: bool,
    ) {
        // Given
        let states = [named("Stop", status(&[(worktree, session("idle"))]))];

        // When
        let verdict = judged("cc_cd_into_slot", &states, &Value::Null);

        // Then
        assert_eq!(verdict, passes);
    }

    #[test]
    fn read_edit_bash__should_fail_without_a_stop() {
        // When
        let verdict = judge("cc_read_edit_bash", &[], &Value::Null);

        // Then
        assert_eq!(verdict, Err("no Stop recorded".to_owned()));
    }

    #[rstest]
    #[case::pass("wt2", true)]
    #[case::elsewhere("wt1", false)]
    fn subagent_elsewhere__should_want_the_subagent_on_wt2(
        #[case] worktree: &str,
        #[case] passes: bool,
    ) {
        // Given
        let states = [named(
            "SubagentStart",
            status(&[("wt1", session("working")), (worktree, subagent("working"))]),
        )];

        // When
        let verdict = judged("cc_subagent_elsewhere", &states, &Value::Null);

        // Then
        assert_eq!(verdict, passes);
    }

    #[rstest]
    #[case::pass("waiting", "working", true)]
    #[case::session_waits_too("waiting", "waiting", false)]
    #[case::subagent_not_waiting("working", "working", false)]
    fn subagent_permission__should_want_only_the_subagent_waiting(
        #[case] subagent_state: &str,
        #[case] session_state: &str,
        #[case] passes: bool,
    ) {
        // Given
        let states = [sub_named(
            "PermissionRequest",
            status(&[
                ("wt1", session(session_state)),
                ("wt2", subagent(subagent_state)),
            ]),
        )];

        // When
        let verdict = judged("cc_subagent_permission", &states, &Value::Null);

        // Then
        assert_eq!(verdict, passes);
    }

    #[rstest]
    #[case::held("waiting", true)]
    #[case::released("working", false)]
    fn permission__should_want_waiting_until_the_bash_result(
        #[case] during: &str,
        #[case] passes: bool,
    ) {
        // Given
        let states = [
            named("PermissionRequest", status(&[("wt1", session("waiting"))])),
            named("Notification", status(&[("wt1", session(during))])),
            at(
                json!({"hook_event_name": "PostToolUse", "tool_name": "Bash"}),
                status(&[("wt1", session("working"))]),
            ),
        ];

        // When
        let verdict = judged("cc_permission", &states, &Value::Null);

        // Then
        assert_eq!(verdict, passes);
    }

    #[rstest]
    #[case::working("working", true)]
    #[case::waiting("waiting", false)]
    fn permission_deny__should_want_a_session_not_waiting_after_the_batch(
        #[case] state: &str,
        #[case] passes: bool,
    ) {
        // Given
        let states = [named("PostToolBatch", status(&[("wt1", session(state))]))];

        // When
        let verdict = judged("cc_permission_deny", &states, &Value::Null);

        // Then
        assert_eq!(verdict, passes);
    }

    #[rstest]
    #[case::idle_prompt_then_idle(true, "idle", true)]
    #[case::idle_prompt_but_waiting(true, "waiting", false)]
    #[case::no_prompt_waiting(false, "waiting", true)]
    #[case::no_prompt_but_idle(false, "idle", false)]
    fn permission_deny_interrupt__should_want_idle_only_after_an_idle_prompt(
        #[case] prompted: bool,
        #[case] state: &str,
        #[case] passes: bool,
    ) {
        // Given
        let mut states = vec![named("Stop", status(&[("wt1", session(state))]))];
        if prompted {
            states.insert(
                0,
                at(
                    json!({"hook_event_name": "Notification", "notification_type": "idle_prompt"}),
                    status(&[("wt1", session("idle"))]),
                ),
            );
        }
        states.push(ended(Vec::new()));

        // When
        let verdict = judged("cc_permission_deny_interrupt", &states, &Value::Null);

        // Then
        assert_eq!(verdict, passes);
    }

    #[rstest]
    #[case::quiet("quiet", true)]
    #[case::still_idle("idle", false)]
    fn interrupt__should_want_a_quiet_session_in_the_late_status(
        #[case] state: &str,
        #[case] passes: bool,
    ) {
        // Given
        let late = status(&[("wt1", session(state))]);

        // When
        let verdict = judged("cc_interrupt", &[], &late);

        // Then
        assert_eq!(verdict, passes);
    }

    #[rstest]
    #[case::pass("working", "idle", true)]
    #[case::waited("waiting", "idle", false)]
    #[case::not_idle("working", "working", false)]
    fn idle_prompt__should_want_no_wait_and_idle_after_stop(
        #[case] before: &str,
        #[case] after: &str,
        #[case] passes: bool,
    ) {
        // Given
        let states = [
            named("UserPromptSubmit", status(&[("wt1", session(before))])),
            named("Stop", status(&[("wt1", session(after))])),
        ];

        // When
        let verdict = judged("cc_idle_prompt", &states, &Value::Null);

        // Then
        assert_eq!(verdict, passes);
    }

    #[rstest]
    #[case::pass("idle", json!("rate_limit"), true)]
    #[case::no_error("idle", Value::Null, false)]
    #[case::not_idle("working", json!("rate_limit"), false)]
    fn stop_failure__should_want_idle_with_an_error(
        #[case] state: &str,
        #[case] error: Value,
        #[case] passes: bool,
    ) {
        // Given
        let actor = json!({"kind": "session", "state": state, "error": error});
        let states = [named("StopFailure", status(&[("wt1", actor)]))];

        // When
        let verdict = judged("cc_stop_failure", &states, &Value::Null);

        // Then
        assert_eq!(verdict, passes);
    }

    #[test]
    fn stop_failure__should_fail_when_none_was_recorded() {
        // Given
        let states = [named("Stop", status(&[("wt1", session("idle"))]))];

        // When
        let verdict = judge("cc_stop_failure", &states, &Value::Null);

        // Then
        assert_eq!(verdict, Err("no StopFailure recorded".to_owned()));
    }

    #[rstest]
    #[case::pass(json!({"state": "ended", "agent_id": null}), 0, true)]
    #[case::not_ended(json!({"state": "working", "agent_id": null}), 0, false)]
    #[case::subagent_left(json!({"state": "ended", "agent_id": null}), 1, false)]
    fn session_end__should_want_an_ended_record_and_no_subagent_records(
        #[case] main: Value,
        #[case] subagents: usize,
        #[case] passes: bool,
    ) {
        // Given
        let mut records = vec![main];
        records.extend((0..subagents).map(|_| json!({"state": "working", "agent_id": "a1"})));
        let states = [ended(records)];

        // When
        let verdict = judged("cc_session_end", &states, &Value::Null);

        // Then
        assert_eq!(verdict, passes);
    }

    #[test]
    fn judge__should_refuse_a_session_it_has_no_asserts_for() {
        // When
        let verdict = judge("cc_unknown", &[], &Value::Null);

        // Then
        assert_eq!(verdict, Err("no wtpool asserts for cc_unknown".to_owned()));
    }

    #[test]
    fn judges__should_name_the_sessions_of_the_contract_run() {
        // Given
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vscode/test/cc/sessions.mjs");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let table = text.split("export const SESSIONS = [").nth(1).unwrap_or("");
        let table = table.split("];").next().unwrap_or("");

        // When
        let mut recorded: Vec<&str> = table
            .split("name: '")
            .skip(1)
            .filter_map(|rest| rest.split('\'').next())
            .collect();
        let mut judged: Vec<&str> = JUDGES.iter().map(|(name, _)| *name).collect();
        recorded.sort_unstable();
        judged.sort_unstable();

        // Then
        assert!(!recorded.is_empty(), "no sessions read from {path:?}");
        assert_eq!(judged, recorded);
    }
}
