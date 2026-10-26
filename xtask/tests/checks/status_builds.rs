//! Work an actor started in `wtpool status`: a build below its session's harness process, or a
//! background task of its own, keeps it working after its turn ends, until that work ends (A1).

use serde_json::{Value, json};

use crate::support::hook::{CLAUDE, CLAUDE_STARTED, Hooks, event, with};
use crate::support::worktree;

fn ev(hooks: &Hooks, fixture: &str, fields: &[(&str, Value)]) -> Value {
    with(event(&hooks.pool, fixture, "s1", &hooks.pool.fg), fields)
}

/// The actor `key` of the worktree `name`.
fn actor(document: &Value, name: &str, key: &str) -> Value {
    worktree(document, name)["actors"]
        .as_array()
        .and_then(|actors| actors.iter().find(|actor| actor["key"] == key))
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn builds__should_keep_the_session_that_started_them_working() {
    // Given: s1 starts a build in wt1 in the background and ends its turn.
    let mut hooks = Hooks::must_new("own-builds");
    let wt1 = hooks.pool.worktree("wt1");
    let wt2 = hooks.pool.worktree("wt2");
    let command = json!({
        "command": format!("cd {} && cargo nextest run", wt1.display()),
        "description": "Run the tests",
        "run_in_background": true,
    });
    let call = [
        ("tool_input", command),
        ("tool_use_id", json!("toolu_build")),
    ];
    hooks.send(&ev(&hooks, "UserPromptSubmit", &[]));
    hooks.send(&ev(&hooks, "PreToolUse-Bash", &call));
    hooks.send(&ev(&hooks, "PostToolUse-Bash", &call));
    hooks.send(&ev(&hooks, "Stop", &[]));
    hooks.advance(600);
    let fg = hooks.pool.fg.clone();
    // The session's shell and its build, an MCP server of the session, and a build of yours.
    hooks.start_child(7100, CLAUDE, "zsh", &wt1, CLAUDE_STARTED);
    hooks.start_child(7101, 7100, "cargo-nextest", &wt1, CLAUDE_STARTED);
    hooks.start_child(7102, 7101, "cargo", &wt1, CLAUDE_STARTED);
    hooks.start_child(7103, 7102, "rustc", &wt1, CLAUDE_STARTED);
    hooks.start_child(7200, CLAUDE, "node", &fg, CLAUDE_STARTED);
    hooks.start_child(7300, 1, "cargo", &wt2, CLAUDE_STARTED);

    // When
    let building = hooks.status();
    for pid in [7103, 7102, 7101, 7100] {
        hooks.stop(pid);
    }
    let built = hooks.status();

    // Then
    let session = actor(&building, "wt1", "s1");
    assert_eq!(session["state"], json!("working"), "{building}");
    assert_eq!(
        session["action"],
        json!({"tool": "build:", "target": "cargo-nextest"}),
        "{building}"
    );
    assert_eq!(worktree(&building, "wt1")["activity"], json!("working"));
    assert_eq!(worktree(&building, "wt2")["activity"], json!("building"));
    let after = actor(&built, "wt1", "s1");
    assert_eq!(after["state"], json!("idle"), "{built}");
    assert_eq!(worktree(&built, "wt1")["activity"], json!("idle"));
}

#[test]
fn builds__should_keep_no_session_working_through_other_children() {
    // Given: only an MCP server and a build of another process run.
    let mut hooks = Hooks::must_new("other-children");
    let fg = hooks.pool.fg.clone();
    let wt2 = hooks.pool.worktree("wt2");
    hooks.send(&ev(&hooks, "UserPromptSubmit", &[]));
    hooks.send(&ev(&hooks, "Stop", &[]));
    hooks.start_child(7200, CLAUDE, "node", &fg, CLAUDE_STARTED);
    hooks.start_child(7201, 7200, "python3", &fg, CLAUDE_STARTED);
    hooks.start_child(7300, 1, "zsh", &wt2, CLAUDE_STARTED);
    hooks.start_child(7301, 7300, "cargo", &wt2, CLAUDE_STARTED);

    // When
    let document = hooks.status();

    // Then
    let session = actor(&document, "fg", "s1");
    assert_eq!(session["state"], json!("idle"), "{document}");
    assert_eq!(worktree(&document, "fg")["activity"], json!("idle"));
    assert_eq!(worktree(&document, "wt2")["activity"], json!("building"));
}

/// A PostToolUse of `cd <worktree> && cargo test` that went to the background as task `id`.
fn background_test(hooks: &Hooks, worktree: &str, id: &str) -> Value {
    let command = format!(
        "cd {} && cargo test",
        hooks.pool.worktree(worktree).display()
    );
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

/// The `background_tasks` of a Stop or SubagentStop that lists `ids` as running.
fn still_running(ids: &[&str]) -> Vec<(&'static str, Value)> {
    let tasks: Vec<Value> = ids
        .iter()
        .map(|id| json!({"id": id, "type": "shell", "status": "running"}))
        .collect();
    vec![("background_tasks", Value::from(tasks))]
}

fn as_subagent(mut payload: Value) -> Value {
    payload["agent_id"] = json!("a1");
    payload["agent_type"] = json!("general-purpose");
    payload
}

#[test]
fn background__should_keep_a_session_working_until_its_task_ends() {
    // Given: s1 runs `cargo test` in wt1 in the background and ends its turn while it runs.
    let hooks = Hooks::must_new("background-session");
    hooks.send(&ev(&hooks, "UserPromptSubmit", &[]));
    hooks.send(&background_test(&hooks, "wt1", "b1"));
    hooks.send(&ev(&hooks, "Stop", &still_running(&["b1"])));
    hooks.advance(600);

    // When
    let waiting = hooks.status();
    hooks.send(&ev(&hooks, "Stop", &still_running(&[])));
    let ended = hooks.status();

    // Then
    let session = actor(&waiting, "wt1", "s1");
    assert_eq!(session["state"], json!("working"), "{waiting}");
    assert_eq!(
        session["action"],
        json!({"tool": "background:", "target": "cargo test"}),
        "{waiting}"
    );
    assert_eq!(worktree(&waiting, "wt1")["activity"], json!("working"));
    let after = actor(&ended, "wt1", "s1");
    assert_eq!(after["state"], json!("idle"), "{ended}");
    assert_eq!(worktree(&ended, "wt1")["activity"], json!("idle"));
}

#[test]
fn background__should_keep_a_subagent_working_until_its_task_ends() {
    // Given: subagent a1 runs `cargo test` in wt2 in the background and stops while it runs,
    // and its session ends its turn.
    let hooks = Hooks::must_new("background-subagent");
    hooks.send(&ev(&hooks, "UserPromptSubmit", &[]));
    hooks.send(&ev(&hooks, "PreToolUse-Agent", &[]));
    hooks.send(&as_subagent(ev(&hooks, "SubagentStart", &[])));
    hooks.send(&as_subagent(background_test(&hooks, "wt2", "b2")));
    hooks.send(&as_subagent(ev(
        &hooks,
        "SubagentStop",
        &still_running(&["b2"]),
    )));
    hooks.send(&ev(&hooks, "PostToolUse-Agent", &[]));
    hooks.send(&ev(&hooks, "Stop", &still_running(&[])));
    hooks.advance(600);

    // When
    let waiting = hooks.status();
    hooks.send(&as_subagent(ev(
        &hooks,
        "SubagentStop",
        &still_running(&[]),
    )));
    let ended = hooks.status();
    hooks.advance(61);
    let gone = hooks.status();

    // Then
    let subagent = actor(&waiting, "wt2", "s1.a1");
    assert_eq!(subagent["state"], json!("working"), "{waiting}");
    assert_eq!(
        subagent["action"],
        json!({"tool": "background:", "target": "cargo test"}),
        "{waiting}"
    );
    assert_eq!(worktree(&waiting, "wt2")["activity"], json!("working"));
    assert_eq!(
        actor(&ended, "wt2", "s1.a1")["state"],
        json!("done"),
        "{ended}"
    );
    assert_eq!(worktree(&ended, "wt2")["activity"], json!("free"));
    assert_eq!(actor(&gone, "wt2", "s1.a1"), Value::Null, "{gone}");
}
