//! `oc_translate:<case>`: each case of the newest `verify/fixtures/opencode-<version>/translate/`
//! sends one payload of OpenCode's plugin to `wtpool hook --harness opencode`, over the records
//! the case names, and judges the record the hook leaves and the lines it logs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use super::{LAYER, OPENCODE, PRINTED, ReplaySandbox, field};
use crate::pool;
use crate::results::CheckResult;

pub const PREFIX: &str = "oc_translate:";
/// The time every case's hook runs at, as `2026-10-20T12:00:00Z` is [`super::clock_text`]`(0)`.
const NOW_SECONDS: i64 = 0;
/// An expected value that matches any value, or none.
const ANY: &str = "{any}";
/// The field of a record that names the start of its writer, the real time of no case.
const WRITER_STARTED: &str = "writer_started";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    before: Option<Value>,
    #[serde(default)]
    others: Vec<Value>,
    payload: Value,
    after: Option<Value>,
    log: usize,
    /// The payload comes from OpenCode's schema, not from a recording.
    #[serde(default)]
    schema: bool,
}

fn cases_dir(root: &Path) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(root.join(crate::hooks::RECORDED))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .starts_with(crate::hooks::OPENCODE_RECORDINGS)
            })
        })
        .collect();
    dirs.sort();
    dirs.pop().map(|dir| dir.join("translate"))
}

/// The names of the translation cases, sorted.
pub(super) fn case_names(root: &Path) -> Vec<String> {
    let Some(dir) = cases_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let path = entry.path();
                    (path.extension().is_some_and(|e| e == "json"))
                        .then(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
                        .flatten()
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The file stem of `record`: `<session_id>[.<agent_id>]`, after `<harness>-` for every harness but
/// Claude Code.
fn stem(record: &Value) -> String {
    let harness = field(record, &["harness"])
        .as_str()
        .unwrap_or("claude-code");
    let session = field(record, &["session_id"]).as_str().unwrap_or_default();
    let prefix = if harness == "claude-code" {
        String::new()
    } else {
        format!("{harness}-")
    };
    match field(record, &["agent_id"]).as_str() {
        Some(agent) => format!("{prefix}{session}.{agent}"),
        None => format!("{prefix}{session}"),
    }
}

/// The differences of `actual` from `expected` below `path`: [`ANY`] matches any value, and
/// [`WRITER_STARTED`] is not compared.
fn differences(path: &str, expected: &Value, actual: Option<&Value>, found: &mut Vec<String>) {
    if expected.as_str() == Some(ANY) {
        return;
    }
    let Some(actual) = actual else {
        found.push(format!("{path}: missing, expected {expected}"));
        return;
    };
    match (expected, actual) {
        (Value::Object(want), Value::Object(have)) => {
            for (key, value) in want {
                if key != WRITER_STARTED {
                    differences(&format!("{path}.{key}"), value, have.get(key), found);
                }
            }
            for (key, value) in have {
                if !want.contains_key(key) && key != WRITER_STARTED {
                    found.push(format!("{path}.{key}: not expected, found {value}"));
                }
            }
        }
        (Value::Array(want), Value::Array(have)) if want.len() == have.len() => {
            for (index, (value, item)) in want.iter().zip(have).enumerate() {
                differences(&format!("{path}[{index}]"), value, Some(item), found);
            }
        }
        _ if expected != actual => {
            found.push(format!("{path}: expected {expected}, found {actual}"))
        }
        _ => {}
    }
}

/// The record files of the sandbox's activity folder by name, as bytes.
fn record_files(sandbox: &ReplaySandbox) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(sandbox.pool.activity())
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().ends_with(".json"))
                .filter_map(|e| {
                    let bytes = fs::read(e.path()).ok()?;
                    Some((e.file_name().to_string_lossy().into_owned(), bytes))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn hook_log(sandbox: &ReplaySandbox) -> PathBuf {
    sandbox.dir.join("state").join("wtpool").join("hook.log")
}

/// Empties the activity folder but its sandbox marker, and the hook log.
fn clear(sandbox: &ReplaySandbox) -> Result<(), String> {
    for entry in fs::read_dir(sandbox.pool.activity())
        .map_err(|e| e.to_string())?
        .flatten()
    {
        if entry.file_name() != ".verify-sandbox" {
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
        }
    }
    let _ = fs::remove_file(hook_log(sandbox));
    Ok(())
}

fn resolved(value: &Value, sandbox: &ReplaySandbox) -> Value {
    let mut value = value.clone();
    pool::resolve_placeholders(&mut value, &sandbox.pool);
    value
}

/// The message of a passed case.
fn passed(case: &Case) -> &'static str {
    if case.schema {
        "a payload of OpenCode 2.2.0's schema, which no recording holds"
    } else {
        ""
    }
}

/// Runs one case and returns its verdict. An `Err` of the outer result is a setup error.
fn run_case(sandbox: &mut ReplaySandbox, case: &Case) -> Result<Result<(), String>, String> {
    clear(sandbox)?;
    for record in case.others.iter().chain(&case.before) {
        let record = resolved(record, sandbox);
        let path = sandbox
            .pool
            .activity()
            .join(format!("{}.json", stem(&record)));
        fs::write(&path, record.to_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let written = record_files(sandbox);
    sandbox.set_clock(NOW_SECONDS)?;
    let stdout = sandbox.hook(&resolved(&case.payload, sandbox))?;
    let mut problems = Vec::new();
    if !stdout.is_empty() {
        problems.push(format!("{PRINTED}: {stdout:?}"));
    }
    let logged = fs::read_to_string(hook_log(sandbox))
        .map(|text| text.lines().count())
        .unwrap_or(0);
    if logged != case.log {
        problems.push(format!(
            "the hook logged {logged} lines, expected {}",
            case.log
        ));
    }
    let now = record_files(sandbox);
    match &case.after {
        None => {
            for (name, bytes) in &now {
                if written.get(name) != Some(bytes) {
                    problems.push(format!("{name}: written, expected no write"));
                }
            }
        }
        Some(after) => {
            let after = resolved(after, sandbox);
            let name = format!("{}.json", stem(&after));
            let actual = now
                .get(&name)
                .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());
            match actual {
                None => problems.push(format!(
                    "no record {name}; the activity folder holds {:?}",
                    now.keys().collect::<Vec<_>>()
                )),
                Some(actual) => differences("record", &after, Some(&actual), &mut problems),
            }
        }
    }
    Ok(if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    })
}

/// `oc_translate:<case>` for every case whose check `counts`, in one sandbox pool at `dir`.
pub(super) fn translate_all(
    root: &Path,
    dir: &Path,
    wtpool: &Path,
    profile: Option<&Path>,
    counts: &dyn Fn(&str) -> bool,
) -> Vec<CheckResult> {
    let wanted: Vec<(String, String)> = case_names(root)
        .into_iter()
        .map(|name| (format!("{PREFIX}{name}"), name))
        .filter(|(check, _)| counts(check))
        .collect();
    let Some(cases) = cases_dir(root).filter(|_| !wanted.is_empty()) else {
        return Vec::new();
    };
    let mut sandbox = match ReplaySandbox::new(dir, wtpool, profile, &OPENCODE) {
        Ok(sandbox) => sandbox,
        Err(error) => {
            return wanted
                .iter()
                .map(|(check, _)| CheckResult::error(check, LAYER, error.clone()))
                .collect();
        }
    };
    let mut results = Vec::new();
    for (check, name) in wanted {
        let path = cases.join(format!("{name}.json"));
        let case = fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<Case>(&text).map_err(|e| e.to_string()))
            .map_err(|e| format!("{}: {e}", path.display()));
        let verdict = case.and_then(|case| Ok((run_case(&mut sandbox, &case)?, passed(&case))));
        results.push(match verdict {
            Err(error) => CheckResult::error(&check, LAYER, error),
            Ok((Ok(()), message)) => CheckResult::pass(&check, LAYER, message),
            Ok((Err(message), _)) => CheckResult::assertion(&check, LAYER, message),
        });
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use serde_json::json;

    #[rstest]
    #[case::claude_code(json!({"harness": "claude-code", "session_id": "s1", "agent_id": null}), "s1")]
    #[case::legacy(json!({"session_id": "s1", "agent_id": "a1"}), "s1.a1")]
    #[case::opencode(json!({"harness": "opencode", "session_id": "ses-x", "agent_id": null}), "opencode-ses-x")]
    #[case::opencode_subagent(
        json!({"harness": "opencode", "session_id": "ses-x", "agent_id": "ses-y"}),
        "opencode-ses-x.ses-y"
    )]
    fn stem__should_put_the_harness_before_other_harnesses_ids(
        #[case] record: Value,
        #[case] expected: &str,
    ) {
        // When
        let found = stem(&record);

        // Then
        assert_eq!(found, expected);
    }

    #[rstest]
    #[case::equal(json!({"a": 1}), json!({"a": 1}), 0)]
    #[case::any_value(json!({"a": "{any}"}), json!({"a": [1]}), 0)]
    #[case::any_absent(json!({"a": "{any}"}), json!({}), 0)]
    #[case::writer_started_aside(json!({"writer_started": 1}), json!({"writer_started": 2}), 0)]
    #[case::other_value(json!({"a": 1}), json!({"a": 2}), 1)]
    #[case::missing(json!({"a": 1}), json!({}), 1)]
    #[case::extra(json!({}), json!({"b": 1}), 1)]
    #[case::nested(json!({"w": {"call": "x", "match": "{any}"}}), json!({"w": {"call": "y", "match": "h"}}), 1)]
    #[case::array_length(json!({"a": [1]}), json!({"a": [1, 2]}), 1)]
    #[case::array_items(json!({"a": [{"m": "{any}", "id": 1}]}), json!({"a": [{"m": "h", "id": 1}]}), 0)]
    fn differences__should_compare_but_any_and_the_writer(
        #[case] expected: Value,
        #[case] actual: Value,
        #[case] count: usize,
    ) {
        // Given
        let mut found = Vec::new();

        // When
        differences("record", &expected, Some(&actual), &mut found);

        // Then
        assert_eq!(found.len(), count, "{found:?}");
    }

    #[test]
    fn case_names__should_hold_every_case_of_the_design() {
        // Given
        let root = crate::repo::root();
        let design = fs::read_to_string(root.join("docs/design/harnesses/opencode.md")).unwrap();
        let list = design
            .split("**Translation cases.**")
            .nth(1)
            .and_then(|rest| rest.split("\n\n").next())
            .unwrap_or_default();
        let mut listed: Vec<String> = list
            .split('`')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect();
        listed.sort();

        // When
        let names = case_names(&root);

        // Then
        assert_eq!(names, listed);
    }

    #[test]
    fn cases__should_each_read_with_concrete_records_before() {
        // Given
        let root = crate::repo::root();
        let dir = cases_dir(&root).unwrap();

        // When
        let cases: Vec<(String, Case)> = case_names(&root)
            .into_iter()
            .map(|name| {
                let text = fs::read_to_string(dir.join(format!("{name}.json"))).unwrap();
                (name, serde_json::from_str(&text).unwrap())
            })
            .collect();

        // Then
        assert_eq!(cases.len(), 58);
        for (name, case) in &cases {
            let before = serde_json::to_string(&(&case.before, &case.others)).unwrap();
            assert!(
                !before.contains(ANY),
                "{name}: a wildcard in a record on disk"
            );
            assert!(case.payload.is_object(), "{name}");
        }
    }
}
