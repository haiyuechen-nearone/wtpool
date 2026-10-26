//! The worktree and state the hook records for a session and its subagents.

use rstest::rstest;

use super::*;

#[test]
fn handle__should_place_and_move_an_actor() {
    // Given
    let pool = Pool::must_new();

    // When
    pool.send(pool.event("UserPromptSubmit", json!({"session_title": "Fix"})));
    pool.send(pool.event(
        "PreToolUse",
        json!({"tool_name": "Read", "tool_use_id": "c1", "tool_input": {"file_path": pool.path("wt1/src/f.txt")}}),
    ));

    // Then
    let record = pool.record("s1").unwrap();
    assert_eq!(record["worktree"], pool.path("wt1"));
    assert_eq!(record["placed_by"], "path");
    assert_eq!(record["label"], "Fix");
    assert_eq!(record["action"]["target"], "src/f.txt");
    assert_eq!(record["process"]["pid"], 7001);
    assert_eq!(record["writer_started"], 1_792_497_600_000_000_i64);
}

#[rstest]
#[case::outside_the_pool("a", true)]
#[case::sandbox_without_the_variable("fg", false)]
fn handle__should_write_nothing(#[case] cwd: &str, #[case] sandbox_variable: bool) {
    // Given
    let mut pool = Pool::must_new();
    std::fs::create_dir_all(pool.root.join(cwd)).unwrap();
    std::fs::write(pool.activity().join(".verify-sandbox"), "").unwrap();
    pool.env.sandbox = sandbox_variable;

    // When
    pool.send(json!({"session_id": "s1", "hook_event_name": "Stop", "cwd": pool.path(cwd)}));
    pool.send(json!({"session_id": "s1", "hook_event_name": "Stop", "cwd": "relative"}));

    // Then
    assert!(pool.record("s1").is_none());
}

#[test]
fn handle__should_finish_and_delete_subagents() {
    // Given
    let pool = Pool::must_new();
    pool.send(pool.event("UserPromptSubmit", json!({})));
    pool.send(pool.event(
        "SubagentStart",
        json!({"agent_id": "a1", "agent_type": "Explore"}),
    ));
    pool.send(pool.event(
        "PreToolUse",
        json!({"agent_id": "a1", "tool_name": "Bash", "tool_use_id": "c1",
               "tool_input": {"command": format!("cd {} && git status", pool.path("wt1"))}}),
    ));
    pool.send(pool.event(
        "PermissionRequest",
        json!({"agent_id": "a1", "tool_name": "Bash",
               "tool_input": {"command": format!("cd {} && git status", pool.path("wt1"))}}),
    ));

    // When
    pool.send(pool.event(
        "Notification",
        json!({"notification_type": "permission_prompt"}),
    ));
    let session = pool.record("s1").unwrap();
    let waiting = pool.record("s1.a1").unwrap();
    pool.send(pool.event("Notification", json!({"notification_type": "auth_success"})));
    pool.send(pool.event("Notification", json!({"notification_type": "idle_prompt"})));
    let done = pool.record("s1.a1").unwrap();
    pool.send(pool.event("SessionEnd", json!({})));

    // Then
    assert_eq!(session["state"], "working");
    assert_eq!(waiting["state"], "waiting");
    assert_eq!(waiting["worktree"], pool.path("wt1"));
    assert_eq!(waiting["action"]["target"], "git status");
    assert_eq!(done["state"], "done");
    assert!(pool.record("s1.a1").is_none());
    assert_eq!(pool.record("s1").unwrap()["state"], "ended");
}

#[test]
fn handle__should_not_move_back_to_the_first_cwd() {
    // Given
    let pool = Pool::must_new();
    pool.send(pool.event("UserPromptSubmit", json!({})));
    pool.send(pool.event("CwdChanged", json!({"new_cwd": pool.path("wt1")})));

    // When
    pool.send(pool.event(
        "CwdChanged",
        json!({"new_cwd": pool.path("fg"), "cwd": pool.path("wt1")}),
    ));
    pool.send(pool.event(
        "PostToolBatch",
        json!({"tool_calls": [{"tool_use_id": "c1", "tool_name": "Read"}]}),
    ));
    pool.send(pool.event("StopFailure", json!({"error": {"code": 7}})));

    // Then
    let record = pool.record("s1").unwrap();
    assert_eq!(record["worktree"], pool.path("wt1"));
    assert_eq!(record["done_ids"], json!(["c1"]));
    assert_eq!(record["error"], r#"{"code":7}"#);
}

#[test]
fn handle__should_ignore_unknown_events_and_place_an_orphan_subagent() {
    // Given
    let pool = Pool::must_new();

    // When
    pool.send(pool.event("SomethingNew", json!({})));
    pool.send(pool.event("SubagentStart", json!({"agent_id": "a1"})));

    // Then
    assert!(pool.record("s1").is_none());
    let subagent = pool.record("s1.a1").unwrap();
    assert_eq!(subagent["worktree"], pool.path("fg"));
    assert_eq!(subagent["placed_by"], "cwd");
    assert_eq!(subagent["label"], "subagent");
}
