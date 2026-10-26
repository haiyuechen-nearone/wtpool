//! The outputs an actor lists in `wtpool status` (S13): its running background tasks, newest first,
//! then the one that ended last, omitted when it has none; never a path that is not absolute.

use std::ffi::OsString;
use std::fs;

use serde_json::{Value, json};

use crate::support::hook::{Hooks, RunOptions, event, with};
use crate::support::{schema_errors, worktree};

const PROJECT: &str = "-home-verify-code-app";

fn ev(hooks: &Hooks, fixture: &str, fields: &[(&str, Value)]) -> Value {
    let mut payload = with(event(&hooks.pool, fixture, "s1", &hooks.pool.fg), fields);
    payload["transcript_path"] = json!(format!("/home/verify/.claude/projects/{PROJECT}/s1.jsonl"));
    payload
}

/// A PostToolUse of `cd wt1 && cargo build` that went to the background as `id`.
fn background(hooks: &Hooks, id: &str) -> Value {
    let command = format!("cd {} && cargo build", hooks.pool.worktree("wt1").display());
    ev(
        hooks,
        "PostToolUse-Bash",
        &[
            ("tool_use_id", json!(format!("toolu_{id}"))),
            (
                "tool_input",
                json!({ "command": command, "run_in_background": true }),
            ),
            (
                "tool_response",
                json!({"stdout": "", "stderr": "", "interrupted": false, "backgroundTaskId": id}),
            ),
        ],
    )
}

/// The outputs of the actor `key` in worktree `name`: task, running, and whether it has a path
/// and times.
fn outputs(document: &Value, name: &str, key: &str) -> Option<Vec<(String, bool)>> {
    let actors = worktree(document, name)["actors"].as_array()?.clone();
    let actor = actors.into_iter().find(|actor| actor["key"] == key)?;
    let listed = actor.get("outputs")?.as_array()?.clone();
    Some(
        listed
            .iter()
            .map(|output| {
                assert_eq!(output["summary"], "cargo build", "{output}");
                assert!(
                    output["path"].as_str().is_some_and(|p| p.starts_with('/')),
                    "{output}"
                );
                assert!(output["started"].is_string(), "{output}");
                let running = output["running"].as_bool().unwrap_or_default();
                assert_eq!(output["ended"].is_null(), running, "{output}");
                (
                    output["task"].as_str().unwrap_or_default().to_owned(),
                    running,
                )
            })
            .collect(),
    )
}

fn pairs(expected: &[(&str, bool)]) -> Vec<(String, bool)> {
    expected
        .iter()
        .map(|(id, running)| ((*id).to_owned(), *running))
        .collect()
}

#[test]
fn status__should_list_outputs() {
    // Given
    let hooks = Hooks::must_new("status-outputs");
    let tmp = hooks.sandbox.root.join("claude-tmp");
    fs::create_dir_all(&tmp).expect("setup: make CLAUDE_CODE_TMPDIR");
    let options = RunOptions {
        envs: vec![("CLAUDE_CODE_TMPDIR".to_owned(), OsString::from(&tmp))],
        ..RunOptions::default()
    };
    let send = |payload: &Value| {
        let run = hooks.run_with(payload, &options);
        assert_eq!(run.code(), Some(0), "wtpool hook: {}", run.describe());
    };
    send(&ev(&hooks, "UserPromptSubmit", &[]));
    let mut read = ev(
        &hooks,
        "PreToolUse-Read",
        &[(
            "tool_input",
            json!({ "file_path": hooks.pool.worktree("wt2").join("f.txt") }),
        )],
    );
    read["agent_id"] = json!("a1");
    read["agent_type"] = json!("Explore");
    send(&read);
    fs::write(
        hooks.activity().join("s9.json"),
        json!({
            "version": 1, "session_id": "s9", "worktree": hooks.pool.worktree("wt3"),
            "state": "idle", "since": "2026-10-20T12:00:00Z", "updated": "2026-10-20T12:00:00Z",
            "tasks": [
                {"id": "b1", "path": "relative/b1.output", "summary": "cargo build",
                 "started": "2026-10-20T12:00:00Z", "ended": null},
                {"id": "b2", "path": "/t/../etc/passwd", "summary": "cargo build",
                 "started": "2026-10-20T12:00:00Z", "ended": null},
                {"id": "b3", "path": "/t/claude-501/p/s9/tasks/b3.output", "summary": "cargo build",
                 "started": "2026-10-20T12:00:00Z", "ended": null},
            ],
        })
        .to_string(),
    )
    .expect("setup: write a record by hand");

    // When
    send(&background(&hooks, "b1"));
    hooks.advance(10);
    send(&background(&hooks, "b2"));
    let both = hooks.status();
    hooks.advance(10);
    send(&ev(
        &hooks,
        "Stop",
        &[(
            "background_tasks",
            json!([{"id": "b2", "type": "shell", "status": "running"}]),
        )],
    ));
    let one_ended = hooks.status();

    // Then
    for document in [&both, &one_ended] {
        let errors = schema_errors(document);
        assert!(
            errors.is_empty(),
            "the document fails verify/schema: {errors:?}"
        );
    }
    assert_eq!(
        outputs(&both, "wt1", "s1"),
        Some(pairs(&[("b2", true), ("b1", true)])),
        "running tasks, newest first: {both}"
    );
    assert_eq!(
        outputs(&one_ended, "wt1", "s1"),
        Some(pairs(&[("b2", true), ("b1", false)])),
        "then the one that ended last: {one_ended}"
    );
    assert_eq!(
        outputs(&one_ended, "wt2", "s1.a1"),
        None,
        "an actor without tasks has no outputs: {one_ended}"
    );
    assert_eq!(
        outputs(&one_ended, "wt3", "s9"),
        Some(pairs(&[("b3", true)])),
        "paths that are not absolute and plain are skipped: {one_ended}"
    );
    let first = &worktree(&one_ended, "wt1")["actors"][0]["outputs"][0];
    let tmp = tmp
        .canonicalize()
        .expect("setup: resolve CLAUDE_CODE_TMPDIR");
    assert!(
        first["path"]
            .as_str()
            .is_some_and(|path| path.starts_with(&*tmp.to_string_lossy())
                && path.ends_with(&format!("/{PROJECT}/s1/tasks/b2.output"))),
        "{first}"
    );
}
