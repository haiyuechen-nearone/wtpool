//! `wtpool hook gc`, the gc the hook runs itself once a day, and the records a week leaves.

use std::collections::BTreeSet;
use std::fs;
use std::process::{Child, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

use crate::support::hook::{Hooks, RunOptions, add_seconds, event, wait_output, with};

const NOW: &str = "2026-10-20T12:00:00Z";
const DAY: i64 = 86_400;

fn ev(hooks: &Hooks, fixture: &str, session: &str, fields: &[(&str, Value)]) -> Value {
    with(event(&hooks.pool, fixture, session, &hooks.pool.fg), fields)
}

fn sub_ev(hooks: &Hooks, fixture: &str, agent: &str, fields: &[(&str, Value)]) -> Value {
    let mut payload = ev(hooks, fixture, "s1", fields);
    payload["agent_id"] = json!(agent);
    payload["agent_type"] = json!("Explore");
    payload
}

fn send_from(hooks: &Hooks, payload: &Value, parent: u32) {
    let options = RunOptions {
        parent: Some(parent),
        ..RunOptions::default()
    };
    let run = hooks.run_with(payload, &options);
    assert_eq!(
        run.code(),
        Some(0),
        "wtpool hook for {} did not exit 0: {}",
        payload["hook_event_name"],
        run.describe()
    );
}

fn must_gc(hooks: &Hooks) {
    let output = hooks.gc();
    assert!(
        output.status.success(),
        "wtpool hook gc exited {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn records(hooks: &Hooks) -> BTreeSet<String> {
    hooks
        .files()
        .into_iter()
        .filter_map(|name| name.strip_suffix(".json").map(str::to_owned))
        .collect()
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

/// Ended, dead and live sessions, 8 and 6 days old.
fn aged_sessions(hooks: &Hooks) {
    let dead = 7098;
    hooks.set_clock(&add_seconds(NOW, -8 * DAY));
    hooks.send(&ev(hooks, "UserPromptSubmit", "ended-8d", &[]));
    hooks.send(&ev(hooks, "SessionEnd", "ended-8d", &[]));
    send_from(hooks, &ev(hooks, "UserPromptSubmit", "dead-8d", &[]), dead);
    hooks.send(&ev(hooks, "UserPromptSubmit", "live-8d", &[]));
    hooks.send(&ev(hooks, "Stop", "live-8d", &[]));
    hooks.send(&ev(hooks, "UserPromptSubmit", "working-8d", &[]));
    hooks.set_clock(&add_seconds(NOW, -6 * DAY));
    hooks.send(&ev(hooks, "UserPromptSubmit", "ended-6d", &[]));
    hooks.send(&ev(hooks, "SessionEnd", "ended-6d", &[]));
    send_from(hooks, &ev(hooks, "UserPromptSubmit", "dead-6d", &[]), dead);
    hooks.set_clock(NOW);
}

#[test]
fn gc__should_delete_only_old_dead_or_ended_records() {
    // Given
    let hooks = Hooks::must_new("gc-old");
    aged_sessions(&hooks);
    let before = records(&hooks);

    // When
    must_gc(&hooks);
    let after = records(&hooks);

    // Then
    assert_eq!(
        before,
        set(&[
            "dead-6d",
            "dead-8d",
            "ended-6d",
            "ended-8d",
            "live-8d",
            "working-8d"
        ]),
        "the hooks wrote other records"
    );
    assert_eq!(
        after,
        set(&["dead-6d", "ended-6d", "live-8d", "working-8d"]),
        "records after gc: ended or dead ones go after 7 days, live ones stay"
    );
}

#[test]
fn gc__should_delete_done_subagents_but_the_newest() {
    // Given
    let hooks = Hooks::must_new("gc-subagents");
    hooks.set_clock(&add_seconds(NOW, -3600));
    hooks.send(&ev(&hooks, "UserPromptSubmit", "s1", &[]));
    // (agent, worktree, minutes before NOW it ended, or None while it works)
    let agents: [(&str, &str, Option<i64>); 7] = [
        ("a1", "wt1", Some(30)),
        ("a2", "wt1", Some(20)),
        ("a3", "wt3", Some(16)),
        ("a4", "wt3", Some(10)),
        ("a5", "wt2", Some(30)),
        ("a6", "wt1", None),
        ("a7", "wt2", Some(40)),
    ];
    for (agent, worktree, ended) in agents {
        hooks.set_clock(&add_seconds(NOW, -50 * 60));
        hooks.send(&sub_ev(&hooks, "SubagentStart", agent, &[]));
        let file = hooks.pool.worktree(worktree).join("f.txt");
        hooks.send(&sub_ev(
            &hooks,
            "PreToolUse-Read-subagent",
            agent,
            &[
                ("tool_input", json!({ "file_path": file })),
                ("tool_use_id", json!(format!("toolu_{agent}"))),
            ],
        ));
        if let Some(minutes) = ended {
            hooks.set_clock(&add_seconds(NOW, -minutes * 60));
            hooks.send(&sub_ev(&hooks, "SubagentStop", agent, &[]));
        }
    }
    hooks.set_clock(NOW);

    // When
    must_gc(&hooks);
    let after = records(&hooks);

    // Then
    assert_eq!(
        after,
        set(&["s1", "s1.a2", "s1.a4", "s1.a5", "s1.a6"]),
        "records after gc: done subagents over 15 minutes old go, but the newest done one of \
         each worktree stays (wt1: a2, wt2: a5, wt3: a4)"
    );
}

#[test]
fn gc__should_delete_the_lock_with_the_record() {
    // Given
    let hooks = Hooks::must_new("gc-lock");
    aged_sessions(&hooks);
    let before = hooks.files();

    // When
    must_gc(&hooks);
    let after = hooks.files();

    // Then
    assert!(
        before.contains("ended-8d.json.lock") && before.contains("ended-6d.json.lock"),
        "the hooks left no lock files: {before:?}"
    );
    assert!(
        !after.contains("ended-8d.json.lock") && !after.contains("dead-8d.json.lock"),
        "gc left the lock of a record it deleted: {after:?}"
    );
    assert!(
        after.contains("ended-6d.json.lock") && after.contains("live-8d.json.lock"),
        "gc deleted the lock of a record it kept: {after:?}"
    );
}

/// The start time of the pool's last gc, as `wtpool-activity/gc.stamp` holds it.
fn stamp(hooks: &Hooks) -> Option<String> {
    fs::read_to_string(hooks.activity().join("gc.stamp"))
        .ok()
        .map(|text| text.trim().to_owned())
}

#[test]
fn gc__should_run_once_a_day_on_session_end() {
    // Given
    let hooks = Hooks::must_new("gc-daily");
    aged_sessions(&hooks);
    let first = stamp(&hooks);
    let aged = records(&hooks);
    let later = add_seconds(NOW, 2 * DAY);

    // When
    hooks.send(&ev(&hooks, "UserPromptSubmit", "today", &[]));
    hooks.send(&ev(&hooks, "SessionEnd", "today", &[]));
    let after_old_stamp = records(&hooks);
    let stamped = stamp(&hooks);
    hooks.set_clock(&later);
    fs::write(
        hooks.activity().join("gc.stamp"),
        add_seconds(&later, -3600),
    )
    .expect("setup: write a stamp an hour old");
    hooks.send(&ev(&hooks, "UserPromptSubmit", "later", &[]));
    hooks.send(&ev(&hooks, "SessionEnd", "later", &[]));
    let after_fresh_stamp = records(&hooks);
    must_gc(&hooks);
    let after_manual = records(&hooks);

    // Then
    assert_eq!(
        first,
        Some(add_seconds(NOW, -6 * DAY)),
        "the session that ended 6 days ago, 2 days after the first one, did not stamp the pool"
    );
    assert_eq!(
        aged,
        set(&[
            "dead-6d",
            "dead-8d",
            "ended-6d",
            "ended-8d",
            "live-8d",
            "working-8d"
        ]),
        "the gc of a session's end deleted records under a week old"
    );
    assert_eq!(
        after_old_stamp,
        set(&["dead-6d", "ended-6d", "live-8d", "today", "working-8d"]),
        "a session's end with a stamp 6 days old did not run gc"
    );
    assert_eq!(
        stamped.as_deref(),
        Some(NOW),
        "the gc did not stamp its time"
    );
    assert_eq!(
        after_fresh_stamp,
        set(&[
            "dead-6d",
            "ended-6d",
            "later",
            "live-8d",
            "today",
            "working-8d"
        ]),
        "a session's end with a stamp an hour old ran gc"
    );
    assert_eq!(
        after_manual,
        set(&["later", "live-8d", "today", "working-8d"]),
        "wtpool hook gc did not run on a fresh stamp"
    );
    assert_eq!(
        stamp(&hooks),
        Some(later),
        "wtpool hook gc did not stamp its time"
    );
}

/// Starts one hook from the default claude without waiting.
fn spawn(hooks: &Hooks, payload: &Value) -> Child {
    use std::io::Write;
    let mut command = hooks.command(payload, &RunOptions::default());
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("setup: start wtpool hook");
    let mut stdin = child.stdin.take().expect("setup: stdin");
    let _ = stdin.write_all(payload.to_string().as_bytes());
    child
}

#[test]
fn budget__b11() {
    // Given
    let hooks = Hooks::must_new("budget-b11");
    let worktrees = ["fg", "wt1", "wt2", "wt3", "x"];
    let start = add_seconds(NOW, -7 * DAY);
    let mut failures = 0;

    // When
    for day in 0..7_i64 {
        let morning = add_seconds(&start, day * DAY + 9 * 3600);
        for session in 0..4 {
            let id = format!("d{day}s{session}");
            hooks.set_clock(&morning);
            hooks.send(&ev(&hooks, "UserPromptSubmit", &id, &[]));
            for batch in 0..5_i64 {
                hooks.set_clock(&add_seconds(&morning, (session * 5 + batch) * 300));
                let agents: Vec<String> = (0..10).map(|n| format!("b{batch}n{n}")).collect();
                for fixture in ["SubagentStart", "SubagentStop"] {
                    let children: Vec<Child> = agents
                        .iter()
                        .enumerate()
                        .map(|(n, agent)| {
                            let cwd = hooks.pool.worktree(worktrees[n % worktrees.len()]);
                            let mut payload = with(event(&hooks.pool, fixture, &id, &cwd), &[]);
                            payload["agent_id"] = json!(agent);
                            payload["agent_type"] = json!("Explore");
                            spawn(&hooks, &payload)
                        })
                        .collect();
                    failures += children
                        .into_iter()
                        .map(|child| wait_output(child, Duration::from_secs(20)))
                        .filter(|output| !output.status.success())
                        .count();
                }
            }
            hooks.set_clock(&add_seconds(&morning, 8 * 3600));
            hooks.send(&ev(&hooks, "SessionEnd", &id, &[]));
        }
        hooks.set_clock(&add_seconds(&start, day * DAY + 23 * 3600));
        must_gc(&hooks);
    }
    let count = records(&hooks).len();

    // Then
    assert_eq!(failures, 0, "{failures} subagent hooks did not exit 0");
    assert!(
        count <= 512,
        "{count} records after a week of 200 subagents a day with daily gc"
    );
    assert!(count > 0, "no record at all after a week of hooks");
}
