//! The neutral events OpenCode's adapter maps its payloads onto: a session's start, its title and
//! model, progress, a wait bound by call id and its end, and a turn that ends in an error.

use std::fs;

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::hook::Hooks;
use crate::support::hook::opencode::{CHILD, ROOT, child_stem, of_child, stem, with_call};

const SHELL: &str = "echo held-permission-probe";
/// A session no record names.
const OTHER: &str = "ses_e76d00000ffeOtherSession00";

fn record_bytes(hooks: &Hooks, stem: &str) -> Option<Vec<u8>> {
    fs::read(hooks.activity().join(format!("{stem}.json"))).ok()
}

fn started(hooks: &Hooks) {
    hooks.oc_send(&hooks.oc_event("session.execution.started", json!({"sessionID": ROOT})));
}

fn child_created(hooks: &Hooks) {
    let created = json!({"sessionID": CHILD, "parentID": ROOT, "title": "Read slot two notes", "agent": "explore"});
    let mut payload = hooks.oc_event("session.created", created);
    payload["parent"] = json!(ROOT);
    hooks.oc_send(&payload);
}

fn asked(hooks: &Hooks, call: &str) -> Value {
    hooks.oc_event(
        "permission.asked",
        json!({"sessionID": ROOT, "id": format!("per_{call}"), "source": {"id": call}}),
    )
}

fn replied(hooks: &Hooks) -> Value {
    hooks.oc_event(
        "permission.replied",
        json!({"sessionID": ROOT, "requestID": "per_x", "reply": "once"}),
    )
}

/// The session already there when `session.created` comes.
#[derive(Clone, Copy, Debug)]
enum Known {
    Unknown,
    Working,
}

#[rstest]
#[case::unknown(Known::Unknown)]
#[case::known(Known::Working)]
fn session_start__should_create_an_idle_record_once(#[case] known: Known) {
    // Given
    let hooks = Hooks::must_with_server("oc-session-start");
    if matches!(known, Known::Working) {
        started(&hooks);
        hooks.advance(30);
    }
    let before = record_bytes(&hooks, &stem(ROOT));
    let mut created = hooks.oc_event("session.created", json!({"sessionID": ROOT}));
    created["directory"] = json!(hooks.worktree_text("wt1"));

    // When
    hooks.oc_send(&created);

    // Then
    match known {
        Known::Unknown => {
            let record = hooks.must_record(&stem(ROOT));
            assert_eq!(
                (&record["state"], &record["worktree"], &record["placed_by"]),
                (
                    &json!("idle"),
                    &json!(hooks.worktree_text("wt1")),
                    &json!("cwd")
                ),
                "{record}"
            );
        }
        Known::Working => {
            assert_eq!(
                record_bytes(&hooks, &stem(ROOT)),
                before,
                "session_start rewrote a known session"
            );
        }
    }
}

/// Where an `info` event lands.
#[derive(Clone, Copy, Debug)]
enum InfoOn {
    WorkingSession,
    UnknownSession,
    UnknownSubagent,
}

#[rstest]
#[case::title_on_a_working_session(InfoOn::WorkingSession, "session.renamed")]
#[case::model_on_a_working_session(InfoOn::WorkingSession, "session.step.started")]
#[case::title_creates_an_unknown_session(InfoOn::UnknownSession, "session.renamed")]
#[case::model_creates_an_unknown_session(InfoOn::UnknownSession, "session.step.started")]
#[case::title_leaves_an_unknown_subagent(InfoOn::UnknownSubagent, "session.renamed")]
#[case::model_leaves_an_unknown_subagent(InfoOn::UnknownSubagent, "session.step.started")]
fn info__should_set_title_and_model_only(#[case] on: InfoOn, #[case] event: &str) {
    // Given
    let hooks = Hooks::must_with_server("oc-info");
    if matches!(on, InfoOn::WorkingSession | InfoOn::UnknownSubagent) {
        started(&hooks);
    }
    let before = hooks.record(&stem(ROOT));
    hooks.advance(30);
    let data = if event == "session.renamed" {
        json!({"sessionID": ROOT, "title": "Stub title s1"})
    } else {
        json!({"sessionID": ROOT, "model": {"id": "glm-5.3-flash-free", "providerID": "opencode"}})
    };
    let mut payload = hooks.oc_event(event, data);
    if matches!(on, InfoOn::UnknownSubagent) {
        payload = of_child(payload, ROOT, CHILD);
    }

    // When
    hooks.oc_send(&payload);

    // Then
    let (field, value) = if event == "session.renamed" {
        ("label", json!("Stub title s1"))
    } else {
        ("model", json!("opencode/glm-5.3-flash-free"))
    };
    match on {
        InfoOn::WorkingSession => {
            let before = before.expect("setup: the working session's record");
            let after = hooks.must_record(&stem(ROOT));
            assert_eq!(after[field], value, "{after}");
            for unchanged in ["state", "since", "updated"] {
                assert_eq!(
                    after[unchanged], before[unchanged],
                    "{unchanged} of {after}"
                );
            }
            if event == "session.renamed" {
                assert_eq!(
                    after["title"],
                    json!({"text": "Stub title s1", "by": "harness"})
                );
            }
        }
        InfoOn::UnknownSession => {
            let after = hooks.must_record(&stem(ROOT));
            assert_eq!(after["state"], "idle", "{after}");
            assert_eq!(after[field], value, "{after}");
        }
        InfoOn::UnknownSubagent => {
            assert!(
                hooks.record(&child_stem(ROOT, CHILD)).is_none(),
                "an info created an unknown subagent: {:?}",
                hooks.files()
            );
        }
    }
}

/// The actor a progress lands on.
#[derive(Clone, Copy, Debug)]
enum ProgressOn {
    Working,
    WorkingWithACall,
    WorkingWithin10Seconds,
    Idle,
    Done,
    Waiting,
    Ended,
    Unknown,
}

#[rstest]
#[case::working(ProgressOn::Working, true)]
#[case::working_with_a_call(ProgressOn::WorkingWithACall, true)]
#[case::within_10_s(ProgressOn::WorkingWithin10Seconds, false)]
#[case::idle(ProgressOn::Idle, false)]
#[case::done(ProgressOn::Done, false)]
#[case::waiting(ProgressOn::Waiting, false)]
#[case::ended(ProgressOn::Ended, false)]
#[case::unknown(ProgressOn::Unknown, false)]
fn progress__should_keep_only_working_records_busy(#[case] on: ProgressOn, #[case] writes: bool) {
    // Given
    let hooks = Hooks::must_with_server("oc-progress");
    let subagent = matches!(on, ProgressOn::Done);
    if !matches!(on, ProgressOn::Unknown) {
        started(&hooks);
    }
    match on {
        ProgressOn::WorkingWithACall | ProgressOn::Waiting => {
            hooks.oc_send(&hooks.oc_call(ROOT, "call_a", "shell", json!({"command": SHELL})));
        }
        ProgressOn::Idle => {
            hooks.oc_send(
                &hooks.oc_event("session.execution.succeeded", json!({"sessionID": ROOT})),
            );
        }
        ProgressOn::Done => {
            child_created(&hooks);
            let ended = hooks.oc_event("session.execution.succeeded", json!({}));
            hooks.oc_send(&of_child(ended, ROOT, CHILD));
        }
        ProgressOn::Ended => {
            hooks.oc_send(&hooks.oc_event("session.deleted", json!({"sessionID": ROOT})));
        }
        _ => {}
    }
    if matches!(on, ProgressOn::Waiting) {
        hooks.oc_send(&asked(&hooks, "call_a"));
    }
    let file = if subagent {
        child_stem(ROOT, CHILD)
    } else {
        stem(ROOT)
    };
    let now = hooks.advance(if matches!(on, ProgressOn::WorkingWithin10Seconds) {
        5
    } else {
        60
    });
    let before = record_bytes(&hooks, &file);
    let before_action = hooks.record(&file).map(|r| r["action"].clone());
    let mut delta = hooks.oc_event("session.text.delta", json!({"sessionID": ROOT}));
    if subagent {
        delta = of_child(delta, ROOT, CHILD);
    }

    // When
    hooks.oc_send(&delta);

    // Then
    let after = record_bytes(&hooks, &file);
    if !writes {
        assert_eq!(after, before, "a progress on {on:?} wrote");
        return;
    }
    let record = hooks.must_record(&file);
    assert_eq!(record["updated"], now.as_str(), "{record}");
    assert_eq!(record["state"], "working", "{record}");
    let action = if matches!(on, ProgressOn::WorkingWithACall) {
        before_action.unwrap_or_default()
    } else {
        json!({"tool": "generating", "target": ""})
    };
    assert_eq!(record["action"], action, "{record}");
}

#[rstest]
#[case::the_newer_call_with_the_same_tool_and_hash("call_b", Some("call_b"))]
#[case::the_older_call("call_a", Some("call_a"))]
#[case::an_id_not_in_flight("call_elsewhere", None)]
fn wait__should_bind_by_call_id_first(#[case] source: &str, #[case] bound: Option<&str>) {
    // Given
    let hooks = Hooks::must_with_server("oc-wait-bind");
    started(&hooks);
    for call in ["call_a", "call_b"] {
        hooks.oc_send(&hooks.oc_call(ROOT, call, "shell", json!({"command": SHELL})));
        hooks.advance(1);
    }

    // When
    hooks.oc_send(&asked(&hooks, source));

    // Then
    let record = hooks.must_record(&stem(ROOT));
    assert_eq!(
        (&record["state"], &record["waiting"]["reason"]),
        (&json!("waiting"), &json!("permission")),
        "{record}"
    );
    if let Some(bound) = bound {
        assert_eq!(record["waiting"]["call"], bound, "{record}");
    }
}

/// The reply a wait gets.
#[derive(Clone, Copy, Debug)]
enum Reply {
    ItsCall,
    WithoutACall,
    AnotherCall,
    UnknownActor,
}

#[rstest]
#[case::its_call(Reply::ItsCall, false)]
#[case::without_a_call(Reply::WithoutACall, false)]
#[case::another_call(Reply::AnotherCall, true)]
#[case::unknown_actor(Reply::UnknownActor, true)]
fn wait_end__should_end_only_its_wait(#[case] reply: Reply, #[case] still_waiting: bool) {
    // Given
    let hooks = Hooks::must_with_server("oc-wait-end");
    started(&hooks);
    for call in ["call_a", "call_b"] {
        hooks.oc_send(&hooks.oc_call(
            ROOT,
            call,
            "shell",
            json!({"command": format!("{SHELL} {call}")}),
        ));
    }
    hooks.oc_send(&asked(&hooks, "call_a"));
    hooks.advance(5);
    let payload = match reply {
        Reply::ItsCall => with_call(replied(&hooks), "call_a"),
        Reply::WithoutACall => replied(&hooks),
        Reply::AnotherCall => with_call(replied(&hooks), "call_b"),
        Reply::UnknownActor => {
            let mut other = replied(&hooks);
            other["data"]["sessionID"] = json!(OTHER);
            other
        }
    };

    // When
    hooks.oc_send(&payload);

    // Then
    let record = hooks.must_record(&stem(ROOT));
    if still_waiting {
        assert_eq!(
            (&record["state"], &record["waiting"]["call"]),
            (&json!("waiting"), &json!("call_a")),
            "{record}"
        );
    } else {
        assert_eq!(
            (&record["state"], &record["waiting"]),
            (&json!("working"), &Value::Null),
            "{record}"
        );
    }
    if matches!(reply, Reply::UnknownActor) {
        assert!(
            hooks.record(&stem(OTHER)).is_none(),
            "a reply created a record: {:?}",
            hooks.files()
        );
    }
}

#[rstest]
#[case::failed("session.execution.failed", json!({"error": {"type": "provider.auth"}}), Some("provider.auth"))]
#[case::interrupted("session.execution.interrupted", json!({"reason": "user"}), None)]
fn turn_end__should_finish_subagents_with_an_error(
    #[case] event: &str,
    #[case] extra: Value,
    #[case] error: Option<&str>,
) {
    // Given
    let hooks = Hooks::must_with_server("oc-turn-end");
    started(&hooks);
    child_created(&hooks);
    hooks.advance(10);
    let mut data = extra;
    data["sessionID"] = json!(ROOT);

    // When
    hooks.oc_send(&hooks.oc_event(event, data));

    // Then
    let root = hooks.must_record(&stem(ROOT));
    let child = hooks.must_record(&child_stem(ROOT, CHILD));
    assert_eq!(
        (&root["state"], &root["error"]),
        (&json!("idle"), &json!(error)),
        "{root}"
    );
    assert_eq!(child["state"], "done", "{child}");
}
