//! `wtpool hook install`: the block it prints (H1, B2), where it refuses to run, and what it says
//! about `~/.claude/settings.json`, which it never writes (H22, D5).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::{
    FileState, Sandbox, file_state, must_release_bin, must_wtpool_bin, set_executable,
};

/// The event table of status-panel.md, section 1.
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

/// A copy of the verify build outside any `target` folder, which install may run from.
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

/// The printed block as a map from event to its hook entries, whether it prints
/// `{"hooks": {...}}` or the inner object.
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
    for (event, entries) in &events {
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

#[test]
fn budget__b2() {
    // Given
    let sandbox = Sandbox::must_new("budget-b2");
    let binary = installed_copy(&sandbox, "bin");

    // When
    let run = install(&sandbox, &binary);

    // Then
    let events = must_block(&run);
    let not_async: Vec<&str> = ASYNC
        .iter()
        .copied()
        .filter(|event| {
            events.get(*event).is_none_or(|entries| {
                entries.is_empty() || entries.iter().any(|e| e["async"] != json!(true))
            })
        })
        .collect();
    assert!(
        not_async.is_empty(),
        "events of H1 not async in the printed block: {not_async:?}"
    );
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

/// Size, mtime and mode of every entry below `dir`.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, FileState> {
    let mut found = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path.clone());
            }
            found.insert(path, file_state(&meta));
        }
    }
    found
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

#[test]
fn install__should_never_write_settings() {
    // Given
    let sandbox = Sandbox::must_new("install-never-writes");
    let binary = installed_copy(&sandbox, "bin");
    let home = sandbox.root.join("home");

    // When
    let without = snapshot(&home);
    let first = install(&sandbox, &binary);
    let still_without = snapshot(&home);
    write_settings(
        &sandbox,
        &json!({ "permissions": { "allow": ["Bash(ls)"] } }),
    );
    let other = snapshot(&home);
    let _ = install(&sandbox, &binary);
    let still_other = snapshot(&home);
    let block = first_object(&first.stdout).map(|_| pasted(&first));
    if let Some(block) = &block {
        write_settings(&sandbox, block);
    }
    let with_block = snapshot(&home);
    let _ = install(&sandbox, &binary);
    let still_with_block = snapshot(&home);

    // Then
    assert_eq!(first.code, Some(0), "wtpool hook install: {}", first.text);
    assert!(
        block.is_some(),
        "wtpool hook install printed no block: {}",
        first.text
    );
    assert_eq!(
        without, still_without,
        "install changed the home without ~/.claude"
    );
    assert_eq!(
        other, still_other,
        "install changed settings.json without the block"
    );
    assert_eq!(
        with_block, still_with_block,
        "install changed settings.json with the block"
    );
}

/// H22: the first line of the note says `hooks present:` exactly when every event runs this
/// `wtpool hook`, and says the same for the block alone and among other settings.
#[test]
fn install__should_report_presence() {
    // Given
    let sandbox = Sandbox::must_new("install-presence");
    let binary = installed_copy(&sandbox, "bin");
    let absent_run = install(&sandbox, &binary);
    let block = pasted(&absent_run);
    let mut partial = block.clone();
    if let Some(hooks) = partial["hooks"].as_object_mut() {
        hooks.remove("SessionEnd");
    }
    let mut merged = block.clone();
    merged["permissions"] = json!({ "allow": ["Bash(ls)"] });
    if let Some(list) = merged["hooks"]["PreToolUse"].as_array_mut() {
        list.push(json!({ "matcher": "Bash", "hooks": [{ "type": "command", "command": "/usr/bin/true" }] }));
    }
    let other_binary = serde_json::from_str::<Value>(&block.to_string().replace(
        &binary.to_string_lossy().to_string(),
        "/opt/elsewhere/wtpool",
    ))
    .expect("setup: JSON");
    let cases: Vec<(&str, Option<Value>, bool)> = vec![
        ("no file", None, false),
        ("other settings", Some(json!({ "permissions": {} })), false),
        ("a block without SessionEnd", Some(partial), false),
        ("the block of another wtpool", Some(other_binary), false),
        ("the block", Some(block.clone()), true),
        ("the block among other settings", Some(merged), true),
    ];

    // When
    let said: Vec<(&str, bool, String)> = cases
        .into_iter()
        .map(|(what, value, present)| {
            let _ = fs::remove_file(settings(&sandbox));
            if let Some(value) = &value {
                write_settings(&sandbox, value);
            }
            (what, present, install(&sandbox, &binary).stderr)
        })
        .collect();

    // Then
    let mut wrong = Vec::new();
    for (what, present, stderr) in &said {
        let prefix = if *present {
            "hooks present: "
        } else {
            "hooks missing: "
        };
        if !stderr.starts_with(prefix) {
            wrong.push(format!(
                "{what}: the note does not start {prefix:?}: {stderr:?}"
            ));
        }
    }
    let present: Vec<&(&str, bool, String)> = said.iter().filter(|(_, p, _)| *p).collect();
    if let [first, second] = present.as_slice()
        && first.2 != second.2
    {
        wrong.push(format!(
            "install says different things for \"{}\" and \"{}\": {:?}, {:?}",
            first.0, second.0, first.2, second.2
        ));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The block of H1 for `command`, as `wtpool hook install` prints it: `{"hooks": {event: [...]}}`.
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
    json!({ "hooks": hooks })
}

/// H22: stdout is the block alone, as one JSON value, so `wtpool hook install > hooks.json` is a
/// valid settings file (Claude Code skips a malformed `--settings` file without a word); the
/// presence note is on stderr and holds no JSON. The release wtpool, which users run.
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
        .map(|p| expected_block(&p.to_string_lossy()))
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
            Ok(value) if !candidates.contains(&value) => {
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
