//! The background tasks a session and its subagents start, and when they end.

use super::*;

/// A PostToolUse of a Bash call that went to the background as task `id`, in session `session`.
fn background_call(pool: &Pool, session: &str, agent: Option<&str>, id: &str) -> Value {
    let mut payload = pool.event(
        "PostToolUse",
        json!({"session_id": session, "tool_name": "Bash", "tool_use_id": format!("c{id}"),
               "transcript_path": format!("/home/me/.claude/projects/-acme-app/{session}.jsonl"),
               "tool_input": {"command": format!("cd {} && cargo build --release", pool.path("wt1"))},
               "tool_response": {"stdout": "", "stderr": "", "backgroundTaskId": id}}),
    );
    if let Some(agent) = agent {
        payload["agent_id"] = json!(agent);
    }
    payload
}

/// The user who owns `folder`, 0 without Unix owners.
#[cfg(unix)]
fn owner(folder: &std::path::Path) -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(folder).unwrap().uid()
}

#[cfg(not(unix))]
fn owner(_: &std::path::Path) -> u32 {
    0
}

/// The id and end of each task of the record `stem`.
fn task_ends(pool: &Pool, stem: &str) -> Vec<(String, Option<String>)> {
    pool.record(stem).unwrap()["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| {
            let ended = task["ended"].as_str().map(str::to_owned);
            (task["id"].as_str().unwrap().to_owned(), ended)
        })
        .collect()
}

#[test]
fn handle__should_record_background_tasks_until_they_end() {
    // Given
    let pool = Pool::must_new();
    let uid = owner(&pool.activity());
    let folder = pool
        .root
        .canonicalize()
        .unwrap()
        .join(format!("claude-{uid}/-acme-app/s1/tasks"));
    pool.send(pool.event("UserPromptSubmit", json!({})));

    // When
    pool.send(background_call(&pool, "s1", None, "b1"));
    pool.send(background_call(&pool, "s1", None, "b2"));
    pool.send(background_call(&pool, "s1", Some("a1"), "b3"));
    let started = pool.record("s1").unwrap();
    pool.send(pool.event(
        "Stop",
        json!({"background_tasks": [{"id": "b2", "type": "shell"}]}),
    ));
    let stopped = task_ends(&pool, "s1");
    pool.send(pool.event("SessionEnd", json!({})));

    // Then
    let tasks = started["tasks"].as_array().unwrap();
    let paths: Vec<&str> = tasks
        .iter()
        .map(|task| task["path"].as_str().unwrap())
        .collect();
    let expected: Vec<String> = ["b1", "b2"]
        .iter()
        .map(|id| {
            folder
                .join(format!("{id}.output"))
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(paths, expected);
    assert_eq!(tasks[0]["summary"], "cargo build");
    assert_eq!(tasks[0]["started"], "2026-10-20T12:00:00Z");
    assert!(tasks.iter().all(|task| task["ended"].is_null()));
    let ended: Vec<bool> = stopped.iter().map(|(_, ended)| ended.is_some()).collect();
    assert_eq!(ended, vec![true, false]);
    let subagent = pool.record("s1.a1");
    assert!(subagent.is_none(), "SessionEnd deletes the subagent record");
    let last = task_ends(&pool, "s1");
    assert!(last.iter().all(|(_, ended)| ended.is_some()));
}

#[test]
fn handle__should_keep_a_subagents_task_on_its_record() {
    // Given
    let pool = Pool::must_new();
    pool.send(pool.event("SubagentStart", json!({"agent_id": "a1"})));

    // When
    pool.send(background_call(&pool, "s1", Some("a1"), "b3"));
    let running = pool.record("s1.a1").unwrap();
    pool.send(pool.event(
        "PostToolUse",
        json!({"agent_id": "a1", "tool_name": "TaskStop", "tool_use_id": "c9",
               "tool_input": {"task_id": "b3"}}),
    ));

    // Then
    let tasks = running["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    assert!(
        tasks[0]["path"]
            .as_str()
            .unwrap()
            .ends_with("/-acme-app/s1/tasks/b3.output")
    );
    assert!(tasks[0]["ended"].is_null());
    assert!(pool.record("s1").is_none());
    assert_eq!(
        task_ends(&pool, "s1.a1"),
        vec![("b3".to_owned(), Some("2026-10-20T12:00:00Z".to_owned()))]
    );
}

#[test]
fn handle__should_record_no_task_outside_the_task_folder() {
    // Given
    let pool = Pool::must_new();
    let mut call = background_call(&pool, "s1", None, "b1");
    call["transcript_path"] = json!("/home/me/.claude/projects/../s1.jsonl");
    let mut missing = background_call(&pool, "s1", None, "b2");
    missing["tool_response"]["backgroundTaskId"] = json!("../../etc/passwd");

    // When
    pool.send(call);
    pool.send(missing);

    // Then
    let record = pool.record("s1").unwrap();
    assert_eq!(record["tasks"], json!([]));
    assert_eq!(record["done_ids"], json!(["cb1", "cb2"]));
}

/// Runs the hooks of `pool` from the process `ppid`, listed as a claude started at 11:00.
fn hook_parent(pool: &Pool, ppid: u32) {
    let own = format!(
        r#"{{"pid":{},"ppid":{ppid},"started":"2026-10-20T12:00:00Z","name":"wtpool","cwd":"/"}}"#,
        std::process::id()
    );
    let other =
        r#"{"pid":7002,"ppid":1,"started":"2026-10-20T11:00:00Z","name":"claude","cwd":"/"}"#;
    std::fs::write(
        pool.root.join("procs"),
        format!("{CLAUDE}\n{other}\n{own}\n"),
    )
    .unwrap();
}

#[test]
fn handle__should_end_old_tasks_the_sessions_list_leaves_out() {
    // Given
    let pool = Pool::must_new();
    let listed = |ids: &[&str]| -> Value {
        ids.iter()
            .map(|id| json!({"id": id, "type": "shell", "status": "running"}))
            .collect()
    };
    for (agent, id, list) in [("a1", "b1", vec!["b1"]), ("a2", "b2", vec!["b1", "b2"])] {
        pool.send(pool.event("SubagentStart", json!({"agent_id": agent})));
        pool.send(background_call(&pool, "s1", Some(agent), id));
        pool.send(pool.event(
            "SubagentStop",
            json!({"agent_id": agent, "background_tasks": listed(&list)}),
        ));
    }
    std::fs::write(pool.root.join("clock"), "2026-10-20T12:10:00Z").unwrap();
    pool.send(pool.event("SubagentStart", json!({"agent_id": "a3"})));
    pool.send(background_call(&pool, "s1", Some("a3"), "b3"));
    let a2_before = pool.record("s1.a2").unwrap();

    // When
    pool.send(pool.event("Stop", json!({"background_tasks": listed(&["b2"])})));
    let first = (
        task_ends(&pool, "s1.a1"),
        task_ends(&pool, "s1.a2"),
        task_ends(&pool, "s1.a3"),
    );
    hook_parent(&pool, 7002);
    std::fs::write(pool.root.join("clock"), "2026-10-20T12:20:00Z").unwrap();
    pool.send(pool.event("Stop", json!({"background_tasks": []})));
    let elsewhere = (task_ends(&pool, "s1.a2"), task_ends(&pool, "s1.a3"));
    hook_parent(&pool, 7001);
    pool.send(pool.event("Stop", json!({})));
    let without_list = task_ends(&pool, "s1.a2");
    pool.send(pool.event(
        "SubagentStop",
        json!({"agent_id": "a3", "background_tasks": listed(&["a3"])}),
    ));

    // Then
    let ended = |id: &str, at: Option<&str>| vec![(id.to_owned(), at.map(str::to_owned))];
    assert_eq!(
        first.0,
        ended("b1", Some("2026-10-20T12:10:00Z")),
        "old and unlisted"
    );
    assert_eq!(first.1, ended("b2", None), "listed");
    assert_eq!(first.2, ended("b3", None), "started under a minute before");
    assert_eq!(elsewhere.0, ended("b2", None), "a list of another process");
    assert_eq!(elsewhere.1, ended("b3", None), "a list of another process");
    assert_eq!(without_list, ended("b2", None), "no list ends nothing");
    assert_eq!(
        task_ends(&pool, "s1.a2"),
        ended("b2", Some("2026-10-20T12:20:00Z")),
        "a3's SubagentStop lists the session's work, which b2 left"
    );
    assert_eq!(
        task_ends(&pool, "s1.a3"),
        ended("b3", Some("2026-10-20T12:20:00Z"))
    );
    let mut a2 = pool.record("s1.a2").unwrap();
    a2["tasks"] = a2_before["tasks"].clone();
    assert_eq!(a2, a2_before, "only the task changes");
}
