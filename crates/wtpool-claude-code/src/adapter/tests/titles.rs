//! The labels of sessions and subagents, from payload and transcript titles.

use super::*;

#[test]
fn handle__should_label_a_session_with_its_transcript_title() {
    // Given
    let pool = Pool::must_new();
    let transcript = pool.root.join("s1.jsonl");
    let ai = r#"{"type":"ai-title","aiTitle":"Bind the branch\u0007 to wt3"}"#;
    std::fs::write(&transcript, format!("{ai}\n")).unwrap();
    let path = transcript.to_string_lossy().into_owned();
    pool.send(pool.event("UserPromptSubmit", json!({"transcript_path": path})));
    let generated = pool.record("s1").unwrap();
    let custom = r#"{"type":"custom-title","customTitle":"wtpool panel build"}"#;
    std::fs::write(&transcript, format!("{ai}\n{custom}\n{ai}\n")).unwrap();

    // When
    pool.send(pool.event("Stop", json!({"transcript_path": path})));
    pool.send(pool.event(
        "SubagentStart",
        json!({"agent_id": "a1", "agent_type": "Explore", "transcript_path": path}),
    ));
    pool.send(pool.event("Stop", json!({"transcript_path": "s1.jsonl"})));

    // Then
    assert_eq!(generated["label"], "Bind the branch to wt3");
    assert_eq!(generated["transcript_offset"], ai.len() + 1);
    let session = pool.record("s1").unwrap();
    assert_eq!(session["label"], "wtpool panel build");
    let len = std::fs::metadata(&transcript).unwrap().len();
    assert_eq!(session["transcript_offset"], len);
    let subagent = pool.record("s1.a1").unwrap();
    assert_eq!(
        (&subagent["label"], &subagent["transcript_offset"]),
        (&json!("Explore"), &json!(0))
    );
}

#[test]
fn handle__should_prefer_the_payload_title_and_keep_subagent_titles() {
    // Given
    let pool = Pool::must_new();
    let transcript = pool.root.join("s1.jsonl");
    std::fs::write(
        &transcript,
        "{\"type\":\"custom-title\",\"customTitle\":\"Older\"}\n",
    )
    .unwrap();
    let path = transcript.to_string_lossy().into_owned();

    // When
    pool.send(pool.event(
        "UserPromptSubmit",
        json!({"transcript_path": path, "session_title": "Newer"}),
    ));
    pool.send(pool.event(
        "PostToolUse",
        json!({"tool_name": "Agent", "tool_use_id": "c1",
               "tool_input": {"description": "Find the parser tests", "subagent_type": "Explore"},
               "tool_response": {"agentId": "a1", "isAsync": true}}),
    ));
    pool.send(pool.event(
        "PostToolUse",
        json!({"tool_name": "Agent", "tool_use_id": "c2",
               "tool_input": {"description": "\u{200b}", "subagent_type": "Explore"},
               "tool_response": {"agentId": "a2"}}),
    ));

    // Then
    let record = pool.record("s1").unwrap();
    assert_eq!(record["label"], "Newer");
    assert_eq!(
        record["subagents"],
        json!([{"agent_id": "a1", "title": "Find the parser tests"}])
    );
}
