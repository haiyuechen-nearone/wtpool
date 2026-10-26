use std::path::PathBuf;

use rstest::rstest;
use serde_json::{Value, json};
use wtpool_core::hook::{HookEnv, ProcessTable, handle};

use super::*;

const CLAUDE: &str =
    r#"{"pid":7001,"ppid":1,"started":"2026-10-20T11:00:00Z","name":"claude","cwd":"/"}"#;

/// A pool without git: `fg` with a `.git` folder holding `wtpool-activity/`, and the linked
/// worktree `wt1`, whose `.git` file points at `fg/.git/worktrees/wt1`.
fn must_pool() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let common = root.join("fg/.git");
    std::fs::create_dir_all(common.join("wtpool-activity")).unwrap();
    let gitdir = common.join("worktrees/wt1");
    std::fs::create_dir_all(&gitdir).unwrap();
    std::fs::create_dir_all(root.join("wt1")).unwrap();
    std::fs::write(
        root.join("wt1/.git"),
        format!("gitdir: {}\n", gitdir.display()),
    )
    .unwrap();
    std::fs::write(
        gitdir.join("gitdir"),
        format!("{}\n", root.join("wt1/.git").display()),
    )
    .unwrap();
    std::fs::write(gitdir.join("commondir"), "../..\n").unwrap();
    (dir, root)
}

/// A pool, a clock at noon, a process table where this process runs from claude, and the
/// adapter with the pool's root as `CLAUDE_CODE_TMPDIR`.
struct Pool {
    _dir: tempfile::TempDir,
    root: PathBuf,
    env: HookEnv,
    adapter: ClaudeCode,
}

impl Pool {
    fn must_new() -> Self {
        let (dir, root) = must_pool();
        let clock = root.join("clock");
        std::fs::write(&clock, "2026-10-20T12:00:00Z").unwrap();
        let procs = root.join("procs");
        let own = format!(
            r#"{{"pid":{},"ppid":7001,"started":"2026-10-20T12:00:00Z","name":"wtpool","cwd":"/"}}"#,
            std::process::id()
        );
        std::fs::write(&procs, format!("{CLAUDE}\n{own}\n")).unwrap();
        let env = HookEnv {
            clock: Some(clock),
            processes: ProcessTable::File(procs),
            sandbox: true,
            home: Some(root.clone()),
            tz: Some("UTC".to_owned()),
            entered_micros: 0,
            log: None,
        };
        let adapter = ClaudeCode {
            tmp: Some(root.clone()),
            ..ClaudeCode::default()
        };
        Self {
            _dir: dir,
            root,
            env,
            adapter,
        }
    }

    fn path(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }

    fn activity(&self) -> PathBuf {
        self.root.join("fg/.git/wtpool-activity")
    }

    fn send(&self, payload: Value) {
        handle(&self.adapter, payload.to_string().as_bytes(), &self.env);
    }

    fn event(&self, name: &str, fields: Value) -> Value {
        let mut payload =
            json!({"session_id": "s1", "hook_event_name": name, "cwd": self.path("fg")});
        for (key, value) in fields.as_object().unwrap() {
            payload[key] = value.clone();
        }
        payload
    }

    /// The record `stem` as written, `None` when there is none.
    fn record(&self, stem: &str) -> Option<Value> {
        let text = std::fs::read(self.activity().join(format!("{stem}.json"))).ok()?;
        Some(serde_json::from_slice(&text).unwrap())
    }
}

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
    assert!(
        pool.record("s1")
            .is_none_or(|session| session["tasks"] == json!([]))
    );
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
    // Given: a1 and a2 end their turns while b1 and b2 run, then a3 starts b3 ten minutes later.
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

    // When: the session's Stop lists only b2, then one from another process lists nothing.
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

#[test]
fn install__should_read_the_settings_in_the_config_folder() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("settings.json"), "{").unwrap();
    let adapter = ClaudeCode {
        config_dir: Some(dir.path().to_path_buf()),
        ..ClaudeCode::default()
    };

    // When
    let printed = adapter.install("/bin/wtpool");

    // Then
    let shown = dir.path().join("settings.json").display().to_string();
    assert!(
        printed
            .stderr
            .starts_with(&format!("hooks missing: {shown} is not valid JSON")),
        "{}",
        printed.stderr
    );
    assert_eq!(
        adapter
            .event_name(br#"{"session_id":"s1","hook_event_name":"Stop","cwd":"/p"}"#)
            .as_deref(),
        Some("Stop")
    );
}
