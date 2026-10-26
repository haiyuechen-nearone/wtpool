//! `wtpool hook install`: the block it prints, where it refuses to run, and its note on
//! `~/.claude/settings.json`. The checks of that note, and that install never writes the file, are
//! in [`note`].

mod note;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::{Sandbox, must_release_bin, must_wtpool_bin, set_executable};

/// The events the printed block runs `wtpool hook` on.
pub(super) const EVENTS: [&str; 14] = [
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionDenied",
    "PostToolBatch",
    "Notification",
    "Stop",
    "StopFailure",
    "SubagentStart",
    "SubagentStop",
    "CwdChanged",
    "SessionEnd",
];
const ASYNC: [&str; 5] = [
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionDenied",
    "PostToolBatch",
];

/// The arguments of the policy entry.
pub(super) const POLICY_ARGS: [&str; 2] = ["hook", "policy"];
/// The matcher of the policy entry's group.
pub(super) const POLICY_MATCHER: &str = "Bash|PowerShell|Monitor";

/// True when `WTPOOL_VERIFY_PHASE` is 4 or more, as the block then also holds the policy entry.
pub(super) fn has_policy() -> bool {
    crate::support::phase() >= 4
}

pub(super) fn is_policy(entry: &Value) -> bool {
    entry["args"] == json!(POLICY_ARGS)
}

/// The matcher of each group of the block's `PreToolUse` that holds the policy entry.
pub(super) fn policy_matchers(block: &Value) -> Vec<Value> {
    let inner = block
        .get("hooks")
        .filter(|h| h.is_object())
        .unwrap_or(block);
    inner["PreToolUse"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|group| {
            group["hooks"]
                .as_array()
                .is_some_and(|hooks| hooks.iter().any(is_policy))
        })
        .map(|group| group["matcher"].clone())
        .collect()
}

/// A copy of the verify build outside any `target` folder, so install may run from it.
pub(super) fn installed_copy(sandbox: &Sandbox, folder: &str) -> PathBuf {
    copy_of(sandbox, folder, &must_wtpool_bin())
}

fn copy_of(sandbox: &Sandbox, folder: &str, source: &Path) -> PathBuf {
    let dir = sandbox.root.join(folder);
    fs::create_dir_all(&dir).expect("setup: create the bin folder");
    let copy = dir.join("wtpool");
    fs::copy(source, &copy).expect("setup: copy wtpool");
    set_executable(&copy);
    copy
}

pub(super) struct Install {
    pub(super) code: Option<i32>,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) text: String,
}

pub(super) fn install(sandbox: &Sandbox, binary: &Path) -> Install {
    let output = sandbox.run(binary, &["hook", "install"]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Install {
        code: output.status.code(),
        text: format!("{stdout}\n--- stderr\n{stderr}"),
        stdout,
        stderr,
    }
}

/// The first JSON object in `text`.
fn first_object(text: &str) -> Option<Value> {
    text.match_indices('{').find_map(|(at, _)| {
        let mut stream = serde_json::Deserializer::from_str(text.get(at..)?).into_iter::<Value>();
        match stream.next() {
            Some(Ok(value)) if value.is_object() => Some(value),
            _ => None,
        }
    })
}

/// The printed block as a map from event to its hook entries, from either `{"hooks": {...}}` or
/// the inner object.
pub(super) fn events_of(block: &Value) -> BTreeMap<String, Vec<Value>> {
    let inner = block
        .get("hooks")
        .filter(|h| h.is_object())
        .unwrap_or(block);
    let mut found = BTreeMap::new();
    for (event, groups) in inner.as_object().into_iter().flatten() {
        let mut entries = Vec::new();
        collect_entries(groups, &mut entries);
        found.insert(event.clone(), entries);
    }
    found
}

/// Every object below `value` that names a `command`.
fn collect_entries(value: &Value, found: &mut Vec<Value>) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| collect_entries(item, found)),
        Value::Object(map) if map.contains_key("command") => found.push(value.clone()),
        Value::Object(map) => map.values().for_each(|item| collect_entries(item, found)),
        _ => {}
    }
}

fn must_block(run: &Install) -> BTreeMap<String, Vec<Value>> {
    let block = first_object(&run.stdout)
        .unwrap_or_else(|| panic!("wtpool hook install printed no JSON block: {}", run.text));
    events_of(&block)
}

#[test]
fn install__should_match_the_event_table() {
    // Given
    let sandbox = Sandbox::must_new("install-table");
    let binary = installed_copy(&sandbox, "bin");
    let paths: Vec<String> = [
        binary.clone(),
        fs::canonicalize(&binary).expect("setup: canonical"),
    ]
    .iter()
    .map(|p| p.to_string_lossy().into_owned())
    .collect();

    // When
    let run = install(&sandbox, &binary);

    // Then
    assert_eq!(run.code, Some(0), "wtpool hook install: {}", run.text);
    let events = must_block(&run);
    let names: Vec<&str> = events.keys().map(String::as_str).collect();
    let mut expected: Vec<&str> = EVENTS.to_vec();
    expected.sort_unstable();
    assert_eq!(names, expected, "the events of the printed block");
    let mut wrong = Vec::new();
    if has_policy() {
        wrong.extend(policy_problems(&run, &events, &paths));
    }
    for (event, entries) in &events {
        let entries: Vec<&Value> = entries
            .iter()
            .filter(|entry| !(has_policy() && event == "PreToolUse" && is_policy(entry)))
            .collect();
        let [entry] = entries.as_slice() else {
            wrong.push(format!("{event}: {} entries, not 1", entries.len()));
            continue;
        };
        let command = entry["command"].as_str().unwrap_or_default();
        if !paths.iter().any(|p| p == command) {
            wrong.push(format!("{event}: command {command:?}, not {paths:?}"));
        }
        if entry["args"] != json!(["hook"]) {
            wrong.push(format!("{event}: args {}", entry["args"]));
        }
        if entry["onFailure"] != json!("continue") {
            wrong.push(format!("{event}: onFailure {}", entry["onFailure"]));
        }
        let is_async = ASYNC.contains(&event.as_str());
        if is_async && entry["async"] != json!(true) {
            wrong.push(format!("{event}: async {}, not true", entry["async"]));
        }
        if !is_async {
            if entry.get("async").is_some_and(|a| *a != json!(false)) {
                wrong.push(format!(
                    "{event}: a sync event has async {}",
                    entry["async"]
                ));
            }
            let timeout = entry.get("timeout");
            if event == "SessionEnd" {
                if timeout.is_some() {
                    wrong.push(format!("SessionEnd has a timeout key: {timeout:?}"));
                }
            } else if timeout != Some(&json!(2)) {
                wrong.push(format!("{event}: timeout {timeout:?}, not 2"));
            }
        }
    }
    assert!(wrong.is_empty(), "the printed block:\n{}", wrong.join("\n"));
}

/// The faults of the policy entry. It must be one sync `PreToolUse` entry running `wtpool hook
/// policy` with the timeout 2, in a group of the matcher [`POLICY_MATCHER`], and no other event
/// may hold it.
fn policy_problems(
    run: &Install,
    events: &BTreeMap<String, Vec<Value>>,
    paths: &[String],
) -> Vec<String> {
    let mut wrong = Vec::new();
    let policies: Vec<(&String, &Value)> = events
        .iter()
        .flat_map(|(event, entries)| entries.iter().map(move |entry| (event, entry)))
        .filter(|(_, entry)| is_policy(entry))
        .collect();
    let [(event, entry)] = policies.as_slice() else {
        wrong.push(format!(
            "{} policy entries (args {POLICY_ARGS:?}), not 1 under PreToolUse: {policies:?}",
            policies.len()
        ));
        return wrong;
    };
    if event.as_str() != "PreToolUse" {
        wrong.push(format!("the policy entry is under {event}, not PreToolUse"));
    }
    let command = entry["command"].as_str().unwrap_or_default();
    if !paths.iter().any(|p| p == command) {
        wrong.push(format!(
            "the policy entry: command {command:?}, not {paths:?}"
        ));
    }
    if entry.get("async").is_some_and(|a| *a != json!(false)) {
        wrong.push(format!("the policy entry is async: {}", entry["async"]));
    }
    if entry.get("timeout") != Some(&json!(2)) {
        wrong.push(format!(
            "the policy entry: timeout {:?}, not 2",
            entry.get("timeout")
        ));
    }
    if entry["onFailure"] != json!("continue") {
        wrong.push(format!(
            "the policy entry: onFailure {}",
            entry["onFailure"]
        ));
    }
    if entry["type"] != json!("command") {
        wrong.push(format!("the policy entry: type {}", entry["type"]));
    }
    let block = first_object(&run.stdout).unwrap_or_default();
    let matchers = policy_matchers(&block);
    if matchers != [json!(POLICY_MATCHER)] {
        wrong.push(format!(
            "the policy entry's group matchers are {matchers:?}, not [{POLICY_MATCHER:?}]"
        ));
    }
    wrong
}

#[test]
fn budget__b2() {
    // Given
    let sandbox = Sandbox::must_new("budget-b2");
    let binary = installed_copy(&sandbox, "bin");

    // When
    let run = install(&sandbox, &binary);

    // Then
    let events = must_block(&run);
    // The policy entry of PreToolUse is sync. The five events' own entries stay async.
    let hook_entries = |entries: &Vec<Value>| -> Vec<Value> {
        entries
            .iter()
            .filter(|entry| !(has_policy() && is_policy(entry)))
            .cloned()
            .collect()
    };
    let not_async: Vec<&str> = ASYNC
        .iter()
        .copied()
        .filter(|event| {
            events.get(*event).map(hook_entries).is_none_or(|entries| {
                entries.is_empty() || entries.iter().any(|e| e["async"] != json!(true))
            })
        })
        .collect();
    assert!(
        not_async.is_empty(),
        "events of H1 not async in the printed block: {not_async:?}"
    );
    if has_policy() {
        let policies: Vec<&Value> = events
            .get("PreToolUse")
            .into_iter()
            .flatten()
            .filter(|entry| is_policy(entry))
            .collect();
        assert!(
            !policies.is_empty()
                && policies
                    .iter()
                    .all(|e| e.get("async").is_none_or(|a| *a == json!(false))),
            "the policy entry of PreToolUse is not in the block or not sync (B2, from phase 4): {policies:?}"
        );
    }
}

#[rstest]
#[case::the_build_folder(None)]
#[case::a_target_folder_elsewhere(Some("x/target/release"))]
fn install__should_refuse_a_binary_under_target(#[case] folder: Option<&str>) {
    // Given
    let sandbox = Sandbox::must_new("install-target");
    let binary = folder.map_or_else(must_wtpool_bin, |f| installed_copy(&sandbox, f));
    let allowed = installed_copy(&sandbox, "bin");

    // When
    let refused = install(&sandbox, &binary);
    let accepted = install(&sandbox, &allowed);

    // Then
    assert_eq!(
        accepted.code,
        Some(0),
        "wtpool hook install outside target: {}",
        accepted.text
    );
    assert_ne!(
        refused.code,
        Some(0),
        "wtpool hook install ran from {}",
        binary.display()
    );
    let printed = first_object(&refused.stdout)
        .map(|b| events_of(&b))
        .unwrap_or_default();
    assert!(
        printed
            .keys()
            .all(|event| !EVENTS.contains(&event.as_str())),
        "wtpool hook install under target printed a hook block: {}",
        refused.text
    );
}

pub(super) fn settings(sandbox: &Sandbox) -> PathBuf {
    sandbox.root.join("home/.claude/settings.json")
}

pub(super) fn write_settings(sandbox: &Sandbox, value: &Value) {
    let path = settings(sandbox);
    fs::create_dir_all(path.parent().expect("setup: a parent")).expect("setup: create ~/.claude");
    fs::write(
        &path,
        serde_json::to_string_pretty(value).expect("setup: JSON"),
    )
    .expect("setup: write settings.json");
}

/// `{"hooks": ...}` from the printed block, as a user pastes it.
pub(super) fn pasted(run: &Install) -> Value {
    let block = first_object(&run.stdout)
        .unwrap_or_else(|| panic!("wtpool hook install printed no JSON block: {}", run.text));
    if block.get("hooks").is_some_and(Value::is_object) {
        block
    } else {
        json!({ "hooks": block })
    }
}

/// The block for `command`, as `wtpool hook install` prints it: `{"hooks": {event: [...]}}`.
fn expected_block(command: &str) -> Value {
    let mut hooks = serde_json::Map::new();
    for event in EVENTS {
        let mut entry = json!({
            "type": "command",
            "command": command,
            "args": ["hook"],
            "onFailure": "continue",
        });
        if ASYNC.contains(&event) {
            entry["async"] = json!(true);
        } else if event != "SessionEnd" {
            entry["timeout"] = json!(2);
        }
        hooks.insert(event.to_owned(), json!([{ "hooks": [entry] }]));
    }
    if has_policy()
        && let Some(groups) = hooks.get_mut("PreToolUse").and_then(Value::as_array_mut)
    {
        groups.push(json!({
            "matcher": POLICY_MATCHER,
            "hooks": [{
                "type": "command",
                "command": command,
                "args": POLICY_ARGS,
                "timeout": 2,
                "onFailure": "continue",
            }],
        }));
    }
    json!({ "hooks": hooks })
}

/// `block` with the groups of each event in a fixed order, as the policy entry's group may stand
/// anywhere among `PreToolUse`'s.
fn groups_sorted(block: &Value) -> Value {
    let mut sorted = block.clone();
    if let Some(hooks) = sorted["hooks"].as_object_mut() {
        for groups in hooks.values_mut() {
            if let Some(list) = groups.as_array_mut() {
                list.sort_by_key(Value::to_string);
            }
        }
    }
    sorted
}

/// Stdout is the block alone, as one JSON value, so `wtpool hook install > hooks.json` is a valid
/// settings file. Claude Code skips a malformed `--settings` file without a word. The presence
/// note is on stderr and holds no JSON. The check runs the release wtpool, as users do.
#[test]
fn install__should_print_only_the_block_on_stdout() {
    // Given
    let sandbox = Sandbox::must_new("install-stdout");
    let binary = copy_of(&sandbox, "bin", &must_release_bin());
    let command = [
        binary.clone(),
        fs::canonicalize(&binary).expect("setup: canonical"),
    ];
    let candidates: Vec<Value> = command
        .iter()
        .map(|p| groups_sorted(&expected_block(&p.to_string_lossy())))
        .collect();
    let absent = install(&sandbox, &binary);
    let block = serde_json::from_str::<Value>(&absent.stdout).unwrap_or(Value::Null);
    write_settings(&sandbox, &block);

    // When
    let present = install(&sandbox, &binary);

    // Then
    let mut problems = Vec::new();
    for (case, run) in [
        ("no block in settings.json", &absent),
        ("the block in settings.json", &present),
    ] {
        if run.code != Some(0) {
            problems.push(format!("{case}: exit {:?}: {}", run.code, run.text));
        }
        match serde_json::from_str::<Value>(&run.stdout) {
            Err(error) => problems.push(format!(
                "{case}: stdout is not one JSON value ({error}); it ends: {:?}",
                run.stdout
                    .chars()
                    .rev()
                    .take(160)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<String>()
            )),
            Ok(value) if !candidates.contains(&groups_sorted(&value)) => {
                problems.push(format!("{case}: stdout is not the block of H1: {value}"));
            }
            Ok(_) => {}
        }
        if run.stderr.trim().is_empty() {
            problems.push(format!("{case}: stderr holds no presence note"));
        }
        if run.stderr.contains(['{', '}']) {
            problems.push(format!("{case}: stderr holds JSON: {:?}", run.stderr));
        }
    }
    if absent.stderr == present.stderr {
        problems.push(format!(
            "stderr says the same whether settings.json holds the block or not: {:?}",
            absent.stderr
        ));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
