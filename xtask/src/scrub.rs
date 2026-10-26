//! The scrubber: it turns the raw recordings of a live `--part cc --contract --record` run, kept
//! in [`recordings_dir`] and hashed into the lock, into `verify/fixtures/claude-<version>/`.
//! `fixtures__should_match_a_fresh_scrub` redoes it. On a machine without that folder it checks
//! only that each fixture comes from a locked recording, and is skipped with that reason.
//!
//! A raw recording `claude-<version>/<session>.jsonl` starts with a header line
//! `{"wt_verify_recording": 1, "pool": <path>, "home": <path>, "config": <path>}` and then holds
//! one hook payload per line, as the recorder hook read it. `config`, Claude Code's configuration
//! folder in the run folder, is absent from recordings made before it moved there.
//!
//! The scrubbed copy drops the header. It writes `{pool}` for the pool folder, `/Users/verify`
//! for the home and `/Users/verify/.claude` for the configuration folder, so a transcript path
//! reads as with the default one. The folder names Claude Code makes from these paths get
//! `-verify-pool` and `-Users-verify`. `{tmp}` replaces the system's temporary folder and
//! `{tmp}/claude` the one Claude Code keeps per account, `/tmp/claude-<uid>`. Session, agent and
//! tool use ids get new names in order of appearance: `s1`, `a1`, `toolu_01`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::oracle::{LOCK, load_lock_text};
use crate::repo::{sha256, walk};
use crate::results::CheckResult;

pub const NAME: &str = "fixtures__should_match_a_fresh_scrub";
const LAYER: &str = "static";
const FIXTURES: &str = "verify/fixtures";

/// The code of a result skipped because the recordings folder does not exist.
pub const NO_RECORDINGS: &str = "no_recordings";

/// True when the recordings folder is missing, so the checks that need it skip.
pub fn recordings_missing() -> bool {
    recordings_dir().is_none_or(|dir| !dir.exists())
}

/// `recordings/` of the harness data folder, [`crate::dirs::data_dir`], where a live `cc` run with
/// `--record` writes.
pub fn recordings_dir() -> Option<PathBuf> {
    crate::dirs::data_dir()
        .ok()
        .map(|dir| dir.join("recordings"))
}

/// The SHA 256 of each raw recording and of each live result `cc-results.json` beside them, by
/// path below `dir`.
pub fn recording_hashes(dir: &Path) -> BTreeMap<String, String> {
    let skip = |_: &str| false;
    walk(dir, dir, &skip)
        .into_iter()
        .filter(|rel| rel.ends_with(".jsonl") || rel.ends_with("/cc-results.json"))
        .filter_map(|rel| {
            fs::read(dir.join(&rel))
                .ok()
                .map(|bytes| (rel, sha256(&bytes)))
        })
        .collect()
}

#[derive(Default)]
struct Names {
    sessions: BTreeMap<String, String>,
    agents: BTreeMap<String, String>,
    tools: BTreeMap<String, String>,
}

impl Names {
    fn name(
        map: &mut BTreeMap<String, String>,
        raw: &str,
        make: impl Fn(usize) -> String,
    ) -> String {
        let next = map.len() + 1;
        map.entry(raw.to_owned())
            .or_insert_with(|| make(next))
            .clone()
    }
}

/// Rewrites every string of `value`. With `rename`, a string under an id `key` gets its name from
/// `names`. Any other string gets `replacements`, then the names of the ids it holds.
fn rewrite(
    value: &mut Value,
    replacements: &[(String, String)],
    names: &mut Names,
    key: Option<&str>,
    rename: bool,
) {
    match value {
        Value::String(text) => {
            let renamed = match key.filter(|_| rename) {
                Some("session_id") => {
                    Some(Names::name(&mut names.sessions, text, |n| format!("s{n}")))
                }
                Some("agent_id") => Some(Names::name(&mut names.agents, text, |n| format!("a{n}"))),
                Some("tool_use_id") => Some(Names::name(&mut names.tools, text, |n| {
                    format!("toolu_{n:02}")
                })),
                _ => None,
            };
            if let Some(renamed) = renamed {
                *text = renamed;
                return;
            }
            for (from, to) in replacements {
                if !from.is_empty() {
                    *text = text.replace(from.as_str(), to);
                }
            }
            for (raw, renamed) in names
                .sessions
                .iter()
                .chain(&names.agents)
                .chain(&names.tools)
            {
                if text.contains(raw.as_str()) {
                    *text = text.replace(raw.as_str(), renamed);
                }
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| rewrite(item, replacements, names, key, rename)),
        Value::Object(map) => {
            for (field, item) in map.iter_mut() {
                rewrite(item, replacements, names, Some(field.as_str()), rename);
            }
        }
        _ => {}
    }
}

/// Where Claude Code keeps its temporary files, `/tmp/claude-<uid>`, before the uid.
const CLAUDE_TMP: &str = "/tmp/claude-";

/// `{tmp}/` for the system's temporary folder, and `{tmp}/claude/` for each per account folder
/// of Claude Code that `raw` names, `/tmp/claude-<uid>/` or `/private/tmp/claude-<uid>/` (macOS's
/// real `/tmp`). Each ends in `/`, so only whole folder names match.
fn temp_replacements(raw: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let system = std::env::temp_dir();
    for dir in [Some(system.clone()), fs::canonicalize(&system).ok()]
        .into_iter()
        .flatten()
    {
        let text = dir.to_string_lossy().trim_end_matches('/').to_owned();
        found.push((format!("{text}/"), "{tmp}/".to_owned()));
    }
    for (at, _) in raw.match_indices(CLAUDE_TMP) {
        let start = at.saturating_add(CLAUDE_TMP.len());
        let after = raw.get(start..).unwrap_or_default();
        let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let end = start.saturating_add(digits);
        if digits == 0 || !raw.get(end..).is_some_and(|rest| rest.starts_with('/')) {
            continue;
        }
        let begin = if raw
            .get(..at)
            .is_some_and(|before| before.ends_with("/private"))
        {
            at.saturating_sub("/private".len())
        } else {
            at
        };
        if let Some(root) = raw.get(begin..=end) {
            found.push((root.to_owned(), "{tmp}/claude/".to_owned()));
        }
    }
    found.sort();
    found.dedup();
    found
}

/// The scrubbed text of one raw recording.
pub fn scrub(raw: &str) -> Result<String, String> {
    let mut lines = raw.lines().filter(|line| !line.trim().is_empty());
    let header: Value = serde_json::from_str(lines.next().ok_or("an empty recording")?)
        .map_err(|e| format!("the header: {e}"))?;
    if header.get("wt_verify_recording").and_then(Value::as_u64) != Some(1) {
        return Err("no wt_verify_recording header".to_owned());
    }
    let text = |field: &str| {
        header
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    // Claude Code also names folders after a path with each other character as `-`, as in
    // `~/.claude/projects/-Users-x-pool-fg/`.
    let dashed = |path: String| -> String {
        path.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect()
    };
    let mut replacements = vec![
        (text("config"), "/Users/verify/.claude".to_owned()),
        (text("pool"), "{pool}".to_owned()),
        (text("home"), "/Users/verify".to_owned()),
        (dashed(text("pool")), "-verify-pool".to_owned()),
        (dashed(text("home")), "-Users-verify".to_owned()),
    ];
    replacements.extend(temp_replacements(raw));
    replacements.retain(|(from, _)| from.len() > 1);
    // Longest first, so the pool inside the home becomes {pool}.
    replacements.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
    let mut names = Names::default();
    let mut out = String::new();
    for (index, line) in lines.enumerate() {
        let mut value: Value =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", index + 2))?;
        // Ids first, so later strings that hold them are renamed too. An Agent call's
        // `tool_response.agentId` is the `agent_id` of the subagent it started, and a recording
        // may hold it before, or without, that subagent's own events.
        for field in ["session_id", "agent_id", "tool_use_id"] {
            if let Some(item) = value.get_mut(field) {
                rewrite(item, &[], &mut names, Some(field), true);
            }
        }
        if let Some(item) = value
            .get_mut("tool_response")
            .and_then(|response| response.get_mut("agentId"))
        {
            rewrite(item, &[], &mut names, Some("agent_id"), true);
        }
        rewrite(&mut value, &replacements, &mut names, None, false);
        out.push_str(&serde_json::to_string(&value).map_err(|e| e.to_string())?);
        out.push('\n');
    }
    Ok(out)
}

/// Writes the scrubbed copy of every recording into `verify/fixtures/`.
pub fn write_fixtures(root: &Path, recordings: &Path) -> Result<Vec<String>, String> {
    let mut written = Vec::new();
    for rel in recording_hashes(recordings)
        .into_keys()
        .filter(|rel| rel.ends_with(".jsonl"))
    {
        let raw = fs::read_to_string(recordings.join(&rel)).map_err(|e| format!("{rel}: {e}"))?;
        let path = root.join(FIXTURES).join(&rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&path, scrub(&raw).map_err(|e| format!("{rel}: {e}"))?)
            .map_err(|e| e.to_string())?;
        written.push(rel);
    }
    Ok(written)
}

/// The problems a fresh scrub of the locked recordings finds in `verify/fixtures/claude-*/`.
pub fn problems(root: &Path, locked: &BTreeMap<String, String>, recordings: &Path) -> Vec<String> {
    if locked.is_empty() {
        return vec![format!(
            "{LOCK} hashes no recording: the oracle step's live run (`--part cc --contract --record`) recorded nothing"
        )];
    }
    let mut found = Vec::new();
    if recordings.exists() {
        found.extend(scrub_problems(root, locked, recordings));
    }
    let skip = |_: &str| false;
    let fixtures = root.join(FIXTURES);
    for rel in walk(&fixtures, &fixtures, &skip) {
        if rel.starts_with("claude-") && rel.ends_with(".jsonl") && !locked.contains_key(&rel) {
            found.push(format!("{FIXTURES}/{rel} comes from no locked recording"));
        }
    }
    found
}

/// The problems a fresh scrub of each locked recording in `recordings` finds.
fn scrub_problems(
    root: &Path,
    locked: &BTreeMap<String, String>,
    recordings: &Path,
) -> Vec<String> {
    let mut found = Vec::new();
    let on_disk = recording_hashes(recordings);
    for (rel, hash) in locked {
        match on_disk.get(rel) {
            None => found.push(format!("the recording {rel} is gone")),
            Some(now) if now != hash => {
                found.push(format!("the recording {rel} changed since the freeze"))
            }
            Some(_) if !rel.ends_with(".jsonl") => {}
            Some(_) => {
                let fresh = fs::read_to_string(recordings.join(rel))
                    .map_err(|e| e.to_string())
                    .and_then(|raw| scrub(&raw));
                let fixture = fs::read_to_string(root.join(FIXTURES).join(rel));
                match (fresh, fixture) {
                    (Err(error), _) => found.push(format!("{rel}: {error}")),
                    (_, Err(_)) => found.push(format!("no fixture {FIXTURES}/{rel}")),
                    (Ok(fresh), Ok(fixture)) if fresh != fixture => {
                        found.push(format!("{FIXTURES}/{rel} differs from a fresh scrub"));
                    }
                    _ => {}
                }
            }
        }
    }
    found
}

pub fn check(root: &Path) -> CheckResult {
    let lock = match fs::read_to_string(root.join(LOCK)).map(|text| load_lock_text(&text)) {
        Ok(Ok(lock)) => lock,
        Ok(Err(error)) => return CheckResult::error(NAME, LAYER, error),
        Err(error) => return CheckResult::error(NAME, LAYER, format!("{LOCK}: {error}")),
    };
    let Some(recordings) = recordings_dir() else {
        return CheckResult::error(NAME, LAYER, "HOME is not set");
    };
    let found = problems(root, &lock.recordings, &recordings);
    if found.is_empty() && !recordings.exists() {
        let mut skipped = CheckResult::skipped(
            NAME,
            LAYER,
            format!(
                "{} does not exist, so no fresh scrub of the {} locked recordings ran; each fixture comes from a locked recording, and the lock hashes each fixture (oracle_unchanged)",
                recordings.display(),
                lock.recordings.len()
            ),
        );
        skipped.code = Some(NO_RECORDINGS.to_owned());
        skipped
    } else if found.is_empty() {
        CheckResult::pass(
            NAME,
            LAYER,
            format!("{} recordings scrub to the fixtures", lock.recordings.len()),
        )
    } else {
        CheckResult::assertion(NAME, LAYER, found.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub__should_rename_ids_and_paths_in_order_of_appearance() {
        // Given
        let raw = concat!(
            r#"{"wt_verify_recording":1,"pool":"/run/x/pool","home":"/Users/you"}"#,
            "\n",
            r#"{"session_id":"9f3a-77","cwd":"/run/x/pool/fg","transcript_path":"/Users/you/.claude/projects/p/9f3a-77.jsonl","tool_use_id":"toolu_ZZ"}"#,
            "\n",
            r#"{"session_id":"9f3a-77","agent_id":"ab12","tool_use_id":"toolu_YY","tool_input":{"command":"cd /run/x/pool/wt2 && ls"}}"#,
            "\n",
        );

        // When
        let scrubbed = scrub(raw).unwrap();

        // Then
        let lines: Vec<Value> = scrubbed
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["session_id"], "s1");
        assert_eq!(lines[0]["cwd"], "{pool}/fg");
        assert_eq!(
            lines[0]["transcript_path"],
            "/Users/verify/.claude/projects/p/s1.jsonl"
        );
        assert_eq!(lines[0]["tool_use_id"], "toolu_01");
        assert_eq!(lines[1]["agent_id"], "a1");
        assert_eq!(lines[1]["tool_use_id"], "toolu_02");
        assert_eq!(lines[1]["tool_input"]["command"], "cd {pool}/wt2 && ls");
    }

    #[test]
    fn scrub__should_write_the_default_claude_folder_for_the_configuration_folder() {
        // Given
        let raw = concat!(
            r#"{"wt_verify_recording":1,"pool":"/Users/you/run/cc/pools/x","home":"/Users/you","config":"/Users/you/run/cc/claude-config"}"#,
            "\n",
            r#"{"session_id":"9f3a-77","transcript_path":"/Users/you/run/cc/claude-config/projects/-Users-you-run-cc-pools-x-fg/9f3a-77.jsonl"}"#,
            "\n",
        );

        // When
        let scrubbed = scrub(raw).unwrap();

        // Then
        let line: Value = serde_json::from_str(scrubbed.lines().next().unwrap()).unwrap();
        assert_eq!(
            line["transcript_path"],
            "/Users/verify/.claude/projects/-verify-pool-fg/s1.jsonl"
        );
    }

    #[test]
    fn scrub__should_rename_the_agent_id_of_an_agent_call() {
        // Given
        let raw = concat!(
            r#"{"wt_verify_recording":1,"pool":"/run/x/pool","home":"/Users/you"}"#,
            "\n",
            r#"{"session_id":"9f3a-77","tool_name":"Agent","tool_response":{"agentId":"a11b22","status":"completed"}}"#,
            "\n",
            r#"{"session_id":"9f3a-77","agent_id":"a11b22","agent_transcript_path":"/Users/you/t/agent-a11b22.jsonl"}"#,
            "\n",
        );

        // When
        let scrubbed = scrub(raw).unwrap();

        // Then
        let lines: Vec<Value> = scrubbed
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines[0]["tool_response"]["agentId"], "a1");
        assert_eq!(lines[1]["agent_id"], "a1");
        assert_eq!(
            lines[1]["agent_transcript_path"],
            "/Users/verify/t/agent-a1.jsonl"
        );
    }

    #[test]
    fn scrub__should_write_tmp_for_temporary_folders() {
        // Given
        let system = std::env::temp_dir();
        let system = system.to_string_lossy();
        let system = system.trim_end_matches('/');
        let raw = format!(
            "{}\n{}\n",
            r#"{"wt_verify_recording":1,"pool":"/run/x/pool","home":"/Users/you"}"#,
            serde_json::json!({
                "scratchpad_dir": "/private/tmp/claude-501/-run-x-pool-fg/9f3a/scratchpad",
                "output": "/tmp/claude-1000/t/a.output and /tmp/claude-x/kept",
                "file": format!("{system}/wtpool-1/f"),
                "lookalike": "/Users/you/tmpfiles/claude-501/kept",
            })
        );

        // When
        let scrubbed = scrub(&raw).unwrap();

        // Then
        let line: Value = serde_json::from_str(scrubbed.trim_end()).unwrap();
        assert_eq!(
            line["scratchpad_dir"],
            "{tmp}/claude/-verify-pool-fg/9f3a/scratchpad"
        );
        assert_eq!(
            line["output"],
            "{tmp}/claude/t/a.output and /tmp/claude-x/kept"
        );
        assert_eq!(line["file"], "{tmp}/wtpool-1/f");
        assert_eq!(line["lookalike"], "/Users/verify/tmpfiles/claude-501/kept");
    }

    #[test]
    fn problems__should_need_a_locked_recording() {
        // Given
        let root = crate::repo::root();

        // When
        let found = problems(&root, &BTreeMap::new(), Path::new("/nonexistent"));

        // Then
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("hashes no recording"), "{found:?}");
    }

    #[test]
    fn problems__should_still_name_unlocked_fixtures_without_recordings() {
        // Given
        let root = crate::repo::root();
        let locked = BTreeMap::from([("claude-0/none.jsonl".to_owned(), "h".to_owned())]);

        // When
        let found = problems(&root, &locked, Path::new("/nonexistent"));

        // Then
        assert!(!found.is_empty());
        assert!(
            found
                .iter()
                .all(|problem| problem.contains("comes from no locked recording")),
            "{found:?}"
        );
    }
}
