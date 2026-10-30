//! The judges of the oc layer: the runner's results as check results, and the oc checks by this
//! run's `oc_replay:<session>` alone where no `opencode` is installed.

use std::collections::BTreeSet;

use serde::Deserialize;

use crate::results::{CheckResult, FailKind, Outcome};

pub(super) const LAYER: &str = "oc";
pub(super) const SANDBOX_CHECK: &str = "contract__should_run_opencode_in_the_sandbox";
/// The prefix of the int part's replays of the recorded OpenCode sessions.
pub(super) const REPLAY_PREFIX: &str = "oc_replay:";
/// The start of the message of every oc result counted by replay alone.
pub const REPLAY_ONLY: &str = "replay only";

/// Each live session that a recorded session's replay stands for.
const REPLAYS: [(&str, &str); 12] = [
    ("oc_read_edit_shell", "run_read_edit_shell"),
    ("oc_subagent", "subagent"),
    ("oc_permission", "permission_approve_reject"),
    ("oc_permission_reject", "permission_approve_reject"),
    ("oc_question", "question"),
    ("oc_interrupt", "interrupt"),
    ("oc_long_answer", "long_answer"),
    ("oc_deny", "deny"),
    ("oc_two_sessions", "two_folders"),
    ("oc_session_delete", "delete_api"),
    ("oc_title", "delete_api"),
    ("oc_run_title", "title_given"),
];

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
enum RunnerOutcome {
    Pass,
    Fail,
    NeedsYou,
    NotRun,
    Skipped,
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
enum RunnerKind {
    Assertion,
    Timeout,
    #[serde(other)]
    Error,
}

/// One result the runner writes to `<run>/oc/results.json`.
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
            (RunnerOutcome::Skipped, _) => CheckResult::skipped(check, LAYER, message),
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
    wanted
        .iter()
        .map(|check| result(check, LAYER, message.to_owned()))
        .collect()
}

/// The oc checks by `replayed`, this run's `oc_replay:<session>` results, alone: each scripted
/// session by the replay of its recording, and the sessions no recording holds, the sandbox
/// check among them, not run, as no session ran.
pub(super) fn by_replay(
    replayed: &[CheckResult],
    wanted: &BTreeSet<String>,
    reason: &str,
) -> Vec<CheckResult> {
    let note = format!("{REPLAY_ONLY} ({reason})");
    let mut found = Vec::new();
    for check in wanted {
        let mut result = match REPLAYS.iter().find(|(live, _)| live == check) {
            Some((_, session)) => {
                let replay = format!("{REPLAY_PREFIX}{session}");
                match replayed.iter().find(|r| r.check == replay) {
                    None => CheckResult::error(
                        check,
                        LAYER,
                        format!("this run has no {replay}: run the int part first"),
                    ),
                    Some(r) if r.outcome == Outcome::Pass => CheckResult::pass(
                        check,
                        LAYER,
                        format!("wtpool by {replay}; the OpenCode asserts need a live run"),
                    ),
                    Some(r) => {
                        CheckResult::assertion(check, LAYER, format!("{replay}: {}", r.message))
                    }
                }
            }
            None if check == SANDBOX_CHECK => CheckResult::skipped(
                check,
                LAYER,
                "no live OpenCode session ran, so none had to stay in the sandbox",
            ),
            None => CheckResult::not_run(
                check,
                LAYER,
                "no recorded session stands for it: it needs an installed opencode",
            ),
        };
        result.message = format!("{note}: {}", result.message);
        found.push(result.replay_only());
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_replay__should_judge_each_session_by_its_recording_alone() {
        // Given
        let replayed = vec![
            CheckResult::pass("oc_replay:subagent", "int", ""),
            CheckResult::assertion("oc_replay:delete_api", "int", "no ended record"),
        ];
        let wanted: BTreeSet<String> = [
            "oc_subagent",
            "oc_title",
            "oc_question",
            "oc_coordinator",
            SANDBOX_CHECK,
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();

        // When
        let found = by_replay(&replayed, &wanted, "no opencode is installed");

        // Then
        let outcomes: Vec<(&str, Outcome, Option<FailKind>)> = found
            .iter()
            .map(|r| (r.check.as_str(), r.outcome, r.kind))
            .collect();
        assert_eq!(
            outcomes,
            vec![
                (SANDBOX_CHECK, Outcome::Skipped, None),
                ("oc_coordinator", Outcome::NotRun, None),
                ("oc_question", Outcome::Fail, Some(FailKind::Error)),
                ("oc_subagent", Outcome::Pass, None),
                ("oc_title", Outcome::Fail, Some(FailKind::Assertion)),
            ]
        );
        assert!(
            found.iter().all(|r| r.replay_only
                && r.message
                    .starts_with("replay only (no opencode is installed): ")),
            "{found:?}"
        );
    }
}
