use rstest::rstest;
use wtpool_core::activity::CallKind;
use wtpool_core::hook::Shell;

use super::*;

const BASE: &str = r#""session_id":"s1","cwd":"/p""#;

fn must_parse(fields: &str) -> HookInput {
    parse(format!("{{{BASE},{fields}}}").as_bytes()).unwrap()
}

/// The facts of a call event whose payload names no call.
fn no_call() -> CallFacts {
    call_facts(None, None, &CallInput::null())
}

#[rstest]
#[case::prompt("UserPromptSubmit", "", Some(Event::TurnStart))]
#[case::permission_request("PermissionRequest", "", Some(Event::PermissionAsked(no_call())))]
#[case::post("PostToolUse", "", Some(Event::CallEnd(no_call())))]
#[case::post_failure("PostToolUseFailure", "", Some(Event::CallEnd(no_call())))]
#[case::denied("PermissionDenied", "", Some(Event::CallEnd(no_call())))]
#[case::batch_without_calls("PostToolBatch", "", Some(Event::CallsEnd(Vec::new())))]
#[case::permission_prompt(
    "Notification",
    "permission_prompt",
    Some(Event::Notified(WaitReason::Permission))
)]
#[case::elicitation(
    "Notification",
    "elicitation_dialog",
    Some(Event::Notified(WaitReason::Input))
)]
#[case::elicitation_url(
    "Notification",
    "elicitation_url_dialog",
    Some(Event::Notified(WaitReason::Input))
)]
#[case::idle_prompt("Notification", "idle_prompt", Some(Event::Idle))]
#[case::other_notification("Notification", "auth_success", None)]
#[case::notification_without_a_type("Notification", "", None)]
#[case::stop("Stop", "", Some(Event::TurnEnd(None)))]
#[case::stop_failure("StopFailure", "", Some(Event::TurnEnd(Some("error".to_owned()))))]
#[case::subagent_start("SubagentStart", "", Some(Event::SubagentStart))]
#[case::subagent_stop("SubagentStop", "", Some(Event::SubagentEnd))]
#[case::cwd_changed("CwdChanged", "", Some(Event::CwdChange))]
#[case::session_end("SessionEnd", "", Some(Event::SessionEnd))]
#[case::unknown("SomethingNew", "", None)]
fn parse__should_follow_the_event_table(
    #[case] name: &str,
    #[case] notification: &str,
    #[case] expected: Option<Event>,
) {
    // Given
    let notification = match notification {
        "" => String::new(),
        kind => format!(r#","notification_type":"{kind}""#),
    };
    let input = format!(r#"{{{BASE},"hook_event_name":"{name}"{notification}}}"#);

    // When
    let parsed = parse(input.as_bytes());

    // Then
    assert_eq!(parsed.map(|input| input.event), expected);
    assert_eq!(no_call().kind, CallKind::Other);
}

#[test]
fn parse__should_give_a_call_its_kind_and_path() {
    // When
    let input = must_parse(
        r#""hook_event_name":"PreToolUse","agent_id":"a1","agent_type":"Explore","tool_name":"Bash","tool_use_id":"c1","tool_input":{"command":"git status"}"#,
    );

    // Then
    let Event::CallStart(call) = &input.event else {
        panic!("{:?}", input.event)
    };
    assert_eq!(call.id.as_ref().map(CallId::as_str), Some("c1"));
    assert_eq!((call.tool.as_str(), call.kind), ("Bash", CallKind::Shell));
    assert_eq!(
        call.input_hash,
        "e0d3e391760d0a9b6c24bf66cecfc5a66557784782cbc704052385bf6e9bb287"
    );
    assert_eq!(
        input.tool_path,
        ToolPath::Command {
            command: "git status".to_owned(),
            shell: Shell::Posix
        }
    );
    assert_eq!(
        (
            input.agent_id.as_ref().map(ActorId::as_str),
            input.agent_type.as_deref()
        ),
        (Some("a1"), Some("Explore"))
    );
    assert_eq!(
        (input.session_id.as_str(), input.cwd.as_str()),
        ("s1", "/p")
    );
}

#[test]
fn parse__should_give_each_batch_call_its_kind_and_no_path() {
    // When
    let input = must_parse(
        r#""hook_event_name":"PostToolBatch","tool_name":"Read","tool_input":{"file_path":"/p/f"},"tool_calls":[{"tool_use_id":"c1","tool_name":"Edit"},{"tool_name":"Agent","tool_input":{"x":1}}]"#,
    );

    // Then
    let Event::CallsEnd(calls) = &input.event else {
        panic!("{:?}", input.event)
    };
    let kinds: Vec<(Option<&str>, &str, CallKind)> = calls
        .iter()
        .map(|call| {
            (
                call.id.as_ref().map(CallId::as_str),
                call.tool.as_str(),
                call.kind,
            )
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            (Some("c1"), "Edit", CallKind::Edit),
            (None, "Agent", CallKind::Agent)
        ]
    );
    assert_eq!(
        (calls[0].input_hash.as_str(), calls[1].input_hash.as_str()),
        (
            "74234e98afe7498fb5daf1f36ac2d78acc339464f950703b8c019892f982b90b",
            "5041bf1f713df204784353e82f6a4a535931cb64f1f4b4a5aeaffcb720918b22"
        )
    );
    assert_eq!(input.tool_path, ToolPath::None);
}

#[test]
fn parse__should_keep_the_title_and_new_folder() {
    // When
    let input = must_parse(
        r#""hook_event_name":"CwdChanged","new_cwd":"/p/wt1","session_title":"Fix","tool_name":"Read","tool_input":{"file_path":"/p/f"}"#,
    );

    // Then
    assert_eq!(input.event, Event::CwdChange);
    assert_eq!(input.new_cwd.as_deref(), Some("/p/wt1"));
    assert_eq!(input.session_title.as_deref(), Some("Fix"));
    assert_eq!(input.tool_path, ToolPath::None);
}

const AGENT_INPUT: &str =
    r#""tool_input":{"description":"Find the tests","subagent_type":"Explore"}"#;

#[rstest]
#[case::agent_launched("PostToolUse", "Agent", r#""tool_response":{"agentId":"a1"}"#, Some(("a1", "Find the tests")))]
#[case::agent_failed("PostToolUseFailure", "Agent", r#""tool_response":{"agentId":"a1"}"#, Some(("a1", "Find the tests")))]
#[case::agent_started("PreToolUse", "Agent", r#""tool_response":{"agentId":"a1"}"#, None)]
#[case::another_tool("PostToolUse", "Read", r#""tool_response":{"agentId":"a1"}"#, None)]
#[case::no_agent_id(
    "PostToolUse",
    "Agent",
    r#""tool_response":{"status":"completed"}"#,
    None
)]
#[case::bad_agent_id("PostToolUse", "Agent", r#""tool_response":{"agentId":"../a1"}"#, None)]
#[case::no_response("PostToolUse", "Agent", r#""x":1"#, None)]
fn parse__should_name_the_subagent_an_agent_call_started(
    #[case] event: &str,
    #[case] tool: &str,
    #[case] response: &str,
    #[case] expected: Option<(&str, &str)>,
) {
    // When
    let input = must_parse(&format!(
        r#""hook_event_name":"{event}","tool_name":"{tool}","tool_use_id":"c1",{AGENT_INPUT},{response}"#
    ));

    // Then
    let spawned = input
        .spawned
        .as_ref()
        .map(|spawned| (spawned.agent_id.as_str(), spawned.title.as_str()));
    assert_eq!(spawned, expected);
}

#[test]
fn parse__should_name_no_subagent_without_a_description() {
    // When
    let input = must_parse(
        r#""hook_event_name":"PostToolUse","tool_name":"Agent","tool_input":{"prompt":"p"},"tool_response":{"agentId":"a1"}"#,
    );
    let without_input = must_parse(
        r#""hook_event_name":"PostToolUse","tool_name":"Agent","tool_response":{"agentId":"a1"}"#,
    );

    // Then
    assert_eq!((input.spawned, without_input.spawned), (None, None));
}

#[test]
fn parse__should_keep_the_transcript() {
    // When
    let input = must_parse(r#""hook_event_name":"Stop","transcript_path":"/t/s1.jsonl""#);

    // Then
    assert_eq!(input.transcript.as_deref(), Some("/t/s1.jsonl"));
}

#[rstest]
#[case::valid(format!(r#"{{{BASE},"hook_event_name":"Stop"}}"#), Some("Stop"))]
#[case::ignored_event(format!(r#"{{{BASE},"hook_event_name":"SomethingNew"}}"#), Some("SomethingNew"))]
#[case::malformed("{".to_owned(), None)]
fn event_name__should_read_any_parsed_payload(
    #[case] input: String,
    #[case] expected: Option<&str>,
) {
    // When
    let name = event_name(input.as_bytes());

    // Then
    assert_eq!(name.as_deref(), expected);
}

#[test]
fn hook_event__should_name_each_event_of_the_table_once() {
    // When
    let named: Vec<Option<HookEvent>> = HookEvent::ALL
        .iter()
        .map(|event| HookEvent::named(event.name()))
        .collect();

    // Then
    let all: Vec<Option<HookEvent>> = HookEvent::ALL.into_iter().map(Some).collect();
    assert_eq!(named, all);
    let asynchronous = HookEvent::ALL.into_iter().filter(|event| event.is_async());
    assert_eq!(asynchronous.count(), 5);
}
