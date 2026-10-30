//! Hook payloads of the oracle: the fixture corpus of `verify/fixtures/hooks/` and the payload a
//! scenario's `hook` step sends to `wtpool hook`.
//!
//! The corpus is written by hand from the hook input of Claude Code 2.1.295, one file per event
//! and tool, with `{pool}` standing for the pool folder. The [scrubbed](crate::scrub) recordings
//! of a live contract run go to `verify/fixtures/claude-<version>/` and join the corpus of the
//! checks that run every fixture. OpenCode's recordings are in `verify/fixtures/opencode-<version>/`,
//! and the payloads its plugin forwards from them, in `forwarded/`, are its corpus.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::pool::{Pool, resolve_placeholders};
use crate::scenario::HookStep;

pub const FIXTURES: &str = "verify/fixtures/hooks";

/// The folder of the scrubbed recordings, one folder per Claude Code version.
pub const RECORDED: &str = "verify/fixtures";

/// The names of the hand written fixtures, sorted, without `.json`.
pub fn fixture_names(root: &Path) -> Vec<String> {
    json_names(&root.join(FIXTURES))
}

fn json_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let path = entry.path();
                    (path.extension().and_then(|e| e.to_str()) == Some("json"))
                        .then(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
                        .flatten()
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// A hand written fixture, placeholders unresolved.
pub fn load_fixture(root: &Path, name: &str) -> Result<Value, String> {
    let path = root.join(FIXTURES).join(format!("{name}.json"));
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if !value.is_object() {
        return Err(format!("{}: a fixture is a JSON object", path.display()));
    }
    Ok(value)
}

/// The start of the folder names in [`RECORDED`] of Claude Code's recordings, one per version.
pub const CLAUDE_CODE_RECORDINGS: &str = "claude-";
/// The start of the folder names in [`RECORDED`] of OpenCode's recordings, one per version.
pub const OPENCODE_RECORDINGS: &str = "opencode-";
/// The folder of an OpenCode version that holds what its plugin forwards, one stream per file.
const FORWARDED: &str = "forwarded";

/// The folders in [`RECORDED`] whose names start with `prefix`, oldest version first.
fn version_dirs(root: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root.join(RECORDED)) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(prefix))
        })
        .collect();
    dirs.sort();
    dirs
}

/// Every scrubbed recording of `verify/fixtures/claude-*/`: (name, payload), placeholders
/// unresolved. A recording file holds one JSON payload per line.
pub fn recorded_payloads(root: &Path) -> Result<Vec<(String, Value)>, String> {
    let mut found = Vec::new();
    for dir in version_dirs(root, CLAUDE_CODE_RECORDINGS) {
        let version = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for name in json_names(&dir).into_iter().chain(jsonl_names(&dir)) {
            let path = ["json", "jsonl"]
                .iter()
                .map(|ext| dir.join(format!("{name}.{ext}")))
                .find(|p| p.exists())
                .unwrap_or_default();
            let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            for (index, line) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
                let value: Value = serde_json::from_str(line)
                    .map_err(|e| format!("{}:{}: {e}", path.display(), index + 1))?;
                found.push((format!("{version}/{name}:{}", index + 1), value));
            }
        }
    }
    Ok(found)
}

/// The pool OpenCode was recorded in, as placeholders of the catalog pool: its main worktree
/// `fg`, and its slots `fg-wt<N>` for `wt<N>`, also as a path relative to `fg`.
const RECORDED_POOL: [(&str, &str); 3] = [
    ("/sandbox/repo/fg-wt", "{pool}/wt"),
    ("/sandbox/repo/fg", "{pool}/fg"),
    ("../fg-wt", "../wt"),
];

/// Rewrites every string of a recorded OpenCode payload to name the catalog pool by placeholders.
pub fn recorded_pool_to_placeholders(value: &mut Value) {
    match value {
        Value::String(text) => {
            for (from, to) in RECORDED_POOL {
                if text.contains(from) {
                    *text = text.replace(from, to);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(recorded_pool_to_placeholders),
        Value::Object(map) => map.values_mut().for_each(recorded_pool_to_placeholders),
        _ => {}
    }
}

/// One forwarded stream: its name and its lines, each the `t` of its raw line and its payload.
pub type Stream = (String, Vec<(i64, Value)>);

/// The forwarded streams of `dir`, an OpenCode version's folder: (stream, lines of (`t`, payload)),
/// payloads with placeholders. A line is `{"line", "t", "payload"}`.
fn forwarded_of(dir: &Path) -> Result<Vec<Stream>, String> {
    let folder = dir.join(FORWARDED);
    let mut found = Vec::new();
    for name in jsonl_names(&folder) {
        let path = folder.join(format!("{name}.jsonl"));
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut lines = Vec::new();
        for (index, line) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
            let value: Value = serde_json::from_str(line)
                .map_err(|e| format!("{}:{}: {e}", path.display(), index + 1))?;
            let (Some(t), Some(payload)) =
                (value.get("t").and_then(Value::as_i64), value.get("payload"))
            else {
                return Err(format!(
                    "{}:{}: a forwarded line has `t` and `payload`",
                    path.display(),
                    index + 1
                ));
            };
            let mut payload = payload.clone();
            recorded_pool_to_placeholders(&mut payload);
            lines.push((t, payload));
        }
        found.push((name, lines));
    }
    Ok(found)
}

/// The forwarded streams of the newest OpenCode version: (stream, lines of (`t`, payload)).
pub fn forwarded_streams(root: &Path) -> Result<Vec<Stream>, String> {
    version_dirs(root, OPENCODE_RECORDINGS)
        .last()
        .map_or_else(|| Ok(Vec::new()), |dir| forwarded_of(dir))
}

/// Every payload OpenCode's plugin forwards in `verify/fixtures/opencode-*/forwarded/`: (name,
/// payload), with placeholders, for `wtpool hook --harness opencode`.
pub fn forwarded_payloads(root: &Path) -> Result<Vec<(String, Value)>, String> {
    let mut found = Vec::new();
    for dir in version_dirs(root, OPENCODE_RECORDINGS) {
        let version = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for (stream, lines) in forwarded_of(&dir)? {
            for (index, (_, payload)) in lines.into_iter().enumerate() {
                found.push((
                    format!("{version}/{FORWARDED}/{stream}:{}", index + 1),
                    payload,
                ));
            }
        }
    }
    Ok(found)
}

fn jsonl_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let path = entry.path();
                    (path.extension().and_then(|e| e.to_str()) == Some("jsonl"))
                        .then(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
                        .flatten()
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The folder a step's `cwd` names: a worktree of the pool, or a path with placeholders.
pub fn step_folder(cwd: &str, pool: &Pool) -> PathBuf {
    if cwd.contains('{') || cwd.contains('/') {
        let mut value = Value::String(cwd.to_owned());
        resolve_placeholders(&mut value, pool);
        PathBuf::from(value.as_str().unwrap_or_default())
    } else {
        pool.worktree(cwd)
    }
}

/// The payload of a scenario's hook step: its fixture with `session_id`, `cwd`, `agent_id` and
/// the `with` fields set, placeholders resolved to `pool`. A step without `agent` drops the
/// fixture's `agent_id` and `agent_type`. A step of another harness is its inline payload, with the
/// placeholders resolved.
pub fn payload(root: &Path, step: &HookStep, pool: &Pool) -> Result<Value, String> {
    if let Some(inline) = step.inline_payload(pool) {
        return Ok(inline);
    }
    let mut value = load_fixture(root, &step.fixture)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| format!("{}: not an object", step.fixture))?;
    set_identity(object, step, pool);
    for (key, field) in &step.with {
        object.insert(key.clone(), field.clone());
    }
    resolve_placeholders(&mut value, pool);
    Ok(value)
}

fn set_identity(object: &mut Map<String, Value>, step: &HookStep, pool: &Pool) {
    let folder = step_folder(&step.cwd, pool);
    object.insert("session_id".into(), Value::String(step.session.clone()));
    object.insert(
        "cwd".into(),
        Value::String(folder.to_string_lossy().into_owned()),
    );
    object.insert(
        "transcript_path".into(),
        Value::String(format!(
            "/Users/verify/.claude/projects/-verify-pool/{}.jsonl",
            step.session
        )),
    );
    match &step.agent {
        Some(agent) => {
            object.insert("agent_id".into(), Value::String(agent.clone()));
        }
        None => {
            object.remove("agent_id");
            object.remove("agent_type");
        }
    }
}

pub const BASH_COMMANDS: &str = "verify/fixtures/bash-commands.toml";

/// One command of [`BASH_COMMANDS`].
#[derive(serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BashCommand {
    pub form: String,
    pub command: String,
    pub cwd: String,
    pub worktree: String,
    pub summary: String,
    #[serde(default)]
    pub home: Option<String>,
    #[serde(default)]
    pub secret: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BashCommands {
    command: Vec<BashCommand>,
}

pub fn bash_commands(root: &Path) -> Result<Vec<BashCommand>, String> {
    let path = root.join(BASH_COMMANDS);
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file: BashCommands = toml::from_str(&text).map_err(|e| format!("{BASH_COMMANDS}: {e}"))?;
    Ok(file.command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures__should_each_be_an_event_of_the_event_table() {
        // Given
        let root = crate::repo::root();
        let events = [
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

        // When
        let names = fixture_names(&root);

        // Then
        assert!(names.len() >= events.len(), "{names:?}");
        for name in &names {
            let value = load_fixture(&root, name).unwrap();
            let event = value["hook_event_name"].as_str().unwrap_or_default();
            assert!(events.contains(&event), "{name}: event {event}");
            assert!(name.starts_with(event), "{name} is named after its event");
        }
        for event in events {
            assert!(
                names.iter().any(|n| n.starts_with(event)),
                "no fixture of {event}"
            );
        }
    }

    #[test]
    fn forwarded_payloads__should_be_plugin_payloads_in_the_catalog_pool() {
        // Given
        let root = crate::repo::root();

        // When
        let payloads = forwarded_payloads(&root).unwrap();

        // Then
        assert!(
            payloads.len() >= 100,
            "{} forwarded payloads",
            payloads.len()
        );
        for (name, payload) in &payloads {
            assert_eq!(payload["v"], 1, "{name}");
            assert!(
                payload["directory"]
                    .as_str()
                    .is_some_and(|d| d.starts_with("{pool}/")),
                "{name}: {payload}"
            );
            assert!(
                !payload.to_string().contains("/sandbox"),
                "{name}: {payload}"
            );
        }
    }

    #[test]
    fn recorded_pool_to_placeholders__should_map_fg_and_its_slots() {
        // Given
        let mut value = serde_json::json!({
            "directory": "/sandbox/repo/fg",
            "data": {"input": {"path": "/sandbox/repo/fg-wt2/notes.txt", "command": "cd ../fg-wt1 && ls"}},
        });

        // When
        recorded_pool_to_placeholders(&mut value);

        // Then
        assert_eq!(
            value,
            serde_json::json!({
                "directory": "{pool}/fg",
                "data": {"input": {"path": "{pool}/wt2/notes.txt", "command": "cd ../wt1 && ls"}},
            })
        );
    }

    #[test]
    fn bash_commands__should_cover_every_form_of_h9_and_h14() {
        // Given
        let root = crate::repo::root();

        // When
        let commands = bash_commands(&root).unwrap();

        // Then
        let text: Vec<&str> = commands.iter().map(|c| c.command.as_str()).collect();
        for form in [
            "cd ",
            "; ",
            "&& ",
            "git -C",
            "--manifest-path",
            "sh -c",
            "nix develop -c",
            "~/",
            "$HOME",
            "../",
            "<<",
            "env ",
            "timeout 600",
            "time ",
            "nice ",
            "=",
            "gh ",
            "npm ",
            "wtpool ",
            "docker ",
            "kubectl ",
            "nix ",
            "cargo ",
        ] {
            assert!(
                text.iter().any(|t| t.contains(form)),
                "no command with {form:?}"
            );
        }
        assert!(commands.iter().filter(|c| c.secret.is_some()).count() >= 3);
        for command in &commands {
            if let Some(secret) = &command.secret {
                assert!(
                    command.command.contains(secret.as_str()),
                    "{}",
                    command.form
                );
            }
        }
    }
}
