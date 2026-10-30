//! The wtpool asserts of each forwarded OpenCode session, judged over the states the replay
//! measured: the records the hook leaves and the documents of `wtpool status --json`.

use serde_json::Value;

use super::judge::{actors, expect};
use super::opencode::{SERVER, name, session};
use super::{ReplayState, field};

/// The asserts of one session over its states. `late` is the status 125 s after its last event.
type Judge = fn(&[ReplayState], &Value) -> Result<(), String>;

/// Session name to judge: one per stream of `verify/fixtures/opencode-<version>/forwarded/`.
const JUDGES: &[(&str, Judge)] = &[
    ("provider_error_401", provider_error_401),
    ("serve_shutdown", serve_shutdown),
    ("run_read_edit_shell", run_read_edit_shell),
    ("subagent", subagent),
    ("permission_approve_reject", permission_approve_reject),
    ("permission_run_autoreject", permission_run_autoreject),
    ("question", question),
    ("interrupt", interrupt),
    ("title_given", title_given),
    ("long_answer", long_answer),
    ("deny", deny),
    ("delete_api", delete_api),
    ("two_folders", two_folders),
];

/// The readable form of an OpenCode id: its first `_` becomes `-`.
fn readable(id: &str) -> String {
    id.replacen('_', "-", 1)
}

/// The actor key of the payload's session: its record's stem.
fn key_of(payload: &Value) -> String {
    let own = readable(session(payload).unwrap_or_default());
    match field(payload, &["parent"]).as_str() {
        Some(root) => format!("opencode-{}.{own}", readable(root)),
        None => format!("opencode-{own}"),
    }
}

/// The actor `key` shows as in `status`, with its worktree's name.
fn shown(status: &Value, key: &str) -> Option<(String, Value)> {
    actors(status)
        .into_iter()
        .find(|(_, a)| *field(a, &["key"]) == key)
}

/// The record of the root session `id`, or of its subagent `agent`.
fn record<'a>(state: &'a ReplayState, id: &str, agent: Option<&str>) -> Option<&'a Value> {
    let agent = agent.map(readable);
    state.records.iter().find(|r| {
        *field(r, &["session_id"]) == readable(id).as_str()
            && field(r, &["agent_id"]).as_str() == agent.as_deref()
    })
}

/// The record of the payload's own session or subagent at `state`.
fn own_record(state: &ReplayState) -> Option<&Value> {
    let id = session(&state.payload)?;
    match field(&state.payload, &["parent"]).as_str() {
        Some(root) => record(state, root, Some(id)),
        None => record(state, id, None),
    }
}

fn named<'a>(states: &'a [ReplayState], event: &str) -> impl Iterator<Item = &'a ReplayState> {
    states.iter().filter(move |s| name(&s.payload) == event)
}

fn last<'a>(states: &'a [ReplayState], event: &str) -> Result<&'a ReplayState, String> {
    named(states, event)
        .last()
        .ok_or_else(|| format!("no {event} forwarded"))
}

fn state_of(record: Option<&Value>) -> Option<&str> {
    record.and_then(|r| field(r, &["state"]).as_str())
}

/// The wtpool asserts of `session` over its states, after those every OpenCode record holds.
pub(super) fn judge(session: &str, states: &[ReplayState], late: &Value) -> Result<(), String> {
    let judge = JUDGES
        .iter()
        .find(|(name, _)| *name == session)
        .map(|(_, judge)| judge)
        .ok_or_else(|| format!("no wtpool asserts for {session}"))?;
    every_record_is_opencode(states)?;
    judge(states, late)
}

/// Each record names `opencode` and its shared server process.
fn every_record_is_opencode(states: &[ReplayState]) -> Result<(), String> {
    let Some(at) = states.last() else {
        return Ok(());
    };
    for record in &at.records {
        let process = field(record, &["process"]);
        expect(
            *field(record, &["harness"]) == "opencode"
                && *field(process, &["pid"]) == SERVER
                && *field(process, &["shared"]) == true,
            format!(
                "the record of {} names harness {} and process {process}, not opencode's shared server {SERVER}",
                field(record, &["session_id"]),
                field(record, &["harness"])
            ),
        )?;
    }
    Ok(())
}

/// After the payload of the last `event`, its session's record is `idle` with nothing in flight.
fn idle_after(states: &[ReplayState], event: &str) -> Result<(), String> {
    let at = last(states, event)?;
    let own = own_record(at);
    expect(
        state_of(own) == Some("idle")
            && own.is_some_and(|r| {
                field(r, &["in_flight"])
                    .as_array()
                    .is_some_and(Vec::is_empty)
            }),
        format!("after {event} the record is {own:?}, not idle with nothing in flight"),
    )
}

fn provider_error_401(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let at = last(states, "session.execution.failed")?;
    let key = key_of(&at.payload);
    let actor = shown(&at.status, &key);
    expect(
        actor.as_ref().is_some_and(|(w, a)| {
            w == "fg"
                && *field(a, &["state"]) == "idle"
                && *field(a, &["error"]) == "provider.auth"
                && *field(a, &["label"]) == "Stub title err401"
                && *field(a, &["model"]) == "stub/scripted"
        }),
        format!("after the failed turn {key} shows as {actor:?}"),
    )
}

fn serve_shutdown(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    expect(
        states.is_empty(),
        format!(
            "the serve shutdown forwards nothing, yet {} payloads were replayed",
            states.len()
        ),
    )
}

fn run_read_edit_shell(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let calls: Vec<&ReplayState> = named(states, "tool.execute.before").collect();
    let read = calls.first().ok_or("no call forwarded")?;
    let key = key_of(&read.payload);
    let placed = shown(&read.status, &key);
    expect(
        placed.as_ref().is_some_and(|(w, a)| {
            w == "wt1"
                && *field(a, &["placed_by"]) == "path"
                && *field(a, &["state"]) == "working"
                && *field(a, &["action", "tool"]) == "read"
        }),
        format!("after the read of ../fg-wt1 the session shows as {placed:?}"),
    )?;
    let at = last(states, "session.execution.succeeded")?;
    let end = shown(&at.status, &key);
    expect(
        end.as_ref().is_some_and(|(w, a)| {
            w == "wt1"
                && *field(a, &["state"]) == "idle"
                && *field(a, &["label"]) == "Stub title s1"
        }),
        format!("after the turn the session shows as {end:?}"),
    )?;
    expect(
        at.records.len() == 1
            && at
                .records
                .iter()
                .all(|r| *field(r, &["model"]) == "stub/scripted"),
        format!("the records at the end: {:?}", at.records),
    )?;
    idle_after(states, "session.execution.succeeded")
}

fn subagent(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let created = last(states, "session.created")?;
    let key = key_of(&created.payload);
    let started = shown(&created.status, &key);
    expect(
        started.as_ref().is_some_and(|(w, a)| {
            w == "fg"
                && *field(a, &["kind"]) == "subagent"
                && *field(a, &["label"]) == "Read slot two notes"
        }),
        format!("after the child's creation {key} shows as {started:?}"),
    )?;
    let read = named(states, "tool.execute.before")
        .find(|s| field(&s.payload, &["parent"]).is_string())
        .ok_or("no call of the child forwarded")?;
    let moved = shown(&read.status, &key);
    expect(
        moved
            .as_ref()
            .is_some_and(|(w, a)| w == "wt2" && *field(a, &["placed_by"]) == "path"),
        format!("after the child's read in fg-wt2 {key} shows as {moved:?}"),
    )?;
    let ended = named(states, "session.execution.succeeded")
        .find(|s| field(&s.payload, &["parent"]).is_string())
        .ok_or("no end of the child forwarded")?;
    expect(
        state_of(own_record(ended)) == Some("done"),
        format!(
            "after the child's end its record is {:?}",
            own_record(ended)
        ),
    )?;
    let root = states
        .last()
        .and_then(|s| s.records.iter().find(|r| field(r, &["agent_id"]).is_null()));
    expect(
        state_of(root) == Some("idle"),
        format!("at the end the root's record is {root:?}"),
    )
}

/// Between each ask and its reply the asking session waits for permission, bound to its call.
fn waits_for_permission(states: &[ReplayState]) -> Result<(), String> {
    let asks: Vec<&ReplayState> = named(states, "permission.asked").collect();
    expect(!asks.is_empty(), "no permission.asked forwarded")?;
    for ask in asks {
        let own = own_record(ask);
        let call = field(&ask.payload, &["data", "source", "id"]).as_str();
        let actor = shown(&ask.status, &key_of(&ask.payload));
        expect(
            state_of(own) == Some("waiting")
                && own.is_some_and(|r| {
                    *field(r, &["waiting", "reason"]) == "permission"
                        && field(r, &["waiting", "call"]).as_str() == call
                })
                && actor.as_ref().is_some_and(|(_, a)| {
                    *field(a, &["state"]) == "waiting" && *field(a, &["reason"]) == "permission"
                }),
            format!("during the ask the record is {own:?} and the actor {actor:?}"),
        )?;
    }
    Ok(())
}

/// After each reply the session works again.
fn works_after_replies(states: &[ReplayState]) -> Result<(), String> {
    for reply in named(states, "permission.replied") {
        let own = own_record(reply);
        expect(
            state_of(own) == Some("working")
                && own.is_some_and(|r| field(r, &["waiting"]).is_null()),
            format!("after the reply the record is {own:?}"),
        )?;
    }
    Ok(())
}

fn permission_approve_reject(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    waits_for_permission(states)?;
    works_after_replies(states)?;
    let approved = named(states, "session.tool.success")
        .next()
        .ok_or("no end of the approved call")?;
    expect(
        state_of(own_record(approved)) == Some("working"),
        format!(
            "after the approved call the record is {:?}",
            own_record(approved)
        ),
    )?;
    idle_after(states, "session.execution.succeeded")?;
    idle_after(states, "session.execution.interrupted")
}

fn permission_run_autoreject(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    waits_for_permission(states)?;
    works_after_replies(states)?;
    idle_after(states, "session.execution.succeeded")
}

fn question(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    for event in ["tool.execute.before", "form.created"] {
        let at = last(states, event)?;
        let own = own_record(at);
        let actor = shown(&at.status, &key_of(&at.payload));
        expect(
            state_of(own) == Some("waiting")
                && own.is_some_and(|r| *field(r, &["waiting", "reason"]) == "question")
                && actor.as_ref().is_some_and(|(_, a)| {
                    *field(a, &["state"]) == "waiting" && *field(a, &["reason"]) == "question"
                }),
            format!("after {event} the record is {own:?} and the actor {actor:?}"),
        )?;
    }
    let replied = last(states, "form.replied")?;
    expect(
        state_of(own_record(replied)) == Some("working"),
        format!("after the answer the record is {:?}", own_record(replied)),
    )?;
    idle_after(states, "session.execution.succeeded")
}

fn interrupt(states: &[ReplayState], late: &Value) -> Result<(), String> {
    waits_for_permission(states)?;
    let interrupted: Vec<&ReplayState> = named(states, "session.execution.interrupted").collect();
    expect(
        interrupted.len() == 3,
        format!("{} interrupts forwarded, not 3", interrupted.len()),
    )?;
    for at in interrupted {
        let own = own_record(at);
        expect(
            state_of(own) == Some("idle")
                && own.is_some_and(|r| {
                    field(r, &["waiting"]).is_null()
                        && field(r, &["in_flight"])
                            .as_array()
                            .is_some_and(Vec::is_empty)
                }),
            format!("after an interrupt the record is {own:?}"),
        )?;
    }
    let quiet: Vec<String> = actors(late)
        .into_iter()
        .filter(|(_, a)| *field(a, &["state"]) != "idle")
        .map(|(_, a)| format!("{} {}", field(&a, &["key"]), field(&a, &["state"])))
        .collect();
    expect(
        quiet.is_empty(),
        format!("125 s after the interrupts these actors are not idle: {quiet:?}"),
    )
}

fn title_given(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let at = last(states, "session.execution.succeeded")?;
    let actor = shown(&at.status, &key_of(&at.payload));
    expect(
        actor.as_ref().is_some_and(|(_, a)| {
            *field(a, &["label"]) == "Given title probe" && *field(a, &["state"]) == "idle"
        }),
        format!("the session it looked up the given title of shows as {actor:?}"),
    )
}

fn long_answer(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let deltas: Vec<&ReplayState> = named(states, "session.text.delta").collect();
    let [first, second] = deltas.as_slice() else {
        return Err(format!(
            "{} progress payloads forwarded, not 2",
            deltas.len()
        ));
    };
    let early = own_record(first);
    expect(
        early.is_some_and(|r| {
            field(r, &["action"]).is_null() && *field(r, &["updated"]) == "2026-10-20T12:00:00Z"
        }),
        format!("a progress 1 s after the turn's start wrote {early:?}"),
    )?;
    let late_delta = own_record(second);
    let actor = shown(&second.status, &key_of(&second.payload));
    expect(
        late_delta.is_some_and(|r| {
            *field(r, &["state"]) == "working"
                && *field(r, &["action", "tool"]) == "generating"
                && *field(r, &["updated"]) == "2026-10-20T12:00:11Z"
        }) && actor.as_ref().is_some_and(|(_, a)| {
            *field(a, &["state"]) == "working" && *field(a, &["action", "tool"]) == "generating"
        }),
        format!(
            "a progress 11 s after the turn's start left {late_delta:?} and the actor {actor:?}"
        ),
    )?;
    idle_after(states, "session.execution.succeeded")
}

fn deny(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let call = last(states, "tool.execute.before")?;
    let id = field(&call.payload, &["data", "id"]).clone();
    let at = last(states, "session.execution.succeeded")?;
    idle_after(states, "session.execution.succeeded")?;
    let own = own_record(at);
    expect(
        own.is_some_and(|r| {
            field(r, &["done_ids"])
                .as_array()
                .is_some_and(|d| d.contains(&id))
        }),
        format!("the denied call {id} has not ended in {own:?}"),
    )
}

fn delete_api(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let created = last(states, "session.created")?;
    let key = key_of(&created.payload);
    let first = shown(&created.status, &key);
    expect(
        first.as_ref().is_some_and(|(w, a)| {
            w == "fg"
                && *field(a, &["state"]) == "idle"
                && *field(a, &["label"]) == "Given title via API"
        }),
        format!("after its creation the session shows as {first:?}"),
    )?;
    let started = last(states, "session.execution.started")?;
    expect(
        state_of(own_record(started)) == Some("working"),
        format!(
            "after the turn's start the record is {:?}",
            own_record(started)
        ),
    )?;
    idle_after(states, "session.execution.succeeded")?;
    let renamed = last(states, "session.renamed")?;
    let label = shown(&renamed.status, &key).map(|(_, a)| field(&a, &["label"]).clone());
    expect(
        label.as_ref().is_some_and(|l| l == "Renamed via API"),
        format!("after the rename the label is {label:?}"),
    )?;
    let deleted = last(states, "session.deleted")?;
    expect(
        state_of(own_record(deleted)) == Some("ended") && shown(&deleted.status, &key).is_none(),
        format!("after the delete the record is {:?}", own_record(deleted)),
    )
}

fn two_folders(states: &[ReplayState], _late: &Value) -> Result<(), String> {
    let started: Vec<&ReplayState> = named(states, "session.execution.started").collect();
    let both = started.last().ok_or("no turn started")?;
    let working: Vec<String> = actors(&both.status)
        .into_iter()
        .filter(|(_, a)| *field(a, &["state"]) == "working")
        .map(|(w, _)| w)
        .collect();
    expect(
        working == ["wt1", "wt2"],
        format!("with both turns started the working actors are on {working:?}"),
    )?;
    let at = states.last().ok_or("nothing replayed")?;
    let mut places: Vec<(String, String)> = at
        .records
        .iter()
        .map(|r| {
            let worktree = field(r, &["worktree"]).as_str().unwrap_or_default();
            let name = worktree.rsplit('/').next().unwrap_or_default().to_owned();
            (
                name,
                field(r, &["state"]).as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    places.sort();
    expect(
        places
            == [
                ("wt1".to_owned(), "idle".to_owned()),
                ("wt2".to_owned(), "idle".to_owned()),
            ],
        format!("the records of the one process at the end: {places:?}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ROOT: &str = "ses_e76df7854ffeXNVfL9JDlWFmtD";
    const CHILD: &str = "ses_e76df7772ffedONBqViK0NwG6Y";

    fn record_of(id: &str, state: &str) -> Value {
        json!({
            "harness": "opencode", "session_id": readable(id), "agent_id": null, "state": state,
            "waiting": null, "in_flight": [], "done_ids": [],
            "process": {"pid": SERVER, "started": "2026-10-20T11:00:00Z", "shared": true},
        })
    }

    fn at(payload: Value, records: Vec<Value>) -> ReplayState {
        ReplayState {
            payload,
            status: Value::Null,
            records,
        }
    }

    fn payload(name: &str, data: Value) -> Value {
        json!({"v": 1, "kind": "event", "name": name, "directory": "/p/fg", "data": data})
    }

    #[test]
    fn key_of__should_be_the_record_stem() {
        // Given
        let root = payload("session.execution.started", json!({"sessionID": ROOT}));
        let mut child = payload("session.execution.started", json!({"sessionID": CHILD}));
        child["parent"] = json!(ROOT);

        // When
        let keys = (key_of(&root), key_of(&child));

        // Then
        assert_eq!(
            keys,
            (
                "opencode-ses-e76df7854ffeXNVfL9JDlWFmtD".to_owned(),
                "opencode-ses-e76df7854ffeXNVfL9JDlWFmtD.ses-e76df7772ffedONBqViK0NwG6Y".to_owned()
            )
        );
    }

    #[test]
    fn judge__should_refuse_a_record_of_another_process() {
        // Given
        let mut record = record_of(ROOT, "idle");
        record["process"]["pid"] = json!(7001);
        let states = [at(
            payload("session.execution.succeeded", json!({"sessionID": ROOT})),
            vec![record],
        )];

        // When
        let found = judge("deny", &states, &Value::Null);

        // Then
        assert!(
            found.as_ref().is_err_and(|m| m.contains("shared server")),
            "{found:?}"
        );
    }

    #[test]
    fn idle_after__should_want_nothing_in_flight() {
        // Given
        let mut record = record_of(ROOT, "idle");
        record["in_flight"] = json!([{"id": "call_1"}]);
        let states = [at(
            payload("session.execution.succeeded", json!({"sessionID": ROOT})),
            vec![record],
        )];

        // When
        let found = idle_after(&states, "session.execution.succeeded");

        // Then
        assert!(found.is_err());
    }

    #[test]
    fn serve_shutdown__should_want_no_payload() {
        // When
        let empty = judge("serve_shutdown", &[], &Value::Null);

        // Then
        assert_eq!(empty, Ok(()));
    }

    #[test]
    fn judge__should_refuse_a_session_it_has_no_asserts_for() {
        // When
        let found = judge("unknown", &[], &Value::Null);

        // Then
        assert_eq!(found, Err("no wtpool asserts for unknown".to_owned()));
    }

    #[test]
    fn judges__should_name_every_forwarded_stream() {
        // Given
        let root = crate::repo::root();

        // When
        let mut streams: Vec<String> = super::super::opencode::sessions(&root)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let mut judged: Vec<String> = JUDGES.iter().map(|(name, _)| (*name).to_owned()).collect();
        streams.sort();
        judged.sort();

        // Then
        assert_eq!(judged, streams);
    }
}
