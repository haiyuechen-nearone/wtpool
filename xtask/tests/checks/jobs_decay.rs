//! Jobs in `wtpool status`: the lists of `machine.jobs` (S14), each job's worktree and actor
//! (S15), and the decay table of A1 with a job of the actor's own (A8).

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::jobs_status::{list_job, must_have, proc, started};
use crate::support::hook::{CLAUDE, Hooks, RunOptions, START, add_seconds, event, with};
use crate::support::jobs::{JobSpec, SETTLE, jobs_of};
use crate::support::{FileState, file_state, must_repo, must_status_json, phase};

/// The PreToolUse of `command` with Bash, from `session` or its subagent `agent`, in fg.
fn bash(hooks: &Hooks, session: &str, agent: Option<&str>, id: &str, command: &str) -> Value {
    let mut payload = with(
        event(&hooks.pool, "PreToolUse-Bash", session, &hooks.pool.fg),
        &[
            (
                "tool_input",
                json!({ "command": command, "description": "Run it" }),
            ),
            ("tool_use_id", json!(id)),
        ],
    );
    if let Some(agent) = agent {
        payload["agent_id"] = json!(agent);
        payload["agent_type"] = json!("general-purpose");
    }
    payload
}

/// `payload` from the subagent `agent` of its session.
fn of_agent(mut payload: Value, agent: &str) -> Value {
    payload["agent_id"] = json!(agent);
    payload["agent_type"] = json!("general-purpose");
    payload
}

/// Sends `payload` from the fake harness process `parent`; the hook must exit 0.
fn send_from(hooks: &Hooks, parent: u32, payload: &Value) {
    let options = RunOptions {
        parent: Some(parent),
        ..RunOptions::default()
    };
    let run = hooks.run_with(payload, &options);
    assert_eq!(run.code(), Some(0), "wtpool hook: {}", run.describe());
}

/// Each entry of the job folder with its size, mtime, mode and inode.
fn folder_state(dir: &Path) -> BTreeMap<PathBuf, FileState> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let meta = fs::symlink_metadata(entry.path()).ok()?;
                    Some((entry.path(), file_state(&meta)))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn status__should_attribute_jobs() {
    // Given
    let mut hooks = Hooks::must_new("status-attribute");
    hooks
        .sandbox
        .write_machine("[jobs]\nlimit = 8\nheavy = [\"cargo test\", \"cargo build\"]\n");
    let at = "2026-10-20T11:00:00Z";
    let (fg, wt1, wt2, wt3, x) = (
        hooks.pool.fg.clone(),
        hooks.pool.worktree("wt1"),
        hooks.pool.worktree("wt2"),
        hooks.pool.worktree("wt3"),
        hooks.pool.worktree("x"),
    );
    let elsewhere = hooks.sandbox.root.join("elsewhere");
    fs::create_dir_all(&elsewhere).expect("setup: create a folder outside the pool");
    let cd = |path: &Path, command: &str| format!("cd {} && {command}", path.display());
    hooks.send(&event(&hooks.pool, "UserPromptSubmit", "s1", &fg));
    hooks.send(&bash(
        &hooks,
        "s1",
        None,
        "toolu_t1",
        &cd(&wt1, "wtpool job -- cargo test"),
    ));
    for (number, agent) in ["a1", "a2"].iter().enumerate() {
        hooks.advance(10);
        hooks.send(&event(&hooks.pool, "PreToolUse-Agent", "s1", &fg));
        hooks.send(&of_agent(
            event(&hooks.pool, "SubagentStart", "s1", &fg),
            agent,
        ));
        hooks.advance(10);
        let folder = if number == 0 { &wt2 } else { &wt3 };
        let id = format!("toolu_{agent}");
        hooks.send(&bash(
            &hooks,
            "s1",
            Some(agent),
            &id,
            &cd(folder, "wtpool job -- cargo build"),
        ));
    }
    // A second harness below the first: the job below it is its session's.
    hooks.start_child(7002, CLAUDE, "claude", &fg, at);
    hooks.advance(10);
    send_from(
        &hooks,
        7002,
        &event(&hooks.pool, "UserPromptSubmit", "s2", &fg),
    );
    send_from(
        &hooks,
        7002,
        &bash(
            &hooks,
            "s2",
            None,
            "toolu_s2",
            &cd(&wt3, "wtpool job -- cargo test"),
        ),
    );
    hooks.start_child(7100, CLAUDE, "zsh", &wt1, at);
    let runs: Vec<(&str, &[&str], &Path, u32)> = vec![
        ("shell call of the session", &["cargo", "test"], &wt1, 7100),
        (
            "shell call of the newer subagent",
            &["cargo", "build"],
            &wt2,
            CLAUDE,
        ),
        ("no call of its summary", &["cargo", "check"], &wt1, CLAUDE),
        ("no harness above it", &["cargo", "test"], &x, 1),
        (
            "a folder outside the pool",
            &["cargo", "test"],
            &elsewhere,
            CLAUDE,
        ),
        ("the nearer harness", &["cargo", "test"], &wt3, 7002),
    ];
    let mut jobs = Vec::new();
    for (what, command, cwd, parent) in &runs {
        let (program, args) = command.split_first().expect("setup: a command");
        let (job, record) = started(&hooks, &JobSpec::held(program, args).in_dir(cwd), "running");
        let record = must_have(&job, record, what);
        list_job(&mut hooks, &job, &record, *parent, cwd, command);
        jobs.push((job, record));
    }
    let folder = hooks.sandbox.jobs_dir();
    let dead = "999999-0badf00d";
    let dead_record = json!({
        "version": 1, "id": dead, "pid": 999_999, "started": at, "summary": "cargo test",
        "label": null, "cwd": wt1.to_string_lossy(), "ticket": 1, "state": "running",
        "queued": at, "since": at,
    });
    fs::write(folder.join(format!("{dead}.json")), dead_record.to_string())
        .expect("setup: write a dead job's record");
    fs::write(folder.join(format!("{dead}.lock")), "").expect("setup: write its lock file");
    let before = folder_state(&folder);

    // When
    let document = hooks.status();
    let after = folder_state(&folder);

    // Then
    let running = jobs_of(&document)["running"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let by_id: BTreeMap<String, &Value> = running
        .iter()
        .filter_map(|item| Some((item["id"].as_str()?.to_owned(), item)))
        .collect();
    let path = |p: &Path| json!(p.to_string_lossy());
    let wanted = [
        (path(&wt1), json!("s1")),
        (path(&wt2), json!("s1.a2")),
        (path(&wt1), json!("s1")),
        (path(&x), Value::Null),
        (Value::Null, json!("s1")),
        (path(&wt3), json!("s2")),
    ];
    let mut wrong = Vec::new();
    for (((what, ..), (_, record)), (worktree, actor)) in runs.iter().zip(&jobs).zip(&wanted) {
        let id = record["id"].as_str().unwrap_or_default();
        let found = by_id
            .get(id)
            .map(|item| (&item["worktree"], &item["actor"]));
        if found != Some((worktree, actor)) {
            wrong.push(format!(
                "{what}: {found:?}, not worktree {worktree} and actor {actor}"
            ));
        }
    }
    if by_id.contains_key(dead) {
        wrong.push("the dead job's record is listed as running".to_owned());
    }
    if before != after {
        wrong.push(format!(
            "wtpool status changed the job folder: {before:?} then {after:?}"
        ));
    }
    assert!(
        wrong.is_empty(),
        "{}\nmachine.jobs: {}",
        wrong.join("\n"),
        jobs_of(&document)
    );
}

/// Whether `case` of the decay table is one of A8's, with a job of s1's own.
pub(super) fn is_job_case(case: &str) -> bool {
    case.starts_with("job_")
}

/// Runs one A8 case of `decay__should_match_the_table`, from phase 4: s1's job waits or runs, and
/// s1 shows the state and action of the case 300 s after its events, `None` leaving the action
/// unchecked.
pub(super) fn must_decay_with_a_job(case: &str) {
    if phase() < 4 {
        return;
    }
    // Given
    let mut hooks = Hooks::must_new("decay-job");
    hooks
        .sandbox
        .write_machine("[jobs]\nlimit = 1\nheavy = [\"cargo test\"]\n");
    let (fg, wt1, wt3) = (
        hooks.pool.fg.clone(),
        hooks.pool.worktree("wt1"),
        hooks.pool.worktree("wt3"),
    );
    let waits = !matches!(case, "job_running_past_120_s" | "job_left_the_folder");
    let blocker = waits.then(|| {
        let (job, record) = started(
            &hooks,
            &JobSpec::held("cargo", &["build"]).in_dir(&wt3),
            "running",
        );
        must_have(&job, record, "the job that holds the one place");
        job
    });
    let label = (case == "job_waiting_with_a_label").then_some("unit tests");
    let wrapper = label.map_or_else(
        || "wtpool job --".to_owned(),
        |label| format!("wtpool job --label '{label}' --"),
    );
    let command = format!("cd {} && {wrapper} cargo test", wt1.display());
    hooks.send(&event(&hooks.pool, "UserPromptSubmit", "s1", &fg));
    hooks.send(&bash(&hooks, "s1", None, "toolu_job", &command));
    let mut spec = JobSpec::held("cargo", &["test"]).in_dir(&wt1);
    spec.label = label.map(str::to_owned);
    let (mut job, record) = started(&hooks, &spec, if waits { "waiting" } else { "running" });
    let record = must_have(&job, record, "s1's cargo test");
    list_job(&mut hooks, &job, &record, CLAUDE, &wt1, &["cargo", "test"]);
    if !waits {
        hooks.start_proc(proc(5001, job.pid(), &["cargo", "test"], &wt1, START));
    }
    match case {
        "job_waiting_after_stop" => {
            hooks.send(&event(&hooks.pool, "Stop", "s1", &fg));
        }
        "job_waiting_while_waiting" => {
            hooks.send(&event(&hooks.pool, "PermissionRequest-Bash", "s1", &fg));
        }
        "job_left_the_folder" => {
            assert!(
                job.finish(SETTLE).is_some(),
                "wtpool job did not end with its command"
            );
            hooks.stop(5001);
            hooks.stop(job.pid());
        }
        _ => {}
    }
    let (state, action) = match case {
        "job_waiting_past_120_s" | "job_waiting_after_stop" => (
            "working",
            Some(json!({"tool": "queued:", "target": "cargo test"})),
        ),
        "job_waiting_with_a_label" => (
            "working",
            Some(json!({"tool": "queued:", "target": "unit tests"})),
        ),
        "job_waiting_while_waiting" => ("waiting", None),
        "job_running_past_120_s" => (
            "working",
            Some(json!({"tool": "Bash", "target": "cargo test"})),
        ),
        "job_left_the_folder" => ("quiet", None),
        other => panic!("setup: unknown case {other}"),
    };

    // When
    hooks.set_clock(&add_seconds(START, 300));
    let document = hooks.status();
    drop(blocker);

    // Then
    let actor = document["pools"][0]["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|w| w["actors"].as_array().cloned().unwrap_or_default())
        .find(|a| a["key"] == "s1")
        .unwrap_or(Value::Null);
    assert_eq!(
        actor["state"],
        json!(state),
        "{case}: s1 300 s after its events: {actor}\nmachine.jobs: {}",
        jobs_of(&document)
    );
    if let Some(action) = action {
        assert_eq!(actor["action"], action, "{case}: s1's action: {actor}");
    }
}

/// `value` with every object that lists `properties` closed, as wtpool emits only those (S12).
fn closed(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if map.contains_key("properties") {
                map.insert("additionalProperties".to_owned(), json!(false));
            }
            map.values_mut().for_each(closed);
        }
        Value::Array(items) => items.iter_mut().for_each(closed),
        _ => {}
    }
}

/// Problems of `jobs` against `$defs/jobs` of the status schema with every object closed.
fn closed_jobs_errors(jobs: &Value) -> Vec<String> {
    let path = must_repo().join("verify/schema/status.phase-4.schema.json");
    let text = fs::read_to_string(&path).expect("setup: read the status schema");
    let mut schema: Value = serde_json::from_str(&text).expect("setup: parse the schema");
    closed(&mut schema);
    if let Some(root) = schema.as_object_mut() {
        for key in ["properties", "required", "type", "additionalProperties"] {
            root.remove(key);
        }
        root.insert("$ref".to_owned(), json!("#/$defs/jobs"));
    }
    let validator = jsonschema::validator_for(&schema).expect("setup: compile the schema");
    validator
        .iter_errors(jobs)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect()
}

/// Every key at any depth of `value`.
fn keys(value: &Value, found: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                found.insert(key.clone());
                keys(item, found);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| keys(item, found)),
        _ => {}
    }
}

/// The compared fields of each job or process of a list: id (or whether it is one), summary,
/// label, worktree, actor, and whether `since` is a time.
fn rows(list: &Value, ids: bool) -> Vec<Value> {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|item| {
            json!({
                "id": if ids { item["id"].clone() } else { json!(item["id"].as_str().is_some_and(|id| !id.is_empty())) },
                "summary": item["summary"], "label": item["label"],
                "worktree": item["worktree"], "actor": item["actor"],
                "since": item["since"].as_str().is_some_and(|t| t.ends_with('Z') && t.get(10..11) == Some("T")),
            })
        })
        .collect()
}

fn row(id: Value, summary: &str, label: Value, worktree: &Path) -> Value {
    json!({"id": id, "summary": summary, "label": label, "worktree": worktree.to_string_lossy(), "actor": null, "since": true})
}

#[test]
fn status__should_list_jobs() {
    // Given
    let mut hooks = Hooks::must_new("status-jobs");
    hooks
        .sandbox
        .write_machine("[jobs]\nlimit = 1\nenforce = \"warn\"\nheavy = [\"cargo test\"]\n");
    let (fg, wt1, wt2, wt3, x) = (
        hooks.pool.fg.clone(),
        hooks.pool.worktree("wt1"),
        hooks.pool.worktree("wt2"),
        hooks.pool.worktree("wt3"),
        hooks.pool.worktree("x"),
    );
    let token = "--token=wtpool-verify-SECRETARG";
    let first = JobSpec::held("cargo", &["test", token])
        .in_dir(&wt1)
        .with_env("WTPOOL_VERIFY_SECRET", "wtpool-verify-SECRETENV");
    let (a, a_record) = started(&hooks, &first, "running");
    let a_record = must_have(&a, a_record, "the running cargo test");
    let (b, b_record) = started(
        &hooks,
        &JobSpec::held("cargo", &["test"])
            .in_dir(&wt2)
            .labelled("second"),
        "waiting",
    );
    let b_record = must_have(&b, b_record, "the first waiting cargo test");
    let (c, c_record) = started(
        &hooks,
        &JobSpec::held("cargo", &["test"]).in_dir(&fg),
        "waiting",
    );
    let c_record = must_have(&c, c_record, "the second waiting cargo test");
    for (job, record, cwd) in [
        (&a, &a_record, &wt1),
        (&b, &b_record, &wt2),
        (&c, &c_record, &fg),
    ] {
        list_job(&mut hooks, job, record, 1, cwd, &["cargo", "test"]);
    }
    hooks.start_proc(proc(
        5001,
        a.pid(),
        &["cargo", "test", token],
        &wt1,
        "2026-10-20T11:40:00Z",
    ));
    hooks.start_proc(proc(
        5101,
        1,
        &["cargo", "test"],
        &wt3,
        "2026-10-20T11:00:00Z",
    ));
    hooks.start_proc(proc(
        5102,
        1,
        &["cargo", "test"],
        &x,
        "2026-10-20T11:30:00Z",
    ));
    let base = hooks.base_envs();
    let cores = OsString::from("16");
    let mut envs: Vec<(&str, &OsStr)> = base.iter().map(|(k, v)| (*k, v.as_os_str())).collect();
    envs.push(("WTPOOL_VERIFY_CORES", cores.as_os_str()));

    // When
    let document = must_status_json(&hooks.sandbox, &fg, &envs);
    hooks.sandbox.remove_machine();
    let defaults = must_status_json(&hooks.sandbox, &fg, &envs);
    hooks.sandbox.write_machine("[jobs]\nlimit = 0\n");
    let broken = must_status_json(&hooks.sandbox, &fg, &envs);

    // Then
    let jobs = jobs_of(&document);
    assert_eq!(
        closed_jobs_errors(jobs),
        Vec::<String>::new(),
        "machine.jobs: {jobs}"
    );
    let config = hooks
        .sandbox
        .machine_config()
        .to_string_lossy()
        .into_owned();
    for (field, wanted) in [
        ("limit", json!(1)),
        ("default", json!(4)),
        ("min", json!(1)),
        ("max", json!(64)),
        ("enforce", json!("warn")),
        ("source", json!({"limit": "machine", "enforce": "machine"})),
        ("config", json!(config)),
        ("error", Value::Null),
        (
            "heavy",
            json!([{"pattern": "cargo test", "source": "machine"}]),
        ),
        ("suggested", json!([])),
    ] {
        assert_eq!(jobs[field], wanted, "machine.jobs.{field}");
    }
    assert_eq!(
        rows(&jobs["running"], true),
        vec![row(a_record["id"].clone(), "cargo test", Value::Null, &wt1)],
        "machine.jobs.running"
    );
    assert_eq!(
        rows(&jobs["waiting"], true),
        vec![
            row(b_record["id"].clone(), "cargo test", json!("second"), &wt2),
            row(c_record["id"].clone(), "cargo test", Value::Null, &fg),
        ],
        "machine.jobs.waiting in queue order"
    );
    assert_eq!(
        rows(&jobs["outside"], false),
        vec![
            row(json!(true), "cargo test", Value::Null, &x),
            row(json!(true), "cargo test", Value::Null, &wt3),
        ],
        "machine.jobs.outside, newest first"
    );
    let text = jobs.to_string();
    let mut found = BTreeSet::new();
    keys(jobs, &mut found);
    assert!(
        !text.contains("SECRETARG") && !text.contains("SECRETENV") && !found.contains("pid"),
        "machine.jobs holds an argument, the environment or a pid: {text}"
    );
    let defaulted = jobs_of(&defaults);
    assert_eq!(
        (
            &defaulted["limit"],
            &defaulted["enforce"],
            &defaulted["source"]
        ),
        (
            &json!(4),
            &json!("warn"),
            &json!({"limit": "default", "enforce": "default"})
        ),
        "machine.jobs without machine.toml: {defaulted}"
    );
    let failed = jobs_of(&broken);
    assert!(
        failed["error"].as_str().is_some_and(|e| !e.is_empty()) && failed["limit"].is_null(),
        "machine.jobs with limit = 0 in machine.toml: {failed}"
    );
}
