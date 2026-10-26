//! The checks the oracle implements and which part runs each.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

use crate::cmd;
use crate::trace;

/// Harness checks the xtask computes itself. Int checks come from the `int` test binary.
pub const HARNESS_CHECKS: [&str; 6] = [
    "oracle_unchanged",
    "repo_context__should_be_unchanged",
    "lint__no_suppressions",
    "trace_complete",
    "isolation__left_no_trace",
    crate::flows::NAME,
];

#[derive(Deserialize, Default)]
struct Absent {
    #[serde(default)]
    absent: Vec<AbsentEntry>,
}

#[derive(Deserialize)]
struct AbsentEntry {
    check: String,
}

pub fn absent_checks(root: &Path) -> Result<BTreeSet<String>, String> {
    let path = root.join("verify/absent.toml");
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let absent: Absent = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(absent.absent.into_iter().map(|entry| entry.check).collect())
}

/// Which part runs a check, `report` for the checks the report computes.
pub fn part_of(check: &str) -> &'static str {
    match check {
        "oracle_unchanged" | "repo_context__should_be_unchanged" | "lint__no_suppressions" => {
            "quick"
        }
        _ if check.starts_with("static:") || check.starts_with("harness:") => "quick",
        "trace_complete" | "isolation__left_no_trace" | "cover" | crate::flows::NAME => "report",
        _ => "int",
    }
}

/// Which part runs a check: the registry of verify/checks.toml first, then [`part_of`].
pub fn part_for(root: &Path, check: &str) -> &'static str {
    crate::web::Registry::load(root)
        .ok()
        .and_then(|registry| registry.part(check))
        .unwrap_or_else(|| part_of(check))
}

/// Every check the oracle in `root` implements.
pub fn implemented_checks(root: &Path) -> Result<BTreeSet<String>, String> {
    let mut checks: BTreeSet<String> = HARNESS_CHECKS.iter().map(|c| (*c).to_owned()).collect();
    for test in ["int"] {
        let listing = cmd::output(
            Command::new("cargo")
                .current_dir(root)
                .args([
                    "nextest",
                    "list",
                    "--locked",
                    "--manifest-path",
                    "xtask/Cargo.toml",
                    "--test",
                    test,
                ])
                .args(["--message-format", "json"]),
            Duration::from_secs(1800),
        )?;
        let json: serde_json::Value = serde_json::from_str(&listing).map_err(|e| e.to_string())?;
        let suites = json
            .get("rust-suites")
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default();
        for suite in suites.values() {
            if let Some(cases) = suite
                .get("testcases")
                .and_then(serde_json::Value::as_object)
            {
                checks.extend(cases.keys().map(|name| crate::junit::check_name(name)));
            }
        }
    }
    checks.extend(crate::web::Registry::load(root)?.layers.into_keys());
    let trace = trace::load(root)?;
    let scenarios = trace::load_scenarios(root)?;
    let mut known = trace.known_checks(&scenarios);
    // The flow checks are named by the User flows table, not the trace list.
    known.extend(crate::flows::check_names(root));
    let unknown: Vec<&String> = checks.iter().filter(|c| !known.contains(*c)).collect();
    if !unknown.is_empty() {
        return Err(format!("checks the trace list does not name: {unknown:?}"));
    }
    Ok(checks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::oracle("oracle_unchanged", "quick")]
    #[case::context("repo_context__should_be_unchanged", "quick")]
    #[case::static_layer("static:fmt", "quick")]
    #[case::trace("trace_complete", "report")]
    #[case::isolation("isolation__left_no_trace", "report")]
    #[case::int_check("version__should_list_version_commit_and_features", "int")]
    fn part_of__should_name_the_part_that_runs_a_check(#[case] check: &str, #[case] part: &str) {
        // Given
        let name = check;

        // When
        let found = part_of(name);

        // Then
        assert_eq!(found, part);
    }
}
