use std::path::PathBuf;

use rstest::rstest;
use serde_json::json;
use wtpool_core::activity::ActorId;
use wtpool_core::hook::{Event, ToolPath};

use super::*;

const TRANSCRIPT: &str = "/Users/me/.claude/projects/-Users-me-code-app/s1.jsonl";

fn payload(fields: serde_json::Value) -> Payload {
    let mut value = json!({"session_id": "s1", "hook_event_name": "PostToolUse", "cwd": "/p"});
    for (key, field) in fields.as_object().unwrap() {
        value[key] = field.clone();
    }
    Payload::parse(value.to_string().as_bytes()).unwrap()
}

fn background(id: serde_json::Value) -> serde_json::Value {
    json!({
        "tool_name": "Bash",
        "transcript_path": TRANSCRIPT,
        "tool_response": {"stdout": "", "stderr": "", "interrupted": false, "backgroundTaskId": id},
    })
}

/// A hook input of session `s1` whose transcript is `transcript`.
fn input(transcript: Option<&str>) -> HookInput {
    HookInput {
        event: Event::TurnStart,
        session_id: ActorId::try_from("s1").unwrap(),
        agent_id: None,
        agent_type: None,
        session_title: None,
        transcript: transcript.map(str::to_owned),
        spawned: None,
        cwd: "/p".to_owned(),
        new_cwd: None,
        tool_path: ToolPath::None,
        task_start: None,
        tasks_running: None,
        task_stopped: None,
    }
}

fn task(id: &str) -> TaskId {
    TaskId::try_from(id).unwrap()
}

#[rstest]
#[case::background(background(json!("b1")), Some("b1"))]
#[case::foreground(
    json!({"tool_name": "Bash", "transcript_path": TRANSCRIPT, "tool_response": {"stdout": "ok"}}),
    None
)]
#[case::id_outside_the_pattern(background(json!("../b1")), None)]
#[case::id_not_a_string(background(json!(7)), None)]
#[case::other_tool(
    json!({"tool_name": "Monitor", "transcript_path": TRANSCRIPT, "tool_response": {"backgroundTaskId": "b1"}}),
    None
)]
#[case::text_response(
    json!({"tool_name": "Bash", "transcript_path": TRANSCRIPT, "tool_response": "done"}),
    None
)]
fn task_start__should_read_a_background_bash_call(
    #[case] fields: serde_json::Value,
    #[case] expected: Option<&str>,
) {
    // When
    let found = task_start(&payload(fields));

    // Then
    assert_eq!(found.as_ref().map(TaskId::as_str), expected);
}

#[rstest]
#[case::the_sessions_transcript(TRANSCRIPT, Some("-Users-me-code-app"))]
#[case::relative_transcript("projects/-app/s1.jsonl", None)]
#[case::another_sessions_transcript("/c/projects/-app/s2.jsonl", None)]
#[case::dot_dot_project("/c/projects/../s1.jsonl", None)]
#[case::project_with_a_space("/c/projects/my app/s1.jsonl", None)]
#[case::project_256_long(&format!("/c/{}/s1.jsonl", "p".repeat(256)), None)]
#[case::project_255_long(&format!("/c/{}/s1.jsonl", "p".repeat(255)), Some(&*"p".repeat(255)))]
#[case::transcript_at_the_root("/s1.jsonl", None)]
fn project_of__should_take_the_folder_of_the_sessions_transcript(
    #[case] transcript: &str,
    #[case] expected: Option<&str>,
) {
    // When
    let project = project_of(transcript, "s1");

    // Then
    assert_eq!(project.as_deref(), expected);
}

#[rstest]
#[case::post_tool_use_failure("PostToolUseFailure")]
#[case::pre_tool_use("PreToolUse")]
fn task_start__should_read_only_a_post_tool_use(#[case] event: &str) {
    // Given
    let mut fields = background(json!("b1"));
    fields["hook_event_name"] = json!(event);

    // When
    let found = task_start(&payload(fields));

    // Then
    assert_eq!(found, None);
}

#[rstest]
#[case::stop_lists_two(
    "Stop",
    json!([{"id": "b1", "type": "shell", "status": "running"}, {"id": "a1", "type": "subagent"}]),
    Some(vec!["b1", "a1"])
)]
#[case::subagent_stop_lists_none("SubagentStop", json!([]), Some(vec![]))]
#[case::claude_code_2_1_310(
    "SubagentStop",
    json!([
        {"id": "a3b35a63408f56109", "type": "subagent", "status": "running",
         "description": "wait test", "agent_type": "general-purpose"},
        {"id": "bi3q285ez", "type": "shell", "status": "running",
         "description": "sleep 5; echo done", "command": "sleep 5; echo done"}
    ]),
    Some(vec!["a3b35a63408f56109", "bi3q285ez"])
)]
#[case::pending_is_in_flight(
    "Stop",
    json!([{"id": "b1", "type": "shell", "status": "pending"}]),
    Some(vec!["b1"])
)]
#[case::finished_are_not(
    "Stop",
    json!([
        {"id": "b1", "type": "shell", "status": "completed"},
        {"id": "b2", "type": "shell", "status": "failed"},
        {"id": "b3", "type": "shell", "status": "killed"},
        {"id": "b4", "type": "shell", "status": "running"}
    ]),
    Some(vec!["b4"])
)]
#[case::status_not_a_text(
    "Stop",
    json!([{"id": "b1", "type": "shell", "status": 1}, {"id": "b2", "status": null}, {"id": "b3"}]),
    Some(vec!["b1", "b2", "b3"])
)]
#[case::entry_without_an_id("Stop", json!([{"type": "shell"}, {"id": 5}, "b1"]), Some(vec![]))]
#[case::not_a_list("Stop", json!({"id": "b1"}), None)]
#[case::other_event("StopFailure", json!([{"id": "b1"}]), None)]
fn tasks_running__should_read_the_list_of_a_stop(
    #[case] event: &str,
    #[case] listed: serde_json::Value,
    #[case] expected: Option<Vec<&str>>,
) {
    // Given
    let fields = json!({"hook_event_name": event, "background_tasks": listed});

    // When
    let running = tasks_running(&payload(fields));

    // Then
    let running: Option<Vec<&str>> = running
        .as_ref()
        .map(|ids| ids.iter().map(TaskId::as_str).collect());
    assert_eq!(running, expected);
}

#[test]
fn tasks_running__should_be_none_without_the_list() {
    // When
    let running = tasks_running(&payload(json!({"hook_event_name": "Stop"})));

    // Then
    assert_eq!(running, None);
}

#[rstest]
#[case::task_stop("TaskStop", json!({"task_id": "b1"}), Some("b1"))]
#[case::task_stop_shell_id("TaskStop", json!({"shell_id": "b2"}), Some("b2"))]
#[case::task_stop_prefers_task_id("TaskStop", json!({"task_id": "b1", "shell_id": "b2"}), Some("b1"))]
#[case::kill_shell("KillShell", json!({"shell_id": "b3"}), Some("b3"))]
#[case::kill_shell_task_id("KillShell", json!({"task_id": "b3"}), None)]
#[case::id_outside_the_pattern("TaskStop", json!({"task_id": "../b1"}), None)]
#[case::other_tool("Bash", json!({"task_id": "b1"}), None)]
fn task_stopped__should_read_the_stopped_task(
    #[case] tool: &str,
    #[case] input: serde_json::Value,
    #[case] expected: Option<&str>,
) {
    // Given
    let fields = json!({"tool_name": tool, "tool_input": input});

    // When
    let stopped = task_stopped(&payload(fields));

    // Then
    assert_eq!(stopped.as_ref().map(TaskId::as_str), expected);
}

#[rstest]
#[case::pre_tool_use(json!({"hook_event_name": "PreToolUse", "tool_name": "TaskStop", "tool_input": {"task_id": "b1"}}))]
#[case::no_input(json!({"tool_name": "TaskStop"}))]
#[case::no_tool(json!({"tool_input": {"task_id": "b1"}}))]
fn task_stopped__should_need_a_post_tool_use_with_input(#[case] fields: serde_json::Value) {
    // When
    let stopped = task_stopped(&payload(fields));

    // Then
    assert_eq!(stopped, None);
}

#[test]
fn task_file__should_name_the_file_in_the_sessions_task_folder() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();

    // When
    let transcript = "/home/me/.claude/projects/-acme-app/s1.jsonl";
    let path = task_file(Some(dir.path()), 501, &input(Some(transcript)), &task("b1"));

    // Then
    let expected = base.join("claude-501/-acme-app/s1/tasks/b1.output");
    assert_eq!(path, Some(expected.to_string_lossy().into_owned()));
}

#[test]
fn task_file__should_find_the_file_a_resumed_session_wrote_under_its_own_id() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let project = dir
        .path()
        .canonicalize()
        .unwrap()
        .join("claude-501/-acme-app");
    let older = project.join("s2/tasks/b1.output");
    let newer = project.join("s3/tasks/b1.output");
    let folder = project.join("s4/tasks/b1.output");
    for path in [&older, &newer] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }
    std::fs::create_dir_all(&folder).unwrap();
    let file = std::fs::File::options().write(true).open(&older).unwrap();
    file.set_modified(std::time::UNIX_EPOCH).unwrap();

    // When
    let transcript = "/home/me/.claude/projects/-acme-app/s1.jsonl";
    let path = task_file(Some(dir.path()), 501, &input(Some(transcript)), &task("b1"));

    // Then
    assert_eq!(path, Some(newer.to_string_lossy().into_owned()));
}

#[cfg(unix)]
#[test]
fn task_file__should_resolve_a_symlinked_base() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().canonicalize().unwrap().join("real");
    std::fs::create_dir(&real).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    // When
    let path = task_file(Some(&link), 0, &input(Some("/c/p/s1.jsonl")), &task("b1"));

    // Then
    assert_eq!(
        path,
        Some(
            real.join("claude-0/p/s1/tasks/b1.output")
                .to_string_lossy()
                .into_owned()
        )
    );
}

#[cfg(unix)]
#[test]
fn task_file__should_use_tmp_without_claude_code_tmpdir() {
    // When
    let path = task_file(None, 501, &input(Some("/c/p/s1.jsonl")), &task("b1"));

    // Then
    let tmp = Path::new("/tmp").canonicalize().unwrap();
    assert_eq!(
        path,
        Some(
            tmp.join("claude-501/p/s1/tasks/b1.output")
                .to_string_lossy()
                .into_owned()
        )
    );
}

#[rstest]
#[case::missing_base(Some("missing"), Some("/c/p/s1.jsonl".to_owned()))]
#[case::project_too_long(None, Some(format!("/c/{}/s1.jsonl", "p".repeat(1000))))]
#[case::another_sessions_transcript(None, Some("/c/p/s2.jsonl".to_owned()))]
#[case::no_transcript(None, None)]
fn task_file__should_refuse_what_it_cannot_name(
    #[case] base: Option<&str>,
    #[case] transcript: Option<String>,
) {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let base = base.map_or_else(|| dir.path().to_path_buf(), |name| dir.path().join(name));

    // When
    let path = task_file(Some(&base), 501, &input(transcript.as_deref()), &task("b1"));

    // Then
    assert_eq!(path, None);
}

#[rstest]
#[case::the_task_file(
    "/t/claude-501/p/s1/tasks/b1.output",
    "/t/claude-501/p/s1/tasks",
    "b1",
    true
)]
#[case::relative(
    "t/claude-501/p/s1/tasks/b1.output",
    "t/claude-501/p/s1/tasks",
    "b1",
    false
)]
#[case::dot_dot(
    "/t/claude-501/p/s1/tasks/../tasks/b1.output",
    "/t/claude-501/p/s1/tasks/../tasks",
    "b1",
    false
)]
#[case::other_folder(
    "/t/claude-501/p/s2/tasks/b1.output",
    "/t/claude-501/p/s1/tasks",
    "b1",
    false
)]
#[case::nested(
    "/t/claude-501/p/s1/tasks/x/b1.output",
    "/t/claude-501/p/s1/tasks",
    "b1",
    false
)]
#[case::other_name(
    "/t/claude-501/p/s1/tasks/b2.output",
    "/t/claude-501/p/s1/tasks",
    "b1",
    false
)]
#[case::not_output(
    "/t/claude-501/p/s1/tasks/b1.log",
    "/t/claude-501/p/s1/tasks",
    "b1",
    false
)]
#[case::control_character(
    "/t/\u{1b}[2J/s1/tasks/b1.output",
    "/t/\u{1b}[2J/s1/tasks",
    "b1",
    false
)]
#[case::bidi_override(
    "/t/\u{202e}p/s1/tasks/b1.output",
    "/t/\u{202e}p/s1/tasks",
    "b1",
    false
)]
#[case::other_letters("/t/é ü/p/s1/tasks/b1.output", "/t/é ü/p/s1/tasks", "b1", true)]
fn is_task_file__should_accept_only_the_file_in_the_folder(
    #[case] path: &str,
    #[case] folder: &str,
    #[case] id: &str,
    #[case] expected: bool,
) {
    // When
    let accepted = is_task_file(Path::new(path), Path::new(folder), id);

    // Then
    assert_eq!(accepted, expected);
}

#[rstest]
#[case::at_the_limit(MAX_TASK_PATH, true)]
#[case::past_the_limit(MAX_TASK_PATH + 1, false)]
fn is_task_file__should_keep_1024_bytes(#[case] length: usize, #[case] expected: bool) {
    // Given
    let tail = "/tasks/b1.output";
    let folder = format!("/{}/tasks", "p".repeat(length - tail.len() - 1));
    let path = PathBuf::from(format!("{folder}/b1.output"));

    // When
    let accepted = is_task_file(&path, Path::new(&folder), "b1");

    // Then
    assert_eq!(path.as_os_str().len(), length);
    assert_eq!(accepted, expected);
}

#[rstest]
#[case::background(
    r#"{"stdout":"","backgroundTaskId":"b1","n":[1,{"a":2}]}"#,
    Some("b1"),
    None
)]
#[case::id_not_a_string(r#"{"backgroundTaskId":["b1"]}"#, None, None)]
#[case::foreground(r#"{"stdout":"x"}"#, None, None)]
#[case::nested(r#"{"file":{"backgroundTaskId":"b9"}}"#, None, None)]
#[case::both_ids(r#"{"agentId":"a1","backgroundTaskId":"b1"}"#, Some("b1"), Some("a1"))]
#[case::list(r#"[{"backgroundTaskId":"b1"}]"#, None, None)]
fn tool_response__should_read_the_background_task_id(
    #[case] response: &str,
    #[case] expected: Option<&str>,
    #[case] agent: Option<&str>,
) {
    // Given
    let fields =
        json!({ "tool_response": serde_json::from_str::<serde_json::Value>(response).unwrap() });

    // When
    let read = payload(fields).tool_response.unwrap_or_default();

    // Then
    assert_eq!(read.background_task_id.as_deref(), expected);
    assert_eq!(read.agent_id.as_deref(), agent);
}
