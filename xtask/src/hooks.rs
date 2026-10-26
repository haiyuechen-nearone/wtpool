//! Hook payloads of the oracle: the fixture corpus of `verify/fixtures/hooks/` and the payload a
//! scenario's `hook` step sends to `wtpool hook`.
//!
//! The corpus is written by hand from the hook input of Claude Code 2.1.295, one file per event
//! and tool, with `{pool}` standing for the pool folder. The [scrubbed](crate::scrub) recordings
//! of a live contract run go to `verify/fixtures/claude-<version>/` and join the corpus of the
//! checks that run every fixture.

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

/// Every scrubbed recording of `verify/fixtures/claude-*/`: (name, payload), placeholders
/// unresolved. A recording file holds one JSON payload per line.
pub fn recorded_payloads(root: &Path) -> Result<Vec<(String, Value)>, String> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(root.join(RECORDED)) else {
        return Ok(found);
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("claude-"))
        })
        .collect();
    dirs.sort();
    for dir in dirs {
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
/// fixture's `agent_id` and `agent_type`.
pub fn payload(root: &Path, step: &HookStep, pool: &Pool) -> Result<Value, String> {
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
