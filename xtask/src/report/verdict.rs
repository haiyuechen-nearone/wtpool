//! The verdict of a run and the parts each phase needs for one.

use std::fmt;

use serde::Serialize;

use crate::results::{CheckResult, Outcome};

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    #[serde(rename = "PASS")]
    Pass,
    #[serde(rename = "NEEDS YOU")]
    NeedsYou,
    #[serde(rename = "FAIL")]
    Fail,
}

impl Verdict {
    pub fn text(self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::NeedsYou => "NEEDS YOU",
            Verdict::Fail => "FAIL",
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.text())
    }
}

/// The parts each phase needs before a report can pass.
pub fn required_parts(phase: u8) -> &'static [&'static str] {
    match phase {
        0 => &["quick", "int"],
        1 => &["quick", "int", "ui", "bench"],
        _ => &["quick", "int", "ui", "bench", "cc"],
    }
}

pub fn verdict(results: &[CheckResult]) -> Verdict {
    if results
        .iter()
        .any(|r| matches!(r.outcome, Outcome::Fail | Outcome::NotRun))
    {
        Verdict::Fail
    } else if results.iter().any(|r| r.outcome == Outcome::NeedsYou) {
        Verdict::NeedsYou
    } else {
        Verdict::Pass
    }
}

pub fn what_to_do(code: &str) -> &'static str {
    match code {
        "oracle_change" => {
            "Review the entry in verify/oracle-changes.toml, then add the `accept <id> <entry hash>` line the message prints to verify/accepted.txt, or leave it out to reject it."
        }
        "logged_out" => "Log in to Claude Code, then rerun the cc part.",
        "no_network" => "Restore the network, then rerun the part.",
        "machine_busy" => "Rerun the part when the machine is idle.",
        "inconclusive" => "Claude Code did not follow the script: rerun the session.",
        "assumption_broken" => {
            "Claude Code behaves differently from the spec: read the recording and decide."
        }
        _ => "Read the listed checks.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::all_pass(&[Outcome::Pass, Outcome::Pass], Verdict::Pass)]
    #[case::needs_you(&[Outcome::Pass, Outcome::NeedsYou], Verdict::NeedsYou)]
    #[case::fail_wins(&[Outcome::NeedsYou, Outcome::Fail], Verdict::Fail)]
    #[case::not_run_fails(&[Outcome::Pass, Outcome::NotRun], Verdict::Fail)]
    fn verdict__should_put_fail_before_needs_you(
        #[case] outcomes: &[Outcome],
        #[case] expected: Verdict,
    ) {
        // Given
        let results: Vec<CheckResult> = outcomes
            .iter()
            .map(|outcome| {
                let mut result = CheckResult::pass("c", "l", "");
                result.outcome = *outcome;
                result
            })
            .collect();

        // When
        let verdict = verdict(&results);

        // Then
        assert_eq!(verdict, expected);
    }

    #[test]
    fn verdict__should_display_its_report_text() {
        // When
        let shown = [Verdict::Pass, Verdict::NeedsYou, Verdict::Fail].map(|v| v.to_string());

        // Then
        assert_eq!(shown, ["PASS", "NEEDS YOU", "FAIL"]);
    }
}
