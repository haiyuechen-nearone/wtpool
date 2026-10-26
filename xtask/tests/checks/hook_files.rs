//! Record files under concurrent and hostile conditions, and the process a record names.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::hook::{CLAUDE, CLAUDE_STARTED, Hooks, RunOptions, event, wait_output, with};
use crate::support::must_symlink;

/// The writers and rounds model the load of one wtpool instance with fewer than 10 worktrees.
const WHOLE_WRITERS: usize = 16;
const UPDATE_WRITERS: usize = 8;
const UPDATE_ROUNDS: usize = 10;

fn ev(hooks: &Hooks, fixture: &str, session: &str, fields: &[(&str, Value)]) -> Value {
    with(event(&hooks.pool, fixture, session, &hooks.pool.fg), fields)
}

fn ids_in_flight(record: &Value) -> Vec<String> {
    record["in_flight"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|call| call["id"].as_str().map(str::to_owned))
        .collect()
}

/// Starts a hook process that has its payload on stdin already and returns it.
fn spawn(hooks: &Hooks, payload: &Value, options: &RunOptions) -> Child {
    let mut command = hooks.command(payload, options);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("setup: start wtpool hook");
    let mut stdin = child.stdin.take().expect("setup: stdin");
    let text = payload.to_string();
    std::thread::spawn(move || {
        let _ = stdin.write_all(text.as_bytes());
    });
    child
}

/// Waits for every child and returns each exit status other than 0, with its stderr.
fn wait_all(children: Vec<Child>) -> Vec<String> {
    children
        .into_iter()
        .map(|child| wait_output(child, Duration::from_secs(20)))
        .filter(|output| output.status.code() != Some(0))
        .map(|output| {
            format!(
                "{:?}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )
        })
        .collect()
}

#[rstest]
#[case::symlink("symlink")]
#[case::oversize("oversize")]
#[case::fifo("fifo")]
fn hook__should_not_follow_a_symlinked_record(#[case] kind: &str) {
    // Given
    let hooks = Hooks::must_new("hook-record-kinds");
    let record = hooks.activity().join("s1.json");
    let victim = hooks.sandbox.root.join("victim.json");
    fs::write(&victim, "WTPOOL-VERIFY-VICTIM\n").expect("setup: write the victim");
    match kind {
        "symlink" => must_symlink(&victim, &record),
        "oversize" => {
            let mut old = ev(&hooks, "UserPromptSubmit", "s1", &[]);
            old["label"] = json!("WTPOOL-VERIFY-OLD");
            old["padding"] = json!("x".repeat(17 * 1024));
            fs::write(&record, old.to_string()).expect("setup: write a large record");
        }
        _ => {
            let made = std::process::Command::new("mkfifo")
                .arg(&record)
                .status()
                .expect("setup: mkfifo");
            assert!(made.success(), "setup: mkfifo failed");
        }
    }

    // When
    let run = hooks.run(&ev(&hooks, "UserPromptSubmit", "s1", &[]));
    let meta = fs::symlink_metadata(&record);
    let text = fs::read_to_string(&record).unwrap_or_default();

    // Then
    assert_eq!(run.code(), Some(0), "wtpool hook: {}", run.describe());
    assert_eq!(
        fs::read_to_string(&victim).unwrap_or_default(),
        "WTPOOL-VERIFY-VICTIM\n",
        "wtpool hook wrote through the symlinked record"
    );
    assert!(
        meta.as_ref().is_ok_and(fs::Metadata::is_file),
        "the {kind} record was not replaced by a regular file: {meta:?}"
    );
    let parsed: Option<Value> = serde_json::from_str(&text).ok();
    assert!(
        parsed.as_ref().is_some_and(|r| r["session_id"] == "s1"),
        "the replaced record is no record of s1: {text:.200}"
    );
    assert!(
        !text.contains("WTPOOL-VERIFY-OLD"),
        "wtpool hook read the record over 16 KiB"
    );
}

/// The size limit of a record. `wtpool status` and the hook refuse to read a larger one.
const MAX_RECORD: usize = 16 * 1024;

/// A folder below `fg` whose path is `length` bytes longer, in parts of 200 bytes.
fn deeper(fg: &Path, length: usize) -> PathBuf {
    let part = format!("/{}", "d".repeat(199));
    let tail: String = part.chars().cycle().take(length).collect();
    PathBuf::from(format!("{}{tail}", fg.display()))
}

#[test]
fn hook__should_keep_a_record_it_can_read() {
    // Given
    let hooks = Hooks::must_new("hook-record-fits");
    let record_path = |stem: &str| hooks.activity().join(format!("{stem}.json"));
    hooks.send(&ev(&hooks, "UserPromptSubmit", "s-short", &[]));
    let short = fs::read(record_path("s-short")).map_or(0, |text| text.len());
    // The first event's cwd, kept as first_cwd, brings the record within 150 bytes of 16 KiB.
    let cwd = deeper(&hooks.pool.fg, MAX_RECORD - 150 - short);
    let full = |fixture: &str, fields: &[(&str, Value)]| {
        with(event(&hooks.pool, fixture, "s-full", &cwd), fields)
    };
    hooks.send(&full("UserPromptSubmit", &[]));
    let first = fs::read(record_path("s-full")).unwrap_or_default();

    // When
    hooks.send(&full(
        "PreToolUse-Read",
        &[
            ("tool_use_id", json!("toolu_over")),
            (
                "tool_input",
                json!({ "file_path": hooks.pool.fg.join("f.txt") }),
            ),
        ],
    ));
    let trimmed = fs::read(record_path("s-full")).unwrap_or_default();
    hooks.send(&full(
        "StopFailure",
        &[("error", json!("\u{1f600}".repeat(80)))],
    ));
    let kept = fs::read(record_path("s-full")).unwrap_or_default();
    let status = hooks.status();
    let log = fs::read_to_string(hooks.sandbox.root.join("xdg/state/wtpool/hook.log"))
        .unwrap_or_default();

    // Then
    assert!(
        (MAX_RECORD - 200..=MAX_RECORD).contains(&first.len()),
        "setup: the first record of s-full has {} bytes",
        first.len()
    );
    let record: Value = serde_json::from_slice(&trimmed).unwrap_or_default();
    assert!(
        trimmed.len() <= MAX_RECORD,
        "a call that brings the record past 16 KiB left {} bytes",
        trimmed.len()
    );
    assert_eq!(
        (&record["action"]["tool"], &record["in_flight"]),
        (&json!("Read"), &json!([])),
        "the call past 16 KiB was not dropped from in_flight alone: {:.300}",
        String::from_utf8_lossy(&trimmed)
    );
    assert_eq!(
        kept, trimmed,
        "an event whose record cannot fit in 16 KiB changed the record"
    );
    let actors = &crate::support::worktree(&status, "fg")["actors"];
    assert!(
        actors
            .as_array()
            .is_some_and(|a| a.iter().any(|actor| actor["key"] == "s-full")),
        "wtpool status cannot read s-full after an event too large to record: {actors}"
    );
    assert!(
        log.contains("wtpool hook: skipped an event: its record cannot be written"),
        "the hook log says nothing of the event it skipped: {log:?}"
    );
}

#[test]
fn hook__should_keep_records_whole_under_parallel_writers() {
    // Given
    let hooks = Hooks::must_new("hook-whole-records");
    hooks.send(&ev(&hooks, "UserPromptSubmit", "s1", &[]));
    let record = hooks.activity().join("s1.json");
    let stop = Arc::new(AtomicBool::new(false));
    let reader = {
        let stop = Arc::clone(&stop);
        let record = record.clone();
        std::thread::spawn(move || {
            let mut torn = Vec::new();
            let mut reads = 0;
            while !stop.load(Ordering::SeqCst) {
                if let Ok(text) = fs::read_to_string(&record) {
                    reads += 1;
                    if serde_json::from_str::<Value>(&text).is_err() {
                        torn.push(text.chars().take(120).collect::<String>());
                    }
                }
            }
            (reads, torn)
        })
    };
    let payloads: Vec<Value> = (0..WHOLE_WRITERS)
        .map(|n| {
            ev(
                &hooks,
                "PreToolUse-Read",
                "s1",
                &[("tool_use_id", json!(format!("toolu_w{n:03}")))],
            )
        })
        .collect();

    // When
    let children: Vec<Child> = payloads
        .iter()
        .map(|payload| spawn(&hooks, payload, &RunOptions::default()))
        .collect();
    let failed = wait_all(children);
    stop.store(true, Ordering::SeqCst);
    let (reads, torn) = reader.join().expect("setup: the reader thread");

    // Then
    assert!(failed.is_empty(), "writers that did not exit 0: {failed:?}");
    assert!(reads > 0, "setup: the reader read nothing");
    assert!(
        torn.is_empty(),
        "torn records read during {WHOLE_WRITERS} writers: {torn:?}"
    );
    let leftovers: Vec<String> = hooks
        .files()
        .into_iter()
        .filter(|name| name != "s1.json" && name != "s1.json.lock")
        .collect();
    assert!(
        leftovers.is_empty(),
        "files left after the writers: {leftovers:?}"
    );
    assert!(
        hooks.record("s1").is_some(),
        "no record after {WHOLE_WRITERS} writers"
    );
}

#[test]
fn liveness__should_record_the_parent_process() {
    // Given
    let hooks = Hooks::must_new("liveness-parent");
    let orphan = RunOptions {
        parent: Some(1),
        ..RunOptions::default()
    };

    // When
    hooks.send(&ev(&hooks, "UserPromptSubmit", "s1", &[]));
    let run = hooks.run_with(&ev(&hooks, "UserPromptSubmit", "s2", &[]), &orphan);

    // Then
    assert_eq!(run.code(), Some(0), "wtpool hook: {}", run.describe());
    let first = hooks.must_record("s1");
    assert_eq!(
        first["process"],
        json!({ "pid": CLAUDE, "started": CLAUDE_STARTED }),
        "the process of a hook run by claude {CLAUDE}"
    );
    let orphaned = hooks.record("s2");
    let process = orphaned.as_ref().map(|r| r["process"].clone());
    assert!(
        orphaned.is_some() && process.as_ref().is_none_or(Value::is_null),
        "a hook whose parent is pid 1 recorded {process:?} (record {orphaned:?})"
    );
}

#[test]
fn liveness__should_keep_the_record_when_reparented() {
    // Given
    let hooks = Hooks::must_new("liveness-reparented");
    hooks.send(&ev(&hooks, "UserPromptSubmit", "s1", &[]));
    hooks.advance(5);
    let orphan = RunOptions {
        parent: Some(1),
        ..RunOptions::default()
    };

    // When
    let run = hooks.run_with(&ev(&hooks, "Stop", "s1", &[]), &orphan);
    let record = hooks.must_record("s1");
    let status = hooks.status();

    // Then
    assert_eq!(run.code(), Some(0), "wtpool hook: {}", run.describe());
    assert_eq!(
        record["process"],
        json!({ "pid": CLAUDE, "started": CLAUDE_STARTED }),
        "a reparented hook replaced the record's process"
    );
    assert_eq!(
        record["state"],
        json!("idle"),
        "the reparented hook's Stop was not applied"
    );
    let actors = &crate::support::worktree(&status, "fg")["actors"];
    assert!(
        actors
            .as_array()
            .is_some_and(|a| a.iter().any(|actor| actor["key"] == "s1")),
        "s1, whose claude lives, left the document after a reparented hook: {actors}"
    );
}

#[test]
fn merge__should_refuse_an_older_state() {
    // Given
    let hooks = Hooks::must_new("merge-older");
    hooks.send(&ev(&hooks, "UserPromptSubmit", "s1", &[]));
    hooks.advance(1);
    let older = ev(&hooks, "Stop", "s1", &[]);
    let newer = ev(&hooks, "PreToolUse-AskUserQuestion", "s1", &[]);
    let mut command = hooks.command(&older, &RunOptions::default());
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // When
    let mut first = command.spawn().expect("setup: start the older hook");
    std::thread::sleep(Duration::from_millis(150));
    let second = hooks.run(&newer);
    let mut stdin = first.stdin.take().expect("setup: stdin");
    let _ = stdin.write_all(older.to_string().as_bytes());
    drop(stdin);
    let output = wait_output(first, Duration::from_secs(5));
    let record = hooks.must_record("s1");

    // Then
    assert_eq!(
        second.code(),
        Some(0),
        "the newer hook: {}",
        second.describe()
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "the older hook exited {:?}",
        output.status
    );
    assert_eq!(
        record["state"],
        json!("waiting"),
        "a hook that started earlier and wrote later replaced the newer state: {record}"
    );
    assert_eq!(record["waiting"]["reason"], json!("question"));
}

#[test]
fn hook__should_lose_no_update_under_parallel_writers() {
    // Given
    let hooks = Hooks::must_new("lose-no-update");
    hooks.send(&ev(&hooks, "UserPromptSubmit", "s1", &[]));
    let mut problems = Vec::new();

    // When
    for round in 0..UPDATE_ROUNDS {
        let ids: Vec<String> = (0..UPDATE_WRITERS)
            .map(|n| format!("toolu_r{round:02}c{n:02}"))
            .collect();
        let starts: Vec<Child> = ids
            .iter()
            .map(|id| {
                let payload = ev(
                    &hooks,
                    "PreToolUse-Read",
                    "s1",
                    &[("tool_use_id", json!(id))],
                );
                spawn(&hooks, &payload, &RunOptions::default())
            })
            .collect();
        problems.extend(
            wait_all(starts)
                .into_iter()
                .map(|e| format!("round {round}: {e}")),
        );
        let flying = ids_in_flight(&hooks.must_record("s1"));
        let lost: Vec<&String> = ids.iter().filter(|id| !flying.contains(id)).collect();
        if !lost.is_empty() {
            problems.push(format!("round {round}: PreToolUse lost {lost:?}"));
        }
        let ends: Vec<Child> = ids
            .iter()
            .map(|id| {
                let payload = ev(
                    &hooks,
                    "PostToolUse-Read",
                    "s1",
                    &[("tool_use_id", json!(id))],
                );
                spawn(&hooks, &payload, &RunOptions::default())
            })
            .collect();
        problems.extend(
            wait_all(ends)
                .into_iter()
                .map(|e| format!("round {round}: {e}")),
        );
        let left = ids_in_flight(&hooks.must_record("s1"));
        let kept: Vec<&String> = ids.iter().filter(|id| left.contains(id)).collect();
        if !kept.is_empty() {
            problems.push(format!(
                "round {round}: PostToolUse lost, still in flight {kept:?}"
            ));
        }
    }

    // Then
    assert!(
        problems.is_empty(),
        "lost updates:\n{}",
        problems.join("\n")
    );
}

#[test]
fn hook__should_restart_when_gc_deleted_the_lock() {
    // Given
    let mut hooks = Hooks::must_new("restart-after-gc");
    let dead = RunOptions {
        parent: Some(7099),
        ..RunOptions::default()
    };
    let mut problems = Vec::new();

    // When
    for round in 0..20 {
        let session = format!("g{round:02}");
        hooks.set_clock("2026-10-01T12:00:00Z");
        let run = hooks.run_with(&ev(&hooks, "UserPromptSubmit", &session, &[]), &dead);
        if run.code() != Some(0) || hooks.record(&session).is_none() {
            problems.push(format!(
                "round {round}: the first hook wrote no record: {}",
                run.describe()
            ));
            continue;
        }
        hooks.set_clock("2026-10-20T12:00:00Z");
        let ids: Vec<String> = (0..8).map(|n| format!("toolu_g{round:02}{n}")).collect();
        let mut gc = hooks
            .sandbox
            .command(&crate::support::must_wtpool_bin(), &hooks.pool.fg);
        gc.args(["hook", "gc"]);
        for (key, value) in hooks.base_envs() {
            gc.env(key, value);
        }
        gc.stdout(Stdio::null()).stderr(Stdio::piped());
        let mut children = vec![gc.spawn().expect("setup: start wtpool hook gc")];
        children.extend(ids.iter().map(|id| {
            let payload = ev(
                &hooks,
                "PreToolUse-Read",
                &session,
                &[("tool_use_id", json!(id))],
            );
            spawn(&hooks, &payload, &dead)
        }));
        problems.extend(
            wait_all(children)
                .into_iter()
                .map(|e| format!("round {round}: {e}")),
        );
        match hooks.record(&session) {
            None => problems.push(format!(
                "round {round}: the record is gone after writers ran"
            )),
            Some(record) => {
                let flying = ids_in_flight(&record);
                let lost: Vec<&String> = ids.iter().filter(|id| !flying.contains(id)).collect();
                if !lost.is_empty() {
                    problems.push(format!("round {round}: lost {lost:?}"));
                }
            }
        }
    }
    hooks.stop(7099);

    // Then
    assert!(
        problems.is_empty(),
        "updates lost to gc:\n{}",
        problems.join("\n")
    );
}
