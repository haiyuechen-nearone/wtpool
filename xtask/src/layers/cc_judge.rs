//! The judges of the cc layer (verification.md, "Contract test"): the cc checks by the live result
//! of the installed Claude Code and this run's replays, or by the replays alone, as pure functions
//! of the files crate::layers::cc reads.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

use crate::replay::PREFIX;
use crate::results::{CheckResult, FailKind, Outcome};

pub(super) const LAYER: &str = "cc";

/// An outcome the runner writes; any other counts as an error.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
enum RunnerOutcome {
    Pass,
    Fail,
    NeedsYou,
    NotRun,
    #[serde(other)]
    Unknown,
}

/// The kind of a failure the runner writes; any other counts as an error.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
enum RunnerKind {
    Assertion,
    Timeout,
    #[serde(other)]
    Error,
}

/// One result the runner writes to `<run>/cc/results.json`.
#[derive(Deserialize)]
pub(super) struct RunnerResult {
    pub(super) check: String,
    outcome: RunnerOutcome,
    #[serde(default)]
    kind: Option<RunnerKind>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: String,
}

impl RunnerResult {
    pub(super) fn passed(&self) -> bool {
        self.outcome == RunnerOutcome::Pass
    }

    /// The Claude Code asserts held, or failed as asserts: anything else decides the check alone.
    fn claude_ok(&self) -> bool {
        match self.outcome {
            RunnerOutcome::Pass => true,
            RunnerOutcome::Fail => self.kind == Some(RunnerKind::Assertion),
            RunnerOutcome::NeedsYou | RunnerOutcome::NotRun | RunnerOutcome::Unknown => false,
        }
    }

    pub(super) fn to_result(&self) -> CheckResult {
        let check = self.check.as_str();
        let message = self.message.clone();
        match (self.outcome, self.kind) {
            (RunnerOutcome::Pass, _) => CheckResult::pass(check, LAYER, message),
            (RunnerOutcome::NeedsYou, _) => CheckResult::needs_you(
                check,
                LAYER,
                self.code.as_deref().unwrap_or("needs_you"),
                message,
            ),
            (RunnerOutcome::Fail, Some(RunnerKind::Assertion)) => {
                CheckResult::assertion(check, LAYER, message)
            }
            (RunnerOutcome::Fail, Some(RunnerKind::Timeout)) => {
                CheckResult::fail(check, LAYER, FailKind::Timeout, message)
            }
            (RunnerOutcome::NotRun, _) => CheckResult::not_run(check, LAYER, message),
            (RunnerOutcome::Fail | RunnerOutcome::Unknown, _) => {
                CheckResult::error(check, LAYER, message)
            }
        }
    }
}

/// The same result for every wanted check, made by `result` from the check, the layer and
/// `message`.
pub(super) fn fail_all(
    wanted: &BTreeSet<String>,
    result: impl Fn(&str, &str, String) -> CheckResult,
    message: &str,
) -> Vec<CheckResult> {
    let mut found = Vec::new();
    for check in wanted {
        found.push(result(check, LAYER, message.to_owned()));
    }
    found
}

/// The live result's file beside the recordings of one Claude Code version.
pub const LIVE_RESULTS: &str = "cc-results.json";

/// Checks over all sessions or of the harness, decided by the live result alone.
const LIVE_ONLY: [&str; 2] = [SANDBOX_CHECK, NO_OUTPUT_CHECK];
const SANDBOX_CHECK: &str = "contract__should_run_in_the_sandbox";
const NO_OUTPUT_CHECK: &str = "cc_no_hook_output";

/// The start of the message of every cc result counted by replay alone.
pub const REPLAY_ONLY: &str = "replay only";

/// A session's check by its replay `cc_replay:<check>` alone: `passed` gives its message when the
/// replay passed.
fn by_its_replay(
    check: &str,
    replayed: &[CheckResult],
    passed: impl FnOnce(&str) -> String,
) -> CheckResult {
    let replay = format!("{PREFIX}{check}");
    match replayed.iter().find(|r| r.check == replay) {
        None => CheckResult::error(
            check,
            LAYER,
            format!("this run has no {replay}: run the int part first"),
        ),
        Some(r) if r.outcome == Outcome::Pass => CheckResult::pass(check, LAYER, passed(&replay)),
        Some(r) => CheckResult::assertion(check, LAYER, format!("{replay}: {}", r.message)),
    }
}

/// The hook output by every session replay: each fails first when `wtpool hook` prints, so a
/// replay that failed on an assert saw no output.
fn no_output_by_replay(check: &str, sessions: &[&CheckResult]) -> CheckResult {
    if sessions.is_empty() {
        return CheckResult::error(
            check,
            LAYER,
            "this run has no cc_replay: run the int part first",
        );
    }
    let mut failed = Vec::new();
    for session in sessions {
        let printed_or_unknown = session.kind != Some(FailKind::Assertion)
            || session.message.starts_with(crate::replay::PRINTED);
        if session.outcome != Outcome::Pass && printed_or_unknown {
            failed.push(session.check.as_str());
        }
    }
    if failed.is_empty() {
        CheckResult::pass(
            check,
            LAYER,
            format!(
                "no hook output in {} replayed sessions; that the hook's parent is claude needs a live run",
                sessions.len()
            ),
        )
    } else {
        CheckResult::assertion(check, LAYER, failed.join(", "))
    }
}

/// The cc checks by the replays alone: each session by its `cc_replay:<name>`, the hook output
/// by every session replay, and the sandbox of live sessions skipped, as no session ran.
pub(super) fn by_replay(
    replayed: &[CheckResult],
    wanted: &BTreeSet<String>,
    reason: &str,
) -> Vec<CheckResult> {
    let note = format!("{REPLAY_ONLY} ({reason})");
    let pid = format!("{PREFIX}cc_records_claude_pid");
    let sessions: Vec<&CheckResult> = replayed.iter().filter(|r| r.check != pid).collect();
    let mut found = Vec::new();
    for check in wanted {
        let mut result = match check.as_str() {
            SANDBOX_CHECK => {
                let mut skipped = CheckResult::skipped(
                    check,
                    LAYER,
                    "no live Claude Code session ran, so none had to stay in the sandbox",
                );
                skipped.code = Some(crate::scrub::NO_RECORDINGS.to_owned());
                skipped
            }
            NO_OUTPUT_CHECK => no_output_by_replay(check, &sessions),
            _ => by_its_replay(check, replayed, |replay| {
                format!("wtpool by {replay}; the Claude Code asserts need a live run")
            }),
        };
        result.message = format!("{note}: {}", result.message);
        found.push(result);
    }
    found
}

/// The live result of the oracle step for one Claude Code version, as read from `path`.
pub(super) struct LiveResult<'a> {
    pub(super) version: &'a str,
    pub(super) path: &'a Path,
    pub(super) text: &'a str,
}

/// The cc checks by `live`, a result that `locked`, the recordings of the lock, must hash: each
/// Claude Code assert by the live result, and each wtpool assert by this run's `replayed` results.
pub(super) fn judge_live(
    live: &LiveResult,
    locked: &BTreeMap<String, String>,
    replayed: &[CheckResult],
    wanted: &BTreeSet<String>,
) -> Vec<CheckResult> {
    let version = live.version;
    let rel = format!("claude-{version}/{LIVE_RESULTS}");
    if locked.get(&rel) != Some(&crate::repo::sha256(live.text.as_bytes())) {
        return fail_all(
            wanted,
            CheckResult::error,
            &format!("the lock does not hash the live result {rel}"),
        );
    }
    let entries: Vec<RunnerResult> = match serde_json::from_str(live.text) {
        Ok(entries) => entries,
        Err(error) => {
            return fail_all(
                wanted,
                CheckResult::not_run,
                &format!("{}: {error}", live.path.display()),
            );
        }
    };
    let mut found = Vec::new();
    for check in wanted {
        let Some(entry) = entries.iter().find(|e| &e.check == check) else {
            found.push(CheckResult::not_run(
                check,
                LAYER,
                format!("the live result of {version} has no {check}"),
            ));
            continue;
        };
        if LIVE_ONLY.contains(&check.as_str()) || !entry.claude_ok() {
            let mut result = entry.to_result();
            result.message = format!("live result of Claude Code {version}: {}", result.message);
            found.push(result);
            continue;
        }
        found.push(by_its_replay(check, replayed, |replay| {
            format!("Claude Code {version} live, wtpool by {replay}")
        }));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    const LIVE: &str = r#"[
        {"check": "cc_permission", "outcome": "pass", "kind": null, "code": null, "message": "ok"},
        {"check": "cc_idle_prompt", "outcome": "fail", "kind": "assertion", "message": "no idle"},
        {"check": "cc_stop_failure", "outcome": "fail", "kind": "error", "message": "crashed"},
        {"check": "cc_no_hook_output", "outcome": "pass", "message": "none"},
        {"check": "cc_session_end", "outcome": "unheard_of", "message": "?"}
    ]"#;

    fn names(checks: &[&str]) -> BTreeSet<String> {
        checks.iter().map(|check| (*check).to_owned()).collect()
    }

    fn summary(found: &[CheckResult]) -> Vec<(&str, Outcome, Option<FailKind>, &str)> {
        found
            .iter()
            .map(|r| (r.check.as_str(), r.outcome, r.kind, r.message.as_str()))
            .collect()
    }

    #[test]
    fn judge_live__should_judge_claude_by_the_live_result_and_wtpool_by_the_replays() {
        // Given
        let live = LiveResult {
            version: "1.0",
            path: Path::new("/r/claude-1.0/cc-results.json"),
            text: LIVE,
        };
        let locked = BTreeMap::from([(
            "claude-1.0/cc-results.json".to_owned(),
            crate::repo::sha256(LIVE.as_bytes()),
        )]);
        let replayed = vec![
            CheckResult::pass("cc_replay:cc_permission", "int", ""),
            CheckResult::assertion("cc_replay:cc_idle_prompt", "int", "differs"),
        ];
        let wanted = names(&[
            "cc_permission",
            "cc_idle_prompt",
            "cc_stop_failure",
            NO_OUTPUT_CHECK,
            "cc_session_end",
            "cc_subagent",
        ]);

        // When
        let found = judge_live(&live, &locked, &replayed, &wanted);

        // Then
        assert_eq!(
            summary(&found),
            vec![
                (
                    "cc_idle_prompt",
                    Outcome::Fail,
                    Some(FailKind::Assertion),
                    "cc_replay:cc_idle_prompt: differs"
                ),
                (
                    NO_OUTPUT_CHECK,
                    Outcome::Pass,
                    None,
                    "live result of Claude Code 1.0: none"
                ),
                (
                    "cc_permission",
                    Outcome::Pass,
                    None,
                    "Claude Code 1.0 live, wtpool by cc_replay:cc_permission"
                ),
                (
                    "cc_session_end",
                    Outcome::Fail,
                    Some(FailKind::Error),
                    "live result of Claude Code 1.0: ?"
                ),
                (
                    "cc_stop_failure",
                    Outcome::Fail,
                    Some(FailKind::Error),
                    "live result of Claude Code 1.0: crashed"
                ),
                (
                    "cc_subagent",
                    Outcome::NotRun,
                    None,
                    "the live result of 1.0 has no cc_subagent"
                ),
            ]
        );
    }

    #[rstest]
    #[case::unlocked(
        LIVE,
        None,
        Outcome::Fail,
        "the lock does not hash the live result claude-1.0/cc-results.json"
    )]
    #[case::other_hash(LIVE, Some("0".to_owned()), Outcome::Fail, "the lock does not hash the live result claude-1.0/cc-results.json")]
    #[case::no_json(
        "[",
        Some(crate::repo::sha256(b"[")),
        Outcome::NotRun,
        "/r/cc-results.json: EOF while parsing a list at line 1 column 1"
    )]
    fn judge_live__should_judge_nothing_by_a_result_it_cannot_trust(
        #[case] text: &str,
        #[case] hash: Option<String>,
        #[case] outcome: Outcome,
        #[case] message: &str,
    ) {
        // Given
        let live = LiveResult {
            version: "1.0",
            path: Path::new("/r/cc-results.json"),
            text,
        };
        let locked: BTreeMap<String, String> = hash
            .into_iter()
            .map(|hash| ("claude-1.0/cc-results.json".to_owned(), hash))
            .collect();
        let wanted = names(&["cc_permission", SANDBOX_CHECK]);

        // When
        let found = judge_live(&live, &locked, &[], &wanted);

        // Then
        assert_eq!(found.len(), 2);
        assert!(
            found
                .iter()
                .all(|r| r.outcome == outcome && r.message == message),
            "{found:?}"
        );
    }

    #[test]
    fn by_replay__should_judge_each_check_by_the_replays_alone() {
        // Given
        let replay = |name: &str| format!("{}{name}", crate::replay::PREFIX);
        let replayed = vec![
            CheckResult::pass(&replay("cc_permission"), "int", ""),
            CheckResult::assertion(&replay("cc_idle_prompt"), "int", "never idle"),
            CheckResult::assertion(
                &replay("cc_stop_failure"),
                "int",
                "wtpool hook printed: Stop: \"x\"",
            ),
            CheckResult::pass(&replay("cc_records_claude_pid"), "int", ""),
        ];
        let wanted: BTreeSet<String> = [
            "cc_permission",
            "cc_idle_prompt",
            "cc_records_claude_pid",
            "cc_session_end",
            NO_OUTPUT_CHECK,
            SANDBOX_CHECK,
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();

        // When
        let found = by_replay(&replayed, &wanted, "no recordings");

        // Then
        let outcomes: Vec<(&str, Outcome, Option<FailKind>)> = found
            .iter()
            .map(|r| (r.check.as_str(), r.outcome, r.kind))
            .collect();
        assert_eq!(
            outcomes,
            vec![
                ("cc_idle_prompt", Outcome::Fail, Some(FailKind::Assertion)),
                (NO_OUTPUT_CHECK, Outcome::Fail, Some(FailKind::Assertion)),
                ("cc_permission", Outcome::Pass, None),
                ("cc_records_claude_pid", Outcome::Pass, None),
                ("cc_session_end", Outcome::Fail, Some(FailKind::Error)),
                (SANDBOX_CHECK, Outcome::Skipped, None),
            ]
        );
        assert!(
            found
                .iter()
                .all(|r| r.message.starts_with("replay only (no recordings): ")),
            "{found:?}"
        );
        let no_output = found.iter().find(|r| r.check == NO_OUTPUT_CHECK).unwrap();
        assert!(
            no_output.message.ends_with("cc_replay:cc_stop_failure"),
            "{}",
            no_output.message
        );
    }
}
