//! `claude_code__should_stay_byte_identical`: what `wtpool hook` does with every Claude Code
//! payload of the oracle, the hand written fixtures, the recorded sessions and the hook steps of
//! the scenarios, and what `wtpool hook install` prints, equal the goldens of
//! `verify/goldens/claude-code-0.1.0/`, one entry per line, which the base build, wtpool 0.1.0, made,
//! but for `model`.
//!
//! A golden entry holds, after one payload, the hook's exit code and stdout, the lines it added to
//! its log, the decision of `wtpool hook policy` for the same payload, each record's text and,
//! but for a scenario's steps, whose own documents judge them, the agents of
//! `wtpool status --json`. The pool's path reads `{pool}`, the sandbox's `{sandbox}`, a record's
//! `writer_started`, the real start of its writer, `0`, and an input hash `{hash}`. A scenario's
//! step holds only the records it changed and the names of those it removed. With
//! `WTPOOL_VERIFY_WRITE_GOLDENS` naming a folder, the check writes the goldens there instead.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use xtask::scenario::{ProcessStep, Step};

use super::install::installed_copy;
use crate::support::hook::{CLAUDE, Hooks, Proc, RunOptions, fixture};
use crate::support::jobs::policy::machine;
use crate::support::{must_repo, must_status_json};

const GOLDENS: &str = "verify/goldens/claude-code-0.1.0";
const WRITE: &str = "WTPOOL_VERIFY_WRITE_GOLDENS";
/// Heavy patterns that recorded and hand written shell calls match, so the policy decides.
const HEAVY: [&str; 4] = ["cargo test", "git status", "ls", "npm test"];

/// `text` with the sandbox's paths as placeholders and each `writer_started` as `0`, without
/// `model`.
fn normalised(text: &str, hooks: &Hooks) -> String {
    let pool = hooks.pool.dir.to_string_lossy().into_owned();
    let sandbox = hooks.sandbox.root.to_string_lossy().into_owned();
    let mut text = text.replace(&pool, "{pool}").replace(&sandbox, "{sandbox}");
    text = replace_number(&text, "\"writer_started\":");
    text = replace_hashes(&text);
    without_model(&text)
}

/// `text` with each input hash, `"match":"<64 hex digits>"`, as `{hash}`: an input that names a
/// path of the sandbox hashes to another value in every run.
fn replace_hashes(text: &str) -> String {
    let key = "\"match\":\"";
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(key) {
        let (before, after) = rest.split_at(at + key.len());
        out.push_str(before);
        let digits = after.len()
            - after
                .trim_start_matches(|c: char| c.is_ascii_hexdigit())
                .len();
        if digits == 64 {
            out.push_str("{hash}");
            rest = after.get(digits..).unwrap_or_default();
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn replace_number(text: &str, key: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(key) {
        let (before, after) = rest.split_at(at + key.len());
        out.push_str(before);
        let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        out.push('0');
        rest = after.get(digits..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

/// `text` without its `"model":"…"` members.
fn without_model(text: &str) -> String {
    let mut out = text.to_owned();
    while let Some(at) = out.find("\"model\":\"") {
        let value = at + "\"model\":\"".len();
        let mut end = value;
        let bytes = out.as_bytes();
        while end < bytes.len() && bytes.get(end) != Some(&b'"') {
            end += if bytes.get(end) == Some(&b'\\') { 2 } else { 1 };
        }
        let mut start = at;
        let mut stop = (end + 1).min(out.len());
        if out.get(stop..stop + 1) == Some(",") {
            stop += 1;
        } else if start > 0 && out.get(start - 1..start) == Some(",") {
            start -= 1;
        }
        out.replace_range(start..stop, "");
    }
    out
}

/// The record files of the activity folder, by name, as normalised text.
fn records(hooks: &Hooks) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for entry in fs::read_dir(hooks.activity())
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".json") {
            let text = fs::read_to_string(entry.path()).unwrap_or_default();
            found.insert(name, normalised(&text, hooks));
        }
    }
    found
}

/// The agents of `wtpool status --json`: each worktree's name, activity and actors.
fn agents(hooks: &Hooks) -> Value {
    let envs = hooks.base_envs();
    let envs: Vec<(&str, &std::ffi::OsStr)> =
        envs.iter().map(|(k, v)| (*k, v.as_os_str())).collect();
    let document = must_status_json(&hooks.sandbox, &hooks.pool.fg, &envs);
    let pool = &document["pools"][0];
    let worktrees: Vec<Value> = pool["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|w| json!({"name": w["name"], "activity": w["activity"], "actors": w["actors"]}))
        .collect();
    let value = json!({"skipped_records": pool["skipped_records"], "worktrees": worktrees});
    serde_json::from_str(&normalised(&value.to_string(), hooks)).unwrap_or(value)
}

/// One payload through `wtpool hook` and `wtpool hook policy`, and what they left.
fn entry(hooks: &Hooks, item: &str, payload: &Value, options: &RunOptions) -> Value {
    let mut found = step_entry(hooks, item, payload, options);
    found["agents"] = agents(hooks);
    found
}

/// [`entry`] but the agents, which a scenario's own documents judge.
fn step_entry(hooks: &Hooks, item: &str, payload: &Value, options: &RunOptions) -> Value {
    let lines = hooks.log_lines();
    let run = hooks.run_with(payload, options);
    let logged: Vec<String> = fs::read_to_string(hooks.hook_log())
        .unwrap_or_default()
        .lines()
        .skip(lines)
        .map(|line| normalised(line, hooks))
        .collect();
    let mut policy_options = options.clone();
    policy_options.args = vec!["policy".to_owned()];
    let policy = hooks.run_with(payload, &policy_options);
    json!({
        "item": item,
        "code": run.code(),
        "stdout": normalised(&String::from_utf8_lossy(&run.output.stdout), hooks),
        "log": logged,
        "policy": {
            "code": policy.code(),
            "stdout": normalised(&String::from_utf8_lossy(&policy.output.stdout), hooks),
        },
        "records": records(hooks),
    })
}

fn clear_activity(hooks: &Hooks) {
    for entry in fs::read_dir(hooks.activity())
        .into_iter()
        .flatten()
        .flatten()
    {
        if entry.file_name() != ".verify-sandbox" {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn fixture_entries() -> Vec<Value> {
    let hooks = Hooks::must_new("cc-identical-fixtures");
    hooks.sandbox.write_machine(&machine("require", &HEAVY));
    xtask::hooks::fixture_names(&must_repo())
        .into_iter()
        .map(|name| {
            clear_activity(&hooks);
            let payload = fixture(&hooks.pool, &name);
            entry(&hooks, &name, &payload, &RunOptions::default())
        })
        .collect()
}

fn recording_entries() -> Vec<Value> {
    let root = must_repo();
    let recorded = xtask::hooks::recorded_payloads(&root).unwrap_or_else(|e| panic!("setup: {e}"));
    let mut sessions: BTreeMap<String, Vec<(String, Value)>> = BTreeMap::new();
    for (name, payload) in recorded {
        let session = name.split(':').next().unwrap_or_default().to_owned();
        sessions.entry(session).or_default().push((name, payload));
    }
    let mut found = Vec::new();
    for payloads in sessions.into_values() {
        let hooks = Hooks::must_new("cc-identical-recordings");
        hooks.sandbox.write_machine(&machine("require", &HEAVY));
        for (name, mut payload) in payloads {
            xtask::pool::resolve_placeholders(&mut payload, &hooks.pool);
            hooks.advance(1);
            found.push(entry(&hooks, &name, &payload, &RunOptions::default()));
        }
    }
    found
}

/// The hook steps of every scenario of phase 4 or before that starts no job, a few scenarios at
/// once.
fn scenario_entries() -> Vec<Value> {
    let root = must_repo();
    let names: Vec<String> = xtask::scenario::names(&root)
        .into_iter()
        .filter(|name| {
            let scenario =
                xtask::scenario::load(&root, name).unwrap_or_else(|e| panic!("setup: {e}"));
            let runs_jobs = scenario.steps.iter().any(|s| matches!(s, Step::Job(_)));
            scenario.phase <= 4 && !runs_jobs && scenario.has_hook_steps()
        })
        .collect();
    let chunk = names.len().div_ceil(4).max(1);
    std::thread::scope(|scope| {
        let workers: Vec<_> = names
            .chunks(chunk)
            .map(|names| {
                scope.spawn(move || {
                    names
                        .iter()
                        .flat_map(|name| scenario_steps(name))
                        .collect::<Vec<Value>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|_| panic!("a scenario's replay panicked"))
            })
            .collect()
    })
}

/// The entries of the hook steps of the scenario `name`, each with the records the step changed
/// and the names of those it removed.
fn scenario_steps(name: &str) -> Vec<Value> {
    let root = must_repo();
    let scenario = xtask::scenario::load(&root, name).unwrap_or_else(|e| panic!("setup: {e}"));
    let mut hooks = Hooks::must_with(name, &scenario.pool);
    hooks.stop(CLAUDE);
    hooks.set_clock(xtask::scenario::HOOK_CLOCK);
    let mut found = Vec::new();
    let mut last: BTreeMap<String, String> = BTreeMap::new();
    for (index, step) in scenario.steps.iter().enumerate() {
        match step {
            Step::Process(ProcessStep::Start(process)) => hooks.start_proc(Proc {
                pid: process.pid,
                ppid: process.parent.unwrap_or(1),
                name: process.name.clone(),
                cwd: hooks.pool.worktree(&process.worktree),
                started: process
                    .started
                    .clone()
                    .unwrap_or_else(|| xtask::scenario::PROCESS_STARTED.to_owned()),
                cmd: process.cmd.clone(),
            }),
            Step::Process(ProcessStep::Stop(pid)) => hooks.stop(*pid),
            Step::Clock(time) => hooks.set_clock(time),
            Step::Machine(text) => hooks.sandbox.write_machine(text),
            Step::Gc(_) => {
                let _ = hooks.gc();
            }
            Step::Hook(hook) => {
                hooks.advance(1);
                let payload = xtask::hooks::payload(&root, hook, &hooks.pool)
                    .unwrap_or_else(|e| panic!("setup: {name} step {index}: {e}"));
                let mut options = RunOptions {
                    parent: Some(hook.process),
                    cwd: Some(xtask::hooks::step_folder(&hook.cwd, &hooks.pool)),
                    ..RunOptions::default()
                };
                if let Some(home) = &hook.home {
                    let home = xtask::hooks::step_folder(home, &hooks.pool);
                    options
                        .envs
                        .push(("HOME".to_owned(), home.into_os_string()));
                }
                let mut entry =
                    step_entry(&hooks, &format!("{name} step {index}"), &payload, &options);
                let now: BTreeMap<String, String> =
                    serde_json::from_value(entry["records"].take()).unwrap_or_default();
                let changed: BTreeMap<&String, &String> = now
                    .iter()
                    .filter(|(file, text)| last.get(*file) != Some(*text))
                    .collect();
                let gone: Vec<&String> = last
                    .keys()
                    .filter(|file| !now.contains_key(*file))
                    .collect();
                entry["records"] = json!(changed);
                entry["removed"] = json!(gone);
                found.push(entry);
                last = now;
            }
            _ => {}
        }
    }
    found
}

/// `wtpool hook install` from a copy outside `target`, with no `wtpool` on `PATH`, before and
/// after its block is in `~/.claude/settings.json`.
fn install_entries() -> Vec<Value> {
    let hooks = Hooks::must_new("cc-identical-install");
    let binary = installed_copy(&hooks.sandbox, "bin");
    let bin = binary.to_string_lossy().into_owned();
    let run = || {
        hooks
            .sandbox
            .command(&binary, &hooks.sandbox.root.join("home"))
            .env("PATH", "/usr/bin:/bin")
            .args(["hook", "install"])
            .output()
            .expect("setup: start wtpool")
    };
    let printed = |output: &std::process::Output| {
        let text = |bytes: &[u8]| {
            normalised(
                &String::from_utf8_lossy(bytes).replace(&bin, "{bin}"),
                &hooks,
            )
        };
        json!({"code": output.status.code(), "stdout": text(&output.stdout), "stderr": text(&output.stderr)})
    };
    let without = run();
    let settings = hooks.sandbox.root.join("home/.claude/settings.json");
    fs::create_dir_all(settings.parent().expect("setup: ~/.claude")).expect("setup: ~/.claude");
    fs::write(&settings, &without.stdout).expect("setup: write settings.json");
    let with = run();
    vec![
        json!({"item": "install without settings.json", "printed": printed(&without)}),
        json!({"item": "install with its block", "printed": printed(&with)}),
    ]
}

fn golden_path(group: &str) -> PathBuf {
    must_repo().join(GOLDENS).join(format!("{group}.jsonl"))
}

/// The items of `actual` that differ from the golden `group`, or none in the writing mode.
fn compare(group: &str, actual: &[Value]) -> Vec<String> {
    if let Some(dir) = std::env::var_os(WRITE) {
        let path = Path::new(&dir).join(format!("{group}.jsonl"));
        let text: String = actual.iter().map(|entry| format!("{entry}\n")).collect();
        fs::write(&path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        return Vec::new();
    }
    let path = golden_path(group);
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("setup: {}: {e}", path.display()));
    let golden: Vec<Value> = text
        .lines()
        .map(|line| {
            serde_json::from_str(line).unwrap_or_else(|e| panic!("setup: {}: {e}", path.display()))
        })
        .collect();
    let mut found = Vec::new();
    if golden.len() != actual.len() {
        found.push(format!(
            "{group}: {} entries, the golden holds {}",
            actual.len(),
            golden.len()
        ));
    }
    for (want, have) in golden.iter().zip(actual) {
        if want != have {
            found.push(format!("{group} {}: was {want}\nnow {have}", want["item"]));
        }
    }
    found
}

/// The entries of one group of goldens.
type Entries = fn() -> Vec<Value>;

#[test]
fn claude_code__should_stay_byte_identical() {
    // Given
    let groups: [(&str, Entries); 4] = [
        ("fixtures", fixture_entries),
        ("recordings", recording_entries),
        ("scenarios", scenario_entries),
        ("install", install_entries),
    ];

    // When
    let differing: Vec<String> = std::thread::scope(|scope| {
        let workers: Vec<_> = groups
            .iter()
            .map(|(group, entries)| scope.spawn(move || compare(group, &entries())))
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|_| panic!("a group's replay panicked"))
            })
            .collect()
    });

    // Then
    assert!(
        differing.is_empty(),
        "Claude Code's outputs differ from the base build's:\n{}",
        differing
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}
