//! Agents in `wtpool status`: the state an actor shows as its last event ages, activity
//! precedence, records outside the pool and the records it reads.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::hook::{CLAUDE_STARTED, Hooks, START, add_seconds};
use crate::support::{must_symlink, worktree};

fn read_in(hooks: &Hooks, worktree: &str) -> Vec<(&'static str, Value)> {
    vec![
        (
            "tool_input",
            json!({ "file_path": hooks.pool.worktree(worktree).join("f.txt") }),
        ),
        ("tool_use_id", json!(format!("toolu_read_{worktree}"))),
    ]
}

fn bash_in(hooks: &Hooks, worktree: &str, id: &str) -> Vec<(&'static str, Value)> {
    let command = format!(
        "cd {} && cargo test",
        hooks.pool.worktree(worktree).display()
    );
    vec![
        (
            "tool_input",
            json!({ "command": command, "description": "Test" }),
        ),
        ("tool_use_id", json!(id)),
    ]
}

/// The actor `key` of any worktree of `document`, with the worktree's name.
fn actor<'a>(document: &'a Value, key: &str) -> Option<(&'a str, &'a Value)> {
    document["pools"][0]["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|w| {
            let found = w["actors"].as_array()?.iter().find(|a| a["key"] == key)?;
            Some((w["name"].as_str().unwrap_or_default(), found))
        })
}

fn state_of(document: &Value, key: &str) -> Option<String> {
    actor(document, key).and_then(|(_, a)| a["state"].as_str().map(str::to_owned))
}

/// Runs the case's events from START, then reads the document `after` seconds later.
fn decay_case(hooks: &mut Hooks, case: &str) -> (i64, &'static str, Option<&'static str>) {
    let s = |fixture: &str, fields: Vec<(&'static str, Value)>| hooks.event(fixture, "s1", &fields);
    hooks.send(&s("UserPromptSubmit", vec![]));
    match case {
        "working_under_120_s" => (119, "s1", Some("working")),
        "working_over_120_s" => (121, "s1", Some("quiet")),
        "bash_during_a_build_here" | "bash_during_a_build_elsewhere" | "read_during_a_build" => {
            let fields = if case == "read_during_a_build" {
                read_in(hooks, "wt1")
            } else {
                bash_in(hooks, "wt1", "toolu_bash")
            };
            let fixture = if case == "read_during_a_build" {
                "PreToolUse-Read"
            } else {
                "PreToolUse-Bash"
            };
            hooks.send(&s(fixture, fields));
            let build_in = if case == "bash_during_a_build_elsewhere" {
                "wt2"
            } else {
                "wt1"
            };
            let cwd = hooks.pool.worktree(build_in);
            hooks.start(4300, "cargo", &cwd, CLAUDE_STARTED);
            let state = if case == "bash_during_a_build_here" {
                "working"
            } else {
                "quiet"
            };
            (300, "s1", Some(state))
        }
        "agent_while_its_subagent_works" => {
            hooks.send(&s("PreToolUse-Agent", vec![]));
            hooks.send(&hooks.sub_event("SubagentStart", "a1", &[]));
            (300, "s1", Some("working"))
        }
        "agent_while_its_subagent_waits" => {
            hooks.send(&s("PreToolUse-Agent", vec![]));
            hooks.send(&hooks.sub_event("SubagentStart", "a1", &[]));
            hooks.send(&hooks.sub_event(
                "PreToolUse-Bash",
                "a1",
                &bash_in(hooks, "wt2", "toolu_sub"),
            ));
            let command = json!({ "command": format!("cd {} && cargo test", hooks.pool.worktree("wt2").display()), "description": "Test" });
            hooks.send(&hooks.sub_event(
                "PermissionRequest-Bash",
                "a1",
                &[("tool_input", command)],
            ));
            (300, "s1", Some("working"))
        }
        "agent_after_its_subagent_is_done" => {
            hooks.send(&s("PreToolUse-Agent", vec![]));
            hooks.send(&hooks.sub_event("SubagentStart", "a1", &[]));
            hooks.send(&hooks.sub_event("SubagentStop", "a1", &[]));
            (300, "s1", Some("quiet"))
        }
        "subagent_without_events" => {
            hooks.send(&s("PreToolUse-Agent", vec![]));
            hooks.send(&hooks.sub_event("SubagentStart", "a1", &[]));
            (300, "s1.a1", Some("quiet"))
        }
        "waiting_never_decays" => {
            hooks.send(&s("PermissionRequest-Bash", vec![]));
            (3600, "s1", Some("waiting"))
        }
        "idle_stays_idle" => {
            hooks.send(&s("Stop", vec![]));
            (3600, "s1", Some("idle"))
        }
        "done_under_a_minute" => {
            hooks.send(&s("PreToolUse-Agent", vec![]));
            hooks.send(&hooks.sub_event("SubagentStart", "a1", &[]));
            hooks.send(&hooks.sub_event("SubagentStop", "a1", &[]));
            (59, "s1.a1", Some("done"))
        }
        "done_over_a_minute" => {
            hooks.send(&s("PreToolUse-Agent", vec![]));
            hooks.send(&hooks.sub_event("SubagentStart", "a1", &[]));
            hooks.send(&hooks.sub_event("SubagentStop", "a1", &[]));
            (61, "s1.a1", None)
        }
        "ended" => {
            hooks.send(&s("SessionEnd", vec![]));
            (1, "s1", None)
        }
        "process_dead" => {
            hooks.stop(crate::support::hook::CLAUDE);
            (1, "s1", None)
        }
        other => panic!("setup: unknown case {other}"),
    }
}

#[rstest]
#[case("working_under_120_s")]
#[case("working_over_120_s")]
#[case("bash_during_a_build_here")]
#[case("bash_during_a_build_elsewhere")]
#[case("read_during_a_build")]
#[case("agent_while_its_subagent_works")]
#[case("agent_while_its_subagent_waits")]
#[case("agent_after_its_subagent_is_done")]
#[case("subagent_without_events")]
#[case("waiting_never_decays")]
#[case("idle_stays_idle")]
#[case("done_under_a_minute")]
#[case("done_over_a_minute")]
#[case("ended")]
#[case("process_dead")]
#[case("job_waiting_past_120_s")]
#[case("job_waiting_with_a_label")]
#[case("job_waiting_after_stop")]
#[case("job_waiting_while_waiting")]
#[case("job_running_past_120_s")]
#[case("job_left_the_folder")]
fn decay__should_match_the_table(#[case] case: &str) {
    if super::jobs_decay::is_job_case(case) {
        super::jobs_decay::must_decay_with_a_job(case);
        return;
    }
    // Given
    let mut hooks = Hooks::must_new("decay");
    let (after, key, expected) = decay_case(&mut hooks, case);
    let fresh = hooks.status();

    // When
    hooks.set_clock(&add_seconds(START, after));
    let later = hooks.status();

    // Then
    assert!(
        actor(&fresh, "s1").is_some() || matches!(case, "ended" | "process_dead"),
        "{case}: s1 is in no document right after its events: {fresh}"
    );
    assert_eq!(
        state_of(&later, key).as_deref(),
        expected,
        "{case}: {key} {after} s after the first event"
    );
}

/// A PermissionRequest whose command names no path, so the actor stays where it is.
fn pathless_permission() -> Vec<(&'static str, Value)> {
    vec![(
        "tool_input",
        json!({ "command": "git push", "description": "Push" }),
    )]
}

fn activity(document: &Value, name: &str) -> String {
    worktree(document, name)["activity"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[rstest]
#[case::waiting_before_working("waiting_before_working", "wt1", "waiting")]
#[case::working_before_building("working_before_building", "wt1", "working")]
#[case::building_before_idle("building_before_idle", "wt1", "building")]
#[case::idle("idle", "wt1", "idle")]
#[case::quiet_counts_as_idle("quiet_counts_as_idle", "wt1", "idle")]
#[case::done_counts_as_nothing("done_counts_as_nothing", "wt1", "free")]
#[case::idle_before_foreground("idle_before_foreground", "fg", "idle")]
#[case::missing_before_waiting("missing_before_waiting", "wt3", "missing")]
fn activity__should_follow_the_precedence(
    #[case] case: &str,
    #[case] name: &str,
    #[case] expected: &str,
) {
    // Given
    let mut hooks = Hooks::must_new("activity");
    let fg = hooks.pool.fg.clone();
    hooks.start(7002, "claude", &fg, CLAUDE_STARTED);
    let run_as = |hooks: &Hooks, session: &str, fixture: &str, fields: &[(&str, Value)]| {
        let payload = hooks.event(fixture, session, fields);
        let options = crate::support::hook::RunOptions {
            parent: Some(if session == "s2" { 7002 } else { 7001 }),
            ..Default::default()
        };
        let run = hooks.run_with(&payload, &options);
        assert_eq!(run.code(), Some(0), "wtpool hook: {}", run.describe());
    };
    let target = if name == "fg" { "fg" } else { name };
    run_as(&hooks, "s1", "UserPromptSubmit", &[]);
    run_as(&hooks, "s1", "PreToolUse-Read", &read_in(&hooks, target));
    let mut clock_after = 1;
    match case {
        "waiting_before_working" => {
            run_as(
                &hooks,
                "s1",
                "PermissionRequest-Bash",
                &pathless_permission(),
            );
            run_as(&hooks, "s2", "UserPromptSubmit", &[]);
            run_as(&hooks, "s2", "PreToolUse-Read", &read_in(&hooks, target));
        }
        "working_before_building" | "building_before_idle" => {
            if case == "building_before_idle" {
                run_as(&hooks, "s1", "Stop", &[]);
            }
            let cwd = hooks.pool.worktree(target);
            hooks.start(4301, "cargo", &cwd, CLAUDE_STARTED);
        }
        "idle" | "idle_before_foreground" => run_as(&hooks, "s1", "Stop", &[]),
        "quiet_counts_as_idle" => clock_after = 200,
        "done_counts_as_nothing" => {
            let mut moved = hooks.event("PreToolUse-Read", "s1", &read_in(&hooks, "fg"));
            moved["tool_use_id"] = json!("toolu_back");
            hooks.send(&moved);
            hooks.send(&hooks.sub_event("SubagentStart", "a1", &[]));
            hooks.send(&hooks.sub_event(
                "PreToolUse-Read-subagent",
                "a1",
                &read_in(&hooks, target),
            ));
            hooks.send(&hooks.sub_event("SubagentStop", "a1", &[]));
        }
        "missing_before_waiting" => {
            run_as(
                &hooks,
                "s1",
                "PermissionRequest-Bash",
                &pathless_permission(),
            );
            fs::remove_dir_all(hooks.pool.worktree(target)).expect("setup: remove wt3");
        }
        other => panic!("setup: unknown case {other}"),
    }

    // When
    hooks.set_clock(&add_seconds(START, clock_after));
    let document = hooks.status();

    // Then
    assert!(
        actor(&document, "s1").is_some()
            || case == "missing_before_waiting"
            || case == "done_counts_as_nothing",
        "{case}: s1 shows nowhere: {document}"
    );
    assert_eq!(
        activity(&document, name),
        expected,
        "{case}: the activity of {name}"
    );
}

#[test]
fn status__should_ignore_a_record_outside_the_pool() {
    // Given
    let hooks = Hooks::must_new("outside-pool");
    hooks.send(&hooks.event("UserPromptSubmit", "s1", &[]));
    hooks.send(&hooks.event("PreToolUse-Read", "s1", &read_in(&hooks, "wt1")));
    hooks.send(&hooks.event("UserPromptSubmit", "s3", &[]));
    hooks.send(&hooks.event("PreToolUse-Read", "s3", &read_in(&hooks, "wt3")));
    let mut foreign = hooks.must_record("s1");
    foreign["session_id"] = json!("s-foreign");
    foreign["worktree"] = json!("/Users/someone/other-repo");
    fs::write(hooks.activity().join("s-foreign.json"), foreign.to_string())
        .expect("setup: write a record of another pool");
    hooks
        .pool
        .git(
            "fg",
            &["worktree", "remove", "--force", &hooks.worktree_text("wt3")],
        )
        .unwrap_or_else(|e| panic!("setup: remove wt3: {e}"));

    // When
    let document = hooks.status();

    // Then
    assert!(
        actor(&document, "s1").is_some(),
        "s1 in wt1 shows nowhere: {document}"
    );
    assert!(
        actor(&document, "s-foreign").is_none(),
        "a record of another pool shows"
    );
    assert!(
        actor(&document, "s3").is_none(),
        "a record of a removed worktree shows"
    );
    assert_eq!(
        document["pools"][0]["skipped_records"],
        json!(0),
        "records outside the pool count as skipped"
    );
    let wt1 = worktree(&document, "wt1")["last_event"].clone();
    let s1_updated = hooks.must_record("s1")["updated"].clone();
    assert_eq!(wt1, s1_updated, "wt1's last_event");
}

fn copy_record(template: &Value, session: &str, path: &Path, extra: &[(&str, Value)]) {
    let mut record = template.clone();
    record["session_id"] = json!(session);
    for (key, value) in extra {
        record[*key] = value.clone();
    }
    fs::write(path, record.to_string()).expect("setup: write a record");
}

#[test]
fn status__should_skip_fifos_symlinks_locks_and_large_records() {
    // Given
    let hooks = Hooks::must_new("skip-records");
    hooks.send(&hooks.event("UserPromptSubmit", "s1", &[]));
    hooks.send(&hooks.event("PreToolUse-Read", "s1", &read_in(&hooks, "wt1")));
    let template = hooks.must_record("s1");
    let dir = hooks.activity();
    let outside = hooks.sandbox.root.join("s-link-target.json");
    copy_record(&template, "s-link", &outside, &[]);
    must_symlink(&outside, &dir.join("s-link.json"));
    copy_record(
        &template,
        "s-large",
        &dir.join("s-large.json"),
        &[("padding", json!("x".repeat(17 * 1024)))],
    );
    copy_record(&template, "s-lock", &dir.join("s-lock.json.lock"), &[]);
    copy_record(&template, "s-temp", &dir.join(".s-temp.json.tmp"), &[]);
    copy_record(&template, "s-temp2", &dir.join("s-temp2.json.tmp"), &[]);
    copy_record(&template, "s-stamp", &dir.join("gc.stamp"), &[]);
    for name in ["x y", "a.b.c", "sé"] {
        copy_record(&template, name, &dir.join(format!("{name}.json")), &[]);
    }
    let made = std::process::Command::new("mkfifo")
        .arg(dir.join("s-fifo.json"))
        .status()
        .expect("setup: mkfifo");
    assert!(made.success(), "setup: mkfifo failed");

    // When
    let started = Instant::now();
    let document = hooks.status();
    let took = started.elapsed();

    // Then
    assert!(
        took < Duration::from_secs(5),
        "wtpool status took {took:?} with a FIFO record"
    );
    assert!(
        actor(&document, "s1").is_some(),
        "the valid record shows nowhere: {document}"
    );
    let shown: Vec<&str> = [
        "s-link", "s-large", "s-lock", "s-temp", "s-temp2", "s-stamp", "x y", "a.b.c", "sé",
        "s-fifo",
    ]
    .into_iter()
    .filter(|key| {
        actor(&document, key).is_some() || document.to_string().contains(&format!("\"{key}\""))
    })
    .collect();
    assert!(
        shown.is_empty(),
        "records wtpool status must skip show: {shown:?}"
    );
}

/// Sets the modification time of `path` to `seconds` after START.
fn set_mtime(path: &Path, seconds: i64) {
    let base = SystemTime::UNIX_EPOCH + Duration::from_secs(1_792_497_600);
    let time = if seconds >= 0 {
        base + Duration::from_secs(seconds.unsigned_abs())
    } else {
        base - Duration::from_secs(seconds.unsigned_abs())
    };
    fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(time))
        .expect("setup: set the modification time");
}

#[test]
fn status__should_keep_the_newest_512_records() {
    // Given
    let hooks = Hooks::must_new("newest-512");
    hooks.send(&hooks.event("UserPromptSubmit", "live", &[]));
    hooks.send(&hooks.event("PreToolUse-Read", "live", &read_in(&hooks, "wt3")));
    hooks.send(&hooks.event("Stop", "live", &[]));
    hooks.send(&hooks.event("UserPromptSubmit", "ended", &[]));
    hooks.send(&hooks.event("SessionEnd", "ended", &[]));
    let template = hooks.must_record("ended");
    fs::remove_file(hooks.activity().join("ended.json")).expect("setup: remove the template");
    let _ = fs::remove_file(hooks.activity().join("ended.json.lock"));
    let dir = hooks.activity();
    let mut newest = String::new();
    for n in 0..600_i64 {
        let worktree = if n < 88 { "wt2" } else { "wt1" };
        let offset = -86_400 + n * 60;
        let updated = add_seconds(START, offset);
        let session = format!("old{n:03}");
        let path = dir.join(format!("{session}.json"));
        copy_record(
            &template,
            &session,
            &path,
            &[
                ("worktree", json!(hooks.worktree_text(worktree))),
                ("updated", json!(updated)),
                ("since", json!(updated)),
            ],
        );
        set_mtime(&path, offset);
        newest = updated;
    }
    let mut live = hooks.must_record("live");
    live["updated"] = json!(add_seconds(START, -2 * 86_400));
    live["since"] = live["updated"].clone();
    fs::write(dir.join("live.json"), live.to_string()).expect("setup: age the live record");
    set_mtime(&dir.join("live.json"), -2 * 86_400);

    // When
    let document = hooks.status();

    // Then
    assert_eq!(
        worktree(&document, "wt1")["last_event"],
        json!(newest),
        "wt1's last_event comes from the newest record"
    );
    assert_eq!(
        worktree(&document, "wt2")["last_event"],
        Value::Null,
        "the 88 oldest records, all in wt2, were read"
    );
    assert_eq!(
        state_of(&document, "live").as_deref(),
        Some("idle"),
        "the oldest record, of a live session, was skipped"
    );
}

#[test]
fn status__should_skip_an_unknown_record_version() {
    // Given
    let hooks = Hooks::must_new("record-version");
    hooks.send(&hooks.event("UserPromptSubmit", "s1", &[]));
    hooks.send(&hooks.event("PreToolUse-Read", "s1", &read_in(&hooks, "wt1")));
    let template = hooks.must_record("s1");
    let dir = hooks.activity();
    copy_record(
        &template,
        "s-v2",
        &dir.join("s-v2.json"),
        &[("version", json!(2))],
    );
    let mut no_version = template.clone();
    no_version["session_id"] = json!("s-none");
    if let Some(map) = no_version.as_object_mut() {
        map.remove("version");
    }
    fs::write(dir.join("s-none.json"), no_version.to_string()).expect("setup: write a record");
    fs::write(dir.join("s-bad.json"), "{\"version\": 1, \"session_id\": ")
        .expect("setup: write a record");

    // When
    let document = hooks.status();

    // Then
    assert!(
        actor(&document, "s1").is_some(),
        "the valid record shows nowhere: {document}"
    );
    for key in ["s-v2", "s-none", "s-bad"] {
        assert!(actor(&document, key).is_none(), "{key} shows");
    }
    assert_eq!(
        document["pools"][0]["skipped_records"],
        json!(3),
        "skipped_records with an unknown version, no version and a parse error"
    );
}

/// A record of `key` as an older wtpool wrote it from `template`: no `harness`, and calls of
/// `tools` without a kind, in the harness process `pid`.
fn older(template: &Value, key: &str, pid: u32, tools: &[&str]) -> Value {
    let mut record = template.clone();
    let (session, agent) = key
        .split_once('.')
        .map_or((key, None), |(s, a)| (s, Some(a)));
    record["session_id"] = json!(session);
    record["agent_id"] = json!(agent);
    record["process"]["pid"] = json!(pid);
    let since = template["since"].clone();
    let calls: Vec<Value> = tools
        .iter()
        .enumerate()
        .map(|(n, tool)| {
            let id = format!("toolu_{key}_{n}");
            json!({ "id": id, "tool": tool, "target": "x", "since": since })
        })
        .collect();
    record["in_flight"] = json!(calls);
    if let Some(map) = record.as_object_mut() {
        map.remove("harness");
    }
    record
}

#[test]
fn status__should_read_the_kind_of_an_older_call() {
    // Given
    // Past 120 s only a shell call during a build, or a call starting a subagent while that
    // subagent works, keeps a session working.
    let mut hooks = Hooks::must_new("older-kinds");
    hooks.send(&hooks.event("UserPromptSubmit", "s1", &[]));
    hooks.send(&hooks.event(
        "PreToolUse-Bash",
        "s1",
        &bash_in(&hooks, "wt1", "toolu_bash"),
    ));
    let (wt1, fg) = (hooks.pool.worktree("wt1"), hooks.pool.fg.clone());
    hooks.start(4300, "cargo", &wt1, CLAUDE_STARTED);
    for pid in [7101, 7102, 7103, 7104] {
        hooks.start(pid, "claude", &fg, CLAUDE_STARTED);
    }
    let template = hooks.must_record("s1");
    let dir = hooks.activity();
    let mut other = older(&template, "s2", 7102, &["Bash"]);
    other["harness"] = json!("other-harness");
    let records = [
        ("s1", older(&template, "s1", 7101, &["Bash"])),
        ("other-harness-s2", other),
        ("s3", older(&template, "s3", 7103, &["Write", "Read"])),
        ("s4", older(&template, "s4", 7104, &["Agent"])),
        ("s4.a1", older(&template, "s4.a1", 7104, &[])),
    ];
    for (key, record) in &records {
        fs::write(dir.join(format!("{key}.json")), record.to_string())
            .unwrap_or_else(|e| panic!("setup: write {key} as an older wtpool: {e}"));
    }

    // When
    hooks.set_clock(&add_seconds(START, 300));
    let document = hooks.status();

    // Then
    let seen: Vec<(&str, Option<String>)> = ["s1", "other-harness-s2", "s3", "s4"]
        .into_iter()
        .map(|key| (key, state_of(&document, key)))
        .collect();
    let working = || Some("working".to_owned());
    let quiet = || Some("quiet".to_owned());
    assert_eq!(
        seen,
        vec![
            ("s1", working()),
            ("other-harness-s2", quiet()),
            ("s3", quiet()),
            ("s4", working())
        ],
        "Bash without a kind runs a shell, the same call of a harness without an adapter is \
         other, Write edits and Read is other, Agent starts a subagent: {document}"
    );
    let harness = |key: &str| actor(&document, key).map(|(_, a)| a["harness"].clone());
    assert_eq!(
        (harness("s1"), harness("other-harness-s2")),
        (Some(json!("claude-code")), Some(json!("other-harness"))),
        "the harness of each actor"
    );
}
