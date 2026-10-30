use rstest::rstest;
use serde_json::{Value, json};
use wtpool_core::activity::{ActorId, CallId, CallKind};
use wtpool_core::hook::{Shell, ToolPath};

use super::*;

const ROOT: &str = "ses_e76df7854ffeXNVfL9JDlWFmtD";
const CHILD: &str = "ses_e76df7772ffedONBqViK0NwG6Y";
const DIR: &str = "/sandbox/repo/fg";

fn root_id() -> ActorId {
    ActorId::try_from("ses-e76df7854ffeXNVfL9JDlWFmtD").unwrap()
}

fn child_id() -> ActorId {
    ActorId::try_from("ses-e76df7772ffedONBqViK0NwG6Y").unwrap()
}

fn call(id: &str) -> CallId {
    CallId::try_from(id).unwrap()
}

/// The payload of `name` with `data`, a child's event when `parent` is set.
fn payload(kind: &str, name: &str, parent: bool, extra: &Value, data: &Value) -> Vec<u8> {
    let mut payload = json!({"v": 1, "kind": kind, "name": name, "directory": DIR});
    if parent {
        payload["parent"] = json!(ROOT);
    }
    for (key, value) in extra.as_object().into_iter().flatten() {
        payload[key] = value.clone();
    }
    payload["data"] = data.clone();
    payload.to_string().into_bytes()
}

fn event_of(name: &str, parent: bool, data: &Value) -> HookInput {
    parse(&payload("event", name, parent, &json!({}), data)).unwrap()
}

fn root(event: Event) -> HookInput {
    HookInput::new(event, root_id(), DIR.to_owned())
}

fn child(event: Event) -> HookInput {
    HookInput {
        agent_id: Some(child_id()),
        ..root(event)
    }
}

fn session_of(parent: bool) -> &'static str {
    if parent { CHILD } else { ROOT }
}

#[test]
fn parse__should_start_a_root_session_with_its_title() {
    // When
    let input = event_of(
        "session.created",
        false,
        &json!({"sessionID": ROOT, "title": "Given title via API"}),
    );

    // Then
    assert_eq!(
        input,
        HookInput {
            title: Some("Given title via API".to_owned()),
            ..root(Event::SessionStart)
        }
    );
}

#[rstest]
#[case::null(json!(null))]
#[case::blank(json!("  "))]
#[case::placeholder(json!("New session - 2025-06-01T12:34:56.789Z"))]
fn parse__should_give_no_title_for_none(#[case] title: Value) {
    // When
    let input = event_of(
        "session.created",
        false,
        &json!({"sessionID": ROOT, "title": title}),
    );

    // Then
    assert_eq!(input, root(Event::SessionStart));
}

#[test]
fn parse__should_start_a_child_session_as_a_subagent() {
    // When
    let input = event_of(
        "session.created",
        true,
        &json!({"sessionID": CHILD, "parentID": ROOT, "title": "Read slot two notes", "agent": "explore"}),
    );

    // Then
    assert_eq!(
        input,
        HookInput {
            agent_type: Some("explore".to_owned()),
            title: Some("Read slot two notes".to_owned()),
            ..child(Event::SubagentStart)
        }
    );
}

#[rstest]
#[case::started_root("session.execution.started", false, json!({}), root(Event::TurnStart))]
#[case::started_child("session.execution.started", true, json!({}), child(Event::SubagentStart))]
#[case::succeeded_root("session.execution.succeeded", false, json!({}), root(Event::TurnEnd { error: None, subagents_done: true }))]
#[case::succeeded_child("session.execution.succeeded", true, json!({}), child(Event::SubagentEnd))]
#[case::interrupted_user("session.execution.interrupted", false, json!({"reason": "user"}), root(Event::TurnEnd { error: None, subagents_done: true }))]
#[case::interrupted_shutdown("session.execution.interrupted", false, json!({"reason": "shutdown"}), root(Event::TurnEnd { error: None, subagents_done: true }))]
#[case::interrupted_child("session.execution.interrupted", true, json!({"reason": "user"}), child(Event::SubagentEnd))]
#[case::failed_root("session.execution.failed", false, json!({"error": {"type": "provider.auth"}}), root(Event::TurnEnd { error: Some("provider.auth".to_owned()), subagents_done: true }))]
#[case::failed_child("session.execution.failed", true, json!({"error": {"type": "aborted"}}), child(Event::TurnEnd { error: Some("aborted".to_owned()), subagents_done: false }))]
#[case::failed_without_type("session.execution.failed", false, json!({}), root(Event::TurnEnd { error: Some("error".to_owned()), subagents_done: true }))]
#[case::deleted_root("session.deleted", false, json!({}), root(Event::SessionEnd))]
#[case::deleted_child("session.deleted", true, json!({}), child(Event::SubagentEnd))]
#[case::text_delta("session.text.delta", false, json!({}), root(Event::Progress))]
#[case::reasoning_delta("session.reasoning.delta", false, json!({}), root(Event::Progress))]
#[case::tool_input_delta("session.tool.input.delta", true, json!({}), child(Event::Progress))]
fn parse__should_map_turns_ends_and_progress(
    #[case] name: &str,
    #[case] parent: bool,
    #[case] fields: Value,
    #[case] expected: HookInput,
) {
    // Given
    let mut data = json!({"sessionID": session_of(parent)});
    for (key, value) in fields.as_object().into_iter().flatten() {
        data[key] = value.clone();
    }

    // When
    let input = event_of(name, parent, &data);

    // Then
    assert_eq!(input, expected);
}

fn tool_before(parent: bool, tool: &str, input: &Value) -> HookInput {
    let data =
        json!({"sessionID": session_of(parent), "id": "call_s1_0_0", "tool": tool, "input": input});
    parse(&payload(
        "hook",
        "tool.execute.before",
        parent,
        &json!({}),
        &data,
    ))
    .unwrap()
}

fn started(tool: &str, kind: CallKind, input: &Value) -> Event {
    let hash = serde_json::from_value::<CallInput>(input.clone()).unwrap();
    Event::CallStart(CallFacts {
        id: Some(call("call_s1_0_0")),
        tool: tool.to_owned(),
        kind,
        target: String::new(),
        input_hash: hash.hash().to_owned(),
    })
}

#[rstest]
#[case::read_relative("read", json!({"path": "../fg-wt1/notes.txt"}), CallKind::Other, ToolPath::File("/sandbox/repo/fg/../fg-wt1/notes.txt".to_owned()))]
#[case::edit("edit", json!({"path": "/sandbox/repo/fg-wt1/notes.txt"}), CallKind::Edit, ToolPath::File("/sandbox/repo/fg-wt1/notes.txt".to_owned()))]
#[case::write("write", json!({"path": "src/new.ts"}), CallKind::Edit, ToolPath::File("/sandbox/repo/fg/src/new.ts".to_owned()))]
#[case::shell("shell", json!({"command": "ls"}), CallKind::Shell, ToolPath::Command { command: "ls".to_owned(), shell: Shell::Posix })]
#[case::shell_cd_slot("shell", json!({"command": "cd ../fg-wt1 && git status"}), CallKind::Shell, ToolPath::Command { command: "cd ../fg-wt1 && git status".to_owned(), shell: Shell::Posix })]
#[case::subagent("subagent", json!({"agent": "explore"}), CallKind::Agent, ToolPath::None)]
#[case::question("question", json!({}), CallKind::Other, ToolPath::None)]
fn parse__should_start_a_call_with_its_kind_and_path(
    #[case] tool: &str,
    #[case] input: Value,
    #[case] kind: CallKind,
    #[case] path: ToolPath,
) {
    // When
    let found = tool_before(false, tool, &input);

    // Then
    assert_eq!(
        found,
        HookInput {
            tool_path: path,
            ..root(started(tool, kind, &input))
        }
    );
}

#[test]
fn parse__should_start_a_childs_call_on_the_subagent() {
    // Given
    let input = json!({"path": "/sandbox/repo/fg-wt2/notes.txt"});

    // When
    let found = tool_before(true, "read", &input);

    // Then
    assert_eq!(
        found,
        HookInput {
            tool_path: ToolPath::File("/sandbox/repo/fg-wt2/notes.txt".to_owned()),
            ..child(started("read", CallKind::Other, &input))
        }
    );
}

#[test]
fn parse__should_hash_a_call_without_input_as_null() {
    // When
    let found = parse(&payload(
        "hook",
        "tool.execute.before",
        false,
        &json!({}),
        &json!({"sessionID": ROOT, "id": "c1", "tool": "question"}),
    ))
    .unwrap();

    // Then
    let Event::CallStart(facts) = found.event else {
        unreachable!("{found:?}")
    };
    assert_eq!(facts.input_hash, CallInput::null().hash());
}

#[rstest]
#[case::success("session.tool.success", false)]
#[case::failed("session.tool.failed", false)]
#[case::failed_child("session.tool.failed", true)]
fn parse__should_end_a_call_by_its_id(#[case] name: &str, #[case] parent: bool) {
    // When
    let found = event_of(
        name,
        parent,
        &json!({"sessionID": session_of(parent), "id": "call_s3b_0_0"}),
    );

    // Then
    let expected = Event::CallEnd(CallRef::Id(call("call_s3b_0_0")));
    let expected = if parent {
        child(expected)
    } else {
        root(expected)
    };
    assert_eq!(found, expected);
}

#[rstest]
#[case::permission(
    "permission.asked",
    json!({"sessionID": ROOT, "id": "per_1", "source": {"id": "call_s3a_0_0"}}),
    Event::Wait(WaitReason::Permission, CallRef::Id(call("call_s3a_0_0")))
)]
#[case::permission_without_a_call(
    "permission.asked",
    json!({"sessionID": ROOT, "id": "per_1"}),
    Event::Notified(WaitReason::Permission)
)]
#[case::question_form(
    "form.created",
    json!({"form": {"id": "frm_1", "sessionID": ROOT, "metadata": {"kind": "question", "tool": {"id": "call_s4_0_0"}}}}),
    Event::Wait(WaitReason::Question, CallRef::Id(call("call_s4_0_0")))
)]
#[case::input_form(
    "form.created",
    json!({"form": {"id": "frm_1", "sessionID": ROOT, "metadata": {"kind": "mcp", "tool": {"id": "call_1"}}}}),
    Event::Wait(WaitReason::Input, CallRef::Id(call("call_1")))
)]
#[case::form_without_metadata(
    "form.created",
    json!({"form": {"id": "frm_1", "sessionID": ROOT}}),
    Event::Notified(WaitReason::Input)
)]
fn parse__should_wait_on_the_call_it_names(
    #[case] name: &str,
    #[case] data: Value,
    #[case] expected: Event,
) {
    // When
    let found = event_of(name, false, &data);

    // Then
    assert_eq!(found, root(expected));
}

#[rstest]
#[case::permission_once("permission.replied", json!({"sessionID": ROOT, "requestID": "per_1", "reply": "once"}), Some("call_s3a_0_0"), Some(call("call_s3a_0_0")))]
#[case::permission_reject("permission.replied", json!({"sessionID": ROOT, "requestID": "per_1", "reply": "reject"}), Some("call_s3b_0_0"), Some(call("call_s3b_0_0")))]
#[case::without_a_call("permission.replied", json!({"sessionID": ROOT, "requestID": "per_1", "reply": "once"}), None, None)]
#[case::form("form.replied", json!({"sessionID": ROOT, "id": "frm_1"}), Some("call_s4_0_0"), Some(call("call_s4_0_0")))]
fn parse__should_end_the_wait_of_a_reply(
    #[case] name: &str,
    #[case] data: Value,
    #[case] call_id: Option<&str>,
    #[case] expected: Option<CallId>,
) {
    // Given
    let extra = call_id.map_or_else(|| json!({}), |id| json!({"call": id}));

    // When
    let found = parse(&payload("event", name, false, &extra, &data)).unwrap();

    // Then
    assert_eq!(found, root(Event::WaitEnd(expected)));
}

#[rstest]
#[case::root(false, "Stub title s1")]
#[case::child(true, "Read slot two notes")]
fn parse__should_name_a_renamed_title(#[case] parent: bool, #[case] title: &str) {
    // When
    let found = event_of(
        "session.renamed",
        parent,
        &json!({"sessionID": session_of(parent), "title": title}),
    );

    // Then
    let expected = if parent {
        child(Event::Info)
    } else {
        root(Event::Info)
    };
    assert_eq!(
        found,
        HookInput {
            title: Some(title.to_owned()),
            ..expected
        }
    );
}

#[rstest]
#[case::root(false, json!({"id": ROOT, "title": "Given title probe"}))]
#[case::child(true, json!({"id": CHILD, "parentID": ROOT, "title": "Read slot two notes"}))]
fn parse__should_name_a_looked_up_title(#[case] parent: bool, #[case] data: Value) {
    // When
    let found = parse(&payload("lookup", "session.get", parent, &json!({}), &data)).unwrap();

    // Then
    let expected = if parent {
        child(Event::Info)
    } else {
        root(Event::Info)
    };
    assert_eq!(
        found,
        HookInput {
            title: data["title"].as_str().map(str::to_owned),
            ..expected
        }
    );
}

#[rstest]
#[case::root(false)]
#[case::child(true)]
fn parse__should_name_the_steps_model(#[case] parent: bool) {
    // When
    let found = event_of(
        "session.step.started",
        parent,
        &json!({"sessionID": session_of(parent), "model": {"id": "glm-5.3-flash-free", "providerID": "opencode"}}),
    );

    // Then
    let expected = if parent {
        child(Event::Info)
    } else {
        root(Event::Info)
    };
    assert_eq!(
        found,
        HookInput {
            model: Some("opencode/glm-5.3-flash-free".to_owned()),
            ..expected
        }
    );
}

#[rstest]
#[case::readable(ROOT, root_id())]
#[case::hashed("ses.1", ActorId::hashed("ses.1"))]
#[case::too_long(&format!("ses_{}", "a".repeat(61)), ActorId::hashed(&format!("ses_{}", "a".repeat(61))))]
#[case::valid_without_underscore("s-1", ActorId::try_from("s-1").unwrap())]
fn parse__should_map_the_session_id(#[case] raw: &str, #[case] expected: ActorId) {
    // When
    let found = event_of("session.deleted", false, &json!({"sessionID": raw}));

    // Then
    assert_eq!(found.session_id, expected);
}

#[test]
fn parse__should_hash_a_call_id_outside_the_pattern() {
    // When
    let found = event_of(
        "session.tool.success",
        false,
        &json!({"sessionID": ROOT, "id": "functions.read:0"}),
    );

    // Then
    assert_eq!(
        found.event,
        Event::CallEnd(CallRef::Id(CallId::hashed("functions.read:0")))
    );
}

#[rstest]
#[case::unknown_name(payload("event", "session.prompt", false, &json!({}), &json!({"sessionID": ROOT})), "the event \"session.prompt\" is not in the event table")]
#[case::unknown_v(br#"{"v":2,"kind":"event","name":"session.deleted","directory":"/p","data":{"sessionID":"s"}}"#.to_vec(), "the payload has version 2")]
#[case::missing_field(payload("hook", "tool.execute.before", false, &json!({}), &json!({"sessionID": ROOT, "tool": "read"})), "the tool.execute.before payload lacks id")]
#[case::relative_directory(br#"{"v":1,"kind":"event","name":"session.deleted","directory":"fg","data":{"sessionID":"s"}}"#.to_vec(), "the payload's directory is relative")]
fn parse__should_reject_with_a_reason_for_the_log(#[case] input: Vec<u8>, #[case] reason: &str) {
    // When
    let rejected = parse(&input);

    // Then
    assert_eq!(rejected, Err(Rejection::Invalid(reason.to_owned())));
}
