//! The background tasks a record keeps (H27): the output file of each Bash call that went to the
//! background, in Claude Code's task folder of its session, at most 5, until Claude Code's own
//! list of its session's work in flight says it ended. The hook never opens an output file.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use crate::support::hook::{HookRun, Hooks, RunOptions, event, with};
use crate::support::worktree;

/// The project folder of the transcripts these payloads name.
const PROJECT: &str = "-home-verify-code-app";

/// The hook's watchdog (H4): a hook that opened a FIFO would wait for it.
const WATCHDOG: Duration = Duration::from_millis(900);

struct Tasks {
    hooks: Hooks,
    /// `CLAUDE_CODE_TMPDIR`, as the session's Claude Code was told.
    tmp: PathBuf,
}

impl Tasks {
    fn must_new(name: &str) -> Self {
        let hooks = Hooks::must_new(name);
        let tmp = hooks.sandbox.root.join("claude tmp");
        fs::create_dir_all(&tmp).expect("setup: make CLAUDE_CODE_TMPDIR");
        Self { hooks, tmp }
    }

    /// The task folder of `session`, where Claude Code writes its outputs.
    fn folder(&self, base: &Path, session: &str) -> PathBuf {
        base.canonicalize()
            .expect("setup: resolve the temporary folder")
            .join(format!("claude-{}", owner(&self.hooks.activity())))
            .join(PROJECT)
            .join(session)
            .join("tasks")
    }

    fn send_in(&self, payload: &Value, tmp: Option<&Path>) -> HookRun {
        let options = RunOptions {
            envs: tmp
                .map(|tmp| ("CLAUDE_CODE_TMPDIR".to_owned(), OsString::from(tmp)))
                .into_iter()
                .collect(),
            ..RunOptions::default()
        };
        let run = self.hooks.run_with(payload, &options);
        assert_eq!(
            run.code(),
            Some(0),
            "wtpool hook for {} did not exit 0: {}",
            payload["hook_event_name"],
            run.describe()
        );
        run
    }

    fn send(&self, payload: &Value) -> HookRun {
        self.send_in(payload, Some(&self.tmp))
    }

    fn ev(&self, fixture: &str, session: &str, fields: &[(&str, Value)]) -> Value {
        let mut payload = with(
            event(&self.hooks.pool, fixture, session, &self.hooks.pool.fg),
            fields,
        );
        payload["transcript_path"] = json!(format!(
            "/home/verify/.claude/projects/{PROJECT}/{session}.jsonl"
        ));
        payload
    }

    /// A PostToolUse of `cd wt1 && cargo build --release` that went to the background as `id`.
    fn background(&self, session: &str, id: &Value) -> Value {
        let command = format!(
            "cd {} && cargo build --release",
            self.hooks.pool.worktree("wt1").display()
        );
        self.ev(
            "PostToolUse-Bash",
            session,
            &[
                (
                    "tool_use_id",
                    json!(format!("toolu_{session}_{}", call_id(id))),
                ),
                (
                    "tool_input",
                    json!({ "command": command, "run_in_background": true }),
                ),
                (
                    "tool_response",
                    json!({"stdout": "", "stderr": "", "interrupted": false, "isImage": false,
                           "noOutputExpected": false, "backgroundTaskId": id}),
                ),
            ],
        )
    }

    /// The tasks of the record `stem`: id, path and whether it ended.
    fn tasks(&self, stem: &str) -> Vec<(String, String, bool)> {
        let record = self.hooks.must_record(stem);
        record["tasks"]
            .as_array()
            .unwrap_or_else(|| panic!("record {stem} has no tasks list: {record}"))
            .iter()
            .map(|task| {
                assert_eq!(task["summary"], "cargo build", "{task}");
                assert!(task["started"].is_string(), "{task}");
                (
                    task["id"].as_str().unwrap_or_default().to_owned(),
                    task["path"].as_str().unwrap_or_default().to_owned(),
                    !task["ended"].is_null(),
                )
            })
            .collect()
    }

    fn ended(&self, stem: &str) -> Vec<(String, bool)> {
        self.tasks(stem)
            .into_iter()
            .map(|(id, _, ended)| (id, ended))
            .collect()
    }
}

/// The letters and digits of a task id, for a call id that keeps its pattern (H17).
fn call_id(id: &Value) -> String {
    let text = id.as_str().map_or_else(|| id.to_string(), str::to_owned);
    text.chars().filter(char::is_ascii_alphanumeric).collect()
}

/// The user that owns `path`: the uid Claude Code puts in its temporary folder's name.
#[cfg(unix)]
fn owner(path: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path)
        .expect("setup: stat wtpool-activity")
        .uid()
}

#[cfg(not(unix))]
fn owner(_: &Path) -> u32 {
    0
}

fn pairs(expected: &[(&str, bool)]) -> Vec<(String, bool)> {
    expected
        .iter()
        .map(|(id, ended)| ((*id).to_owned(), *ended))
        .collect()
}

#[test]
fn hook__should_record_background_tasks() {
    // Given
    let tasks = Tasks::must_new("hook-tasks");
    let folder = tasks.folder(&tasks.tmp, "s1");
    fs::create_dir_all(&folder).expect("setup: make the task folder");
    let fifo = folder.join("b1.output");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("setup: run mkfifo");
    assert!(made.success(), "setup: mkfifo failed");
    tasks.send(&tasks.ev("UserPromptSubmit", "s1", &[]));

    // When: six tasks start, the first one's output file a FIFO the hook must never open.
    let first = tasks.send(&tasks.background("s1", &json!("b1")));
    let one = tasks.tasks("s1");
    for id in ["b2", "b3", "b4", "b5", "b6"] {
        tasks.send(&tasks.background("s1", &json!(id)));
    }
    tasks.send(&tasks.background("s1", &json!("b6")));
    let six = tasks.tasks("s1");

    // Then
    assert!(
        first.elapsed < WATCHDOG,
        "wtpool hook took {:?} with a FIFO as the task's output file",
        first.elapsed
    );
    let path = |id: &str| {
        folder
            .join(format!("{id}.output"))
            .to_string_lossy()
            .into_owned()
    };
    assert_eq!(one, vec![("b1".to_owned(), path("b1"), false)]);
    let kept: Vec<(String, String, bool)> = ["b2", "b3", "b4", "b5", "b6"]
        .iter()
        .map(|id| ((*id).to_owned(), path(id), false))
        .collect();
    assert_eq!(six, kept, "the last 5 tasks, a known id added once");

    // When: calls that name no task in the session's folder.
    tasks.send(&tasks.background("s1", &json!("../../b7")));
    tasks.send(&tasks.background("s1", &json!("b/8")));
    tasks.send(&tasks.background("s1", &json!(9)));
    tasks.send(&with(
        tasks.background("s1", &json!("b10")),
        &[(
            "transcript_path",
            json!("/home/verify/.claude/projects/-other/s2.jsonl"),
        )],
    ));
    tasks.send(&with(
        tasks.background("s1", &json!("b11")),
        &[(
            "transcript_path",
            json!("/home/verify/.claude/projects/../s1.jsonl"),
        )],
    ));
    tasks.send(&with(
        tasks.background("s1", &json!("b12")),
        &[("transcript_path", json!("projects/-relative/s1.jsonl"))],
    ));
    tasks.send(&tasks.ev(
        "PostToolUse-Bash",
        "s1",
        &[("tool_use_id", json!("toolu_foreground"))],
    ));

    // Then
    assert_eq!(
        tasks.tasks("s1"),
        kept,
        "no task outside the session's task folder"
    );

    // When: the turn ends with b3 and b4 in flight, then a Stop without the list.
    tasks.hooks.advance(60);
    tasks.send(&tasks.ev(
        "Stop",
        "s1",
        &[(
            "background_tasks",
            json!([{"id": "b3", "type": "shell", "status": "running", "description": "Build"},
                   {"id": "b4", "type": "shell", "status": "running", "description": "Build"}]),
        )],
    ));
    let after_stop = tasks.ended("s1");
    tasks.send(&tasks.ev("Stop", "s1", &[]));
    let after_bare_stop = tasks.ended("s1");
    tasks.send(&tasks.ev(
        "PostToolUse-Bash",
        "s1",
        &[
            ("tool_name", json!("TaskStop")),
            ("tool_use_id", json!("toolu_stop")),
            ("tool_input", json!({ "task_id": "b3" })),
        ],
    ));
    let after_task_stop = tasks.ended("s1");
    tasks.send(&tasks.ev("SessionEnd", "s1", &[]));

    // Then
    let stopped = pairs(&[
        ("b2", true),
        ("b3", false),
        ("b4", false),
        ("b5", true),
        ("b6", true),
    ]);
    assert_eq!(
        after_stop, stopped,
        "a Stop ends the tasks it does not list"
    );
    assert_eq!(
        after_bare_stop, stopped,
        "a Stop without the list ends nothing"
    );
    assert_eq!(
        after_task_stop,
        pairs(&[
            ("b2", true),
            ("b3", true),
            ("b4", false),
            ("b5", true),
            ("b6", true)
        ]),
        "TaskStop ends its task"
    );
    let record = tasks.hooks.must_record("s1");
    let ended_at: Vec<&Value> = record["tasks"]
        .as_array()
        .map(|list| list.iter().map(|task| &task["ended"]).collect())
        .unwrap_or_default();
    assert!(
        ended_at.iter().all(|time| time.is_string()),
        "SessionEnd ends every task: {record}"
    );
    assert!(
        fs::symlink_metadata(&fifo).is_ok_and(|meta| !meta.is_file()),
        "the FIFO is still there, never replaced"
    );
    subagent_task_under_tmp();
}

/// Without `CLAUDE_CODE_TMPDIR` the task folder is under `/tmp`, and a subagent's task stays on
/// its own record until its SubagentStop's list leaves it out.
fn subagent_task_under_tmp() {
    // Given
    let tasks = Tasks::must_new("hook-tasks-tmp");
    let folder = tasks.folder(Path::new("/tmp"), "s1");
    let mut start = tasks.background("s1", &json!("b1"));
    start["agent_id"] = json!("a1");
    start["agent_type"] = json!("Explore");

    // When
    tasks.send_in(&start, None);
    let started = tasks.tasks("s1.a1");
    let mut stop = tasks.ev("SubagentStop", "s1", &[("background_tasks", json!([]))]);
    stop["agent_id"] = json!("a1");
    tasks.send_in(&stop, None);

    // Then
    let path = folder.join("b1.output").to_string_lossy().into_owned();
    assert_eq!(started, vec![("b1".to_owned(), path, false)]);
    assert_eq!(tasks.ended("s1.a1"), pairs(&[("b1", true)]));
    assert!(
        tasks.hooks.record("s1").is_none(),
        "the subagent's task stays on its own record"
    );
}

/// An entry of Claude Code 2.1.310's `background_tasks`: a shell task or a subagent.
fn entry(id: &str, kind: &str, status: &str) -> Value {
    match kind {
        "subagent" => json!({"id": id, "type": "subagent", "status": status,
                             "description": "Verify", "agent_type": "general-purpose"}),
        _ => json!({"id": id, "type": "shell", "status": status, "description": "Build",
                    "command": "cargo build --release"}),
    }
}

/// The fields Claude Code 2.1.310 adds to every Stop and SubagentStop, with `listed`.
fn stop_fields(listed: &[Value]) -> Vec<(&'static str, Value)> {
    vec![
        ("background_tasks", Value::from(listed.to_vec())),
        ("session_crons", json!([])),
        ("effort", json!({"level": "medium"})),
        ("prompt_id", json!("5cfd4e37-3b3a-4fa4-b620-c759a5fab371")),
        ("last_assistant_message", json!("Report delivered.")),
    ]
}

fn as_agent(mut payload: Value, agent: &str) -> Value {
    payload["agent_id"] = json!(agent);
    payload["agent_type"] = json!("general-purpose");
    payload
}

/// The actor `key` in worktree wt1 of `document`.
fn actor_in_wt1(document: &Value, key: &str) -> Value {
    worktree(document, "wt1")["actors"]
        .as_array()
        .and_then(|actors| actors.iter().find(|actor| actor["key"] == key))
        .cloned()
        .unwrap_or(Value::Null)
}

/// Claude Code 2.1.310 lists the work in flight of the whole session on every Stop and
/// SubagentStop, and leaves a task out once it ends: a subagent whose last SubagentStop still
/// listed its build ends that task at the session's next Stop.
#[test]
fn hook__should_end_a_task_through_its_session() {
    // Given: the session runs b0; subagent a1 ends its turn while its build b1 runs, and a2
    // while b1 and its own b2 run.
    let tasks = Tasks::must_new("hook-tasks-session");
    tasks.send(&tasks.ev("UserPromptSubmit", "s1", &[]));
    tasks.send(&tasks.background("s1", &json!("b0")));
    for (agent, id) in [("a1", "b1"), ("a2", "b2")] {
        tasks.send(&as_agent(tasks.ev("SubagentStart", "s1", &[]), agent));
        tasks.send(&as_agent(tasks.background("s1", &json!(id)), agent));
    }
    let a1_stop = stop_fields(&[
        entry("a1", "subagent", "running"),
        entry("b0", "shell", "running"),
        entry("b1", "shell", "running"),
    ]);
    tasks.send(&as_agent(tasks.ev("SubagentStop", "s1", &a1_stop), "a1"));
    let a2_stop = stop_fields(&[
        entry("a2", "subagent", "running"),
        entry("b0", "shell", "running"),
        entry("b1", "shell", "running"),
        entry("b2", "shell", "running"),
    ]);
    tasks.send(&as_agent(tasks.ev("SubagentStop", "s1", &a2_stop), "a2"));
    let later = tasks.hooks.advance(600);
    let stuck = tasks.hooks.status();

    // When: b1 has ended, and the session's Stop lists b0 as completed and b2 as running.
    let stop = stop_fields(&[
        entry("a2", "subagent", "running"),
        entry("b0", "shell", "completed"),
        entry("b2", "shell", "running"),
    ]);
    tasks.send(&tasks.ev("Stop", "s1", &stop));
    let cleared = tasks.hooks.status();

    // Then
    let a1 = actor_in_wt1(&stuck, "s1.a1");
    assert_eq!(a1["state"], json!("working"), "{stuck}");
    assert_eq!(
        a1["action"],
        json!({"tool": "background:", "target": "cargo build"}),
        "{stuck}"
    );
    assert_eq!(
        tasks.ended("s1"),
        pairs(&[("b0", true)]),
        "listed as completed"
    );
    assert_eq!(
        tasks.ended("s1.a1"),
        pairs(&[("b1", true)]),
        "left out of the list"
    );
    assert_eq!(
        tasks.ended("s1.a2"),
        pairs(&[("b2", false)]),
        "still listed"
    );
    let record = tasks.hooks.must_record("s1.a1");
    assert_eq!(record["tasks"][0]["ended"], json!(later), "{record}");
    assert_eq!(record["state"], json!("done"), "{record}");
    assert_eq!(
        actor_in_wt1(&cleared, "s1.a1"),
        Value::Null,
        "a done subagent past its minute: {cleared}"
    );
    assert_eq!(
        actor_in_wt1(&cleared, "s1.a2")["action"],
        json!({"tool": "background:", "target": "cargo build"}),
        "{cleared}"
    );
}

/// The session's turn end changes another record only under that record's lock: one whose lock
/// path holds a folder, which no lock can be taken on, keeps its running task.
#[test]
fn hook__should_end_no_task_of_a_record_it_cannot_lock() {
    // Given
    let tasks = Tasks::must_new("hook-tasks-unlockable");
    tasks.send(&tasks.ev("UserPromptSubmit", "s1", &[]));
    for (agent, id) in [("a1", "b1"), ("a2", "b2")] {
        tasks.send(&as_agent(tasks.ev("SubagentStart", "s1", &[]), agent));
        tasks.send(&as_agent(tasks.background("s1", &json!(id)), agent));
    }
    let lock = tasks.hooks.activity().join("s1.a1.json.lock");
    let _ = fs::remove_file(&lock);
    fs::create_dir(&lock).expect("setup: a folder at the lock path");
    tasks.hooks.advance(600);

    // When
    tasks.send(&tasks.ev("Stop", "s1", &stop_fields(&[])));

    // Then
    assert_eq!(
        tasks.ended("s1.a1"),
        pairs(&[("b1", false)]),
        "a record whose lock cannot be taken"
    );
    assert_eq!(
        tasks.ended("s1.a2"),
        pairs(&[("b2", true)]),
        "left out of the list"
    );
}
