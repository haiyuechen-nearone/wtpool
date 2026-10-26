//! Flaky checks. A check that passed only on a retry of its layer's runner is flaky, and a check
//! flaky in [`IN_A_ROW`] runs in a row fails. A run is a report of the same phase in `verify/out/`.
//! Earlier runs count newest first, by the time their report was written, and a run where the
//! check was not flaky ends the row.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use crate::results::{CheckResult, FailKind, Outcome};

/// The number of runs in a row a check may be flaky in before it fails.
pub const IN_A_ROW: usize = 3;

/// The flaky checks of one earlier run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub name: String,
    pub flaky: BTreeSet<String>,
}

/// A flaky check of this run and the earlier runs, newest first, it was flaky in just before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Streak {
    pub check: String,
    pub earlier: Vec<String>,
}

impl Streak {
    pub fn runs(&self) -> usize {
        self.earlier.len().saturating_add(1)
    }
}

/// The reports of the other runs of `phase` below `out_dir`, newest first.
pub fn earlier_runs(out_dir: &Path, run: &str, phase: u8) -> Vec<Run> {
    let Ok(entries) = fs::read_dir(out_dir) else {
        return Vec::new();
    };
    let mut found: Vec<(SystemTime, Run)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == run {
            continue;
        }
        let path = entry.path().join("report.json");
        let (Ok(text), Ok(meta)) = (fs::read_to_string(&path), fs::metadata(&path)) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if json.get("phase").and_then(serde_json::Value::as_u64) != Some(u64::from(phase)) {
            continue;
        }
        let flaky = json
            .get("results")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter(|result| result.get("flaky").and_then(serde_json::Value::as_bool) == Some(true))
            .filter_map(|result| result.get("check").and_then(serde_json::Value::as_str))
            .map(str::to_owned)
            .collect();
        let written = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        found.push((written, Run { name, flaky }));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    found.into_iter().map(|(_, run)| run).collect()
}

/// Each flaky check of `results` with its row of earlier flaky runs. A flaky check whose row
/// reaches [`IN_A_ROW`] runs with this one becomes an assertion failure.
pub fn apply(results: &mut [CheckResult], earlier: &[Run]) -> Vec<Streak> {
    let mut streaks = Vec::new();
    for result in results.iter_mut().filter(|result| result.flaky) {
        let row: Vec<String> = earlier
            .iter()
            .take_while(|run| run.flaky.contains(&result.check))
            .take(IN_A_ROW.saturating_sub(1))
            .map(|run| run.name.clone())
            .collect();
        let streak = Streak {
            check: result.check.clone(),
            earlier: row,
        };
        if streak.runs() >= IN_A_ROW && result.outcome == Outcome::Pass {
            result.outcome = Outcome::Fail;
            result.kind = Some(FailKind::Assertion);
            let note = format!(
                "flaky in {IN_A_ROW} runs in a row: this run and {}",
                streak.earlier.join(", ")
            );
            result.message = if result.message.is_empty() {
                note
            } else {
                format!("{note}\n{}", result.message)
            };
        }
        streaks.push(streak);
    }
    streaks
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn run(name: &str, flaky: &[&str]) -> Run {
        Run {
            name: name.to_owned(),
            flaky: flaky.iter().map(|check| (*check).to_owned()).collect(),
        }
    }

    #[rstest]
    #[case::first_time(&[][..], 1, Outcome::Pass)]
    #[case::twice(&[&["oversize_line"][..]][..], 2, Outcome::Pass)]
    #[case::three_in_a_row(&[&["oversize_line"][..], &["oversize_line"][..]][..], 3, Outcome::Fail)]
    #[case::row_broken(&[&["oversize_line"][..], &[][..], &["oversize_line"][..]][..], 2, Outcome::Pass)]
    #[case::other_check(&[&["garbage_line"][..], &["garbage_line"][..]][..], 1, Outcome::Pass)]
    fn apply__should_fail_a_check_flaky_in_three_runs_in_a_row(
        #[case] earlier: &[&[&str]],
        #[case] runs: usize,
        #[case] expected: Outcome,
    ) {
        // Given
        let earlier: Vec<Run> = earlier
            .iter()
            .enumerate()
            .map(|(index, flaky)| run(&format!("r{index}"), flaky))
            .collect();
        let mut flaky = CheckResult::pass("oversize_line", "host", "");
        flaky.flaky = true;
        let mut results = vec![flaky, CheckResult::pass("partial_lines", "host", "")];

        // When
        let streaks = apply(&mut results, &earlier);

        // Then
        assert_eq!(
            (
                streaks.len(),
                streaks[0].runs(),
                results[0].outcome,
                results[1].outcome
            ),
            (1, runs, expected, Outcome::Pass)
        );
    }

    #[test]
    fn earlier_runs__should_read_other_reports_of_the_phase_newest_first() {
        // Given
        let out = std::env::temp_dir().join(format!("wtpool-verify-flaky-{}", std::process::id()));
        let _ = fs::remove_dir_all(&out);
        let write = |run: &str, phase: u8, flaky: bool| {
            fs::create_dir_all(out.join(run)).unwrap();
            let json = serde_json::json!({"phase": phase, "results": [
                {"check": "garbage_line", "flaky": flaky}, {"check": "partial_lines", "flaky": false}
            ]});
            fs::write(out.join(run).join("report.json"), json.to_string()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        write("older", 1, true);
        write("other_phase", 2, true);
        write("newer", 1, false);
        write("this", 1, true);

        // When
        let runs = earlier_runs(&out, "this", 1);
        let _ = fs::remove_dir_all(&out);

        // Then
        assert_eq!(
            runs,
            vec![run("newer", &[]), run("older", &["garbage_line"])]
        );
    }
}
