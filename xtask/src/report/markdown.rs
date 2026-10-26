//! The report as Markdown, and as HTML that shows that Markdown.

use std::fmt::Write as _;

use super::verdict::{Verdict, required_parts, what_to_do};
use crate::cover::Coverage;
use crate::flaky::{self, Streak};
use crate::results::{CheckResult, Outcome};

/// The oracle, `verify/accepted.txt` and spec diffs the report shows.
pub(super) struct Diffs {
    pub(super) oracle: String,
    pub(super) accepted: String,
    pub(super) spec: String,
}

/// The facts the report opens with.
pub(super) struct Header<'a> {
    pub(super) verdict: Verdict,
    pub(super) phase: u8,
    pub(super) run: &'a str,
    pub(super) commit: &'a str,
    pub(super) base: &'a str,
    pub(super) base_source: &'a str,
    pub(super) claude: &'a str,
    /// The judge of the cc checks: the cc part, or replay alone.
    pub(super) contract: &'a str,
}

fn outcome_text(result: &CheckResult) -> String {
    match (result.outcome, result.kind, &result.code) {
        (Outcome::Pass, _, _) if result.flaky => "pass (flaky)".to_owned(),
        (Outcome::Pass, _, _) => "pass".to_owned(),
        (Outcome::NeedsYou, _, Some(code)) => format!("needs you: {code}"),
        (Outcome::NeedsYou, _, None) => "needs you".to_owned(),
        (Outcome::NotRun, _, _) => "not run".to_owned(),
        (Outcome::Skipped, _, _) => "skipped".to_owned(),
        (Outcome::Fail, Some(kind), _) => format!("fail ({kind:?})").to_lowercase(),
        (Outcome::Fail, None, _) => "fail".to_owned(),
    }
}

pub(super) fn markdown(
    header: &Header,
    results: &[CheckResult],
    streaks: &[Streak],
    coverage: &Option<Coverage>,
    diffs: &Diffs,
    reused: &[String],
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Verify report, phase {}\n", header.phase);
    let _ = writeln!(out, "**Verdict: {}**\n", header.verdict);
    let _ = writeln!(out, "- Commit: {}", header.commit);
    let _ = writeln!(out, "- Base: {} ({})", header.base, header.base_source);
    let _ = writeln!(out, "- Claude Code: {}", header.claude);
    let _ = writeln!(out, "- cc checks: {}", header.contract);
    let _ = writeln!(out, "- Run: {}\n", header.run);

    let _ = writeln!(out, "## Reused parts\n");
    if reused.is_empty() {
        let _ = writeln!(out, "None: every part ran in this run.\n");
    }
    for line in reused {
        let _ = writeln!(out, "- {line}");
    }
    if !reused.is_empty() {
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "## Needs you\n");
    let needs: Vec<&CheckResult> = results
        .iter()
        .filter(|r| r.outcome == Outcome::NeedsYou)
        .collect();
    if needs.is_empty() {
        let _ = writeln!(out, "Nothing.\n");
    }
    for result in needs {
        let code = result.code.as_deref().unwrap_or("needs_you");
        let _ = writeln!(out, "- `{}`: {code}. {}", result.check, what_to_do(code));
        let _ = writeln!(out, "\n```\n{}\n```\n", result.message);
    }

    let _ = writeln!(
        out,
        "## Skipped on this machine\n\nChecks whose subject this machine lacks. They decide nothing and fail no verdict.\n"
    );
    let skipped: Vec<&CheckResult> = results
        .iter()
        .filter(|r| r.outcome == Outcome::Skipped)
        .collect();
    if skipped.is_empty() {
        let _ = writeln!(out, "None.\n");
    }
    for result in &skipped {
        let _ = writeln!(out, "- `{}`: {}", result.check, result.message);
    }
    if !skipped.is_empty() {
        let _ = writeln!(out);
    }

    let _ = writeln!(
        out,
        "## Oracle changes since the freeze\n\n```\n{}\n```\n",
        diffs.oracle.trim_end()
    );
    let _ = writeln!(
        out,
        "## verify/accepted.txt since the freeze\n\n```\n{}\n```\n",
        diffs.accepted.trim_end()
    );
    let _ = writeln!(
        out,
        "## Spec changes since the base\n\n```\n{}\n```\n",
        diffs.spec.trim_end()
    );

    let _ = writeln!(out, "## Failures\n");
    let failures: Vec<&CheckResult> = results
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Fail | Outcome::NotRun))
        .collect();
    if failures.is_empty() {
        let _ = writeln!(out, "None.\n");
    }
    for result in failures {
        let _ = writeln!(
            out,
            "### `{}` ({}, {})\n",
            result.check,
            result.layer,
            outcome_text(result)
        );
        let _ = writeln!(out, "```\n{}\n```\n", result.message.trim_end());
    }

    flaky_section(&mut out, streaks);
    budgets_section(&mut out, header.phase, results);

    let _ = writeln!(out, "## Coverage\n");
    match coverage {
        None => {
            let _ = writeln!(out, "Not measured.\n");
        }
        Some(coverage) => {
            let _ = writeln!(
                out,
                "{} of {} changed lines covered by unit or an oracle layer (int, render, host). Lines no oracle layer covers:\n",
                coverage.covered, coverage.changed_lines
            );
            if coverage.not_oracle_covered.is_empty() {
                let _ = writeln!(out, "None.\n");
            }
            for line in &coverage.not_oracle_covered {
                let mark = if line.unit_only {
                    " (unit only)"
                } else if coverage.not_measured.contains(&line.path) {
                    " (not measured)"
                } else {
                    " (none)"
                };
                let _ = writeln!(out, "- {}:{}{mark}", line.path, line.line);
            }
            let _ = writeln!(out);
        }
    }

    let _ = writeln!(
        out,
        "## Check results\n\n| Check | Layer | Result | Seconds |\n|---|---|---|---|"
    );
    for result in results {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {:.1} |",
            result.check,
            result.layer,
            outcome_text(result),
            result.seconds
        );
    }
    out
}

/// The checks that passed only on a retry, with the runs in a row they were flaky in.
fn flaky_section(out: &mut String, streaks: &[Streak]) {
    let _ = writeln!(
        out,
        "## Flaky checks\n\nChecks that passed only on a retry. One flaky in {} runs in a row fails.\n",
        flaky::IN_A_ROW
    );
    if streaks.is_empty() {
        let _ = writeln!(out, "None.\n");
    }
    for streak in streaks {
        let earlier = if streak.earlier.is_empty() {
            String::new()
        } else {
            format!(", also {}", streak.earlier.join(", "))
        };
        let _ = writeln!(
            out,
            "- `{}`: flaky in {} run(s) in a row{earlier}",
            streak.check,
            streak.runs()
        );
    }
    if !streaks.is_empty() {
        let _ = writeln!(out);
    }
}

/// The number of a budget check, `budget__b<n>`.
fn budget_number(check: &str) -> Option<u32> {
    check.strip_prefix("budget__b")?.parse().ok()
}

/// Each budget result, B1 to B11, with its layer, outcome and first line.
fn budgets_section(out: &mut String, phase: u8, results: &[CheckResult]) {
    let _ = writeln!(out, "## Budgets\n");
    let mut budgets: Vec<&CheckResult> = results
        .iter()
        .filter(|result| budget_number(&result.check).is_some())
        .collect();
    if budgets.is_empty() {
        let none = if required_parts(phase).contains(&"bench") {
            "None in this run."
        } else {
            "None in this phase."
        };
        let _ = writeln!(out, "{none}\n");
        return;
    }
    budgets.sort_by_key(|result| budget_number(&result.check));
    for result in budgets {
        let detail = result
            .message
            .lines()
            .next()
            .map(|line| format!(": {line}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "- `{}` ({}): {}{detail}",
            result.check,
            result.layer,
            outcome_text(result)
        );
    }
    let _ = writeln!(out);
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) fn html(markdown: &str, verdict: Verdict) -> String {
    let colour = match verdict {
        Verdict::Pass => "#1a7f37",
        Verdict::NeedsYou => "#9a6700",
        Verdict::Fail => "#cf222e",
    };
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><title>Verify report</title>\n<style>body{{font:14px/1.5 system-ui,sans-serif;max-width:1100px;margin:2em auto;padding:0 16px}}pre{{white-space:pre-wrap;background:#f6f8fa;padding:12px}}.verdict{{color:{colour};font-weight:700}}</style></head>\n<body><p class=\"verdict\">{}</p><pre>{}</pre></body></html>\n",
        verdict,
        escape(markdown)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn budgets_section__should_list_each_budget_in_order() {
        // Given
        let results = vec![
            CheckResult::assertion("budget__b10", "bench", "p95 9 ms over 5 ms\nsamples"),
            CheckResult::pass("idle_pool", "int", ""),
            CheckResult::pass("budget__b2", "bench", ""),
        ];
        let mut out = String::new();

        // When
        budgets_section(&mut out, 1, &results);

        // Then
        assert_eq!(
            out,
            "## Budgets\n\n- `budget__b2` (bench): pass\n- `budget__b10` (bench): fail (assertion): p95 9 ms over 5 ms\n\n"
        );
    }

    #[rstest]
    #[case::phase_without_bench(0, "None in this phase.")]
    #[case::run_without_budgets(1, "None in this run.")]
    fn budgets_section__should_say_why_none_is_listed(#[case] phase: u8, #[case] none: &str) {
        // Given
        let results = vec![CheckResult::pass("idle_pool", "int", "")];
        let mut out = String::new();

        // When
        budgets_section(&mut out, phase, &results);

        // Then
        assert_eq!(out, format!("## Budgets\n\n{none}\n\n"));
    }
}
