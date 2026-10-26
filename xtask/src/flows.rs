//! `flows__should_each_have_an_e2e_check`: every flow of the User flows table, read from the
//! harness's design document, has a passing `flow__<name>` check, and every `flow__` check names a
//! flow of the table.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::results::{CheckResult, Outcome};

pub const NAME: &str = "flows__should_each_have_an_e2e_check";
pub const PREFIX: &str = "flow__";
const DOC: &str = "docs/design/status-panel/verification.md";
const HEADING: &str = "## User flows";
const LAYER: &str = "report";

/// One row of the User flows table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flow {
    pub name: String,
    pub phase: u8,
}

/// Reads one table row, `| \`name\` | phase | steps | IDs |`. The header and rule give `None`.
pub fn parse_row(line: &str) -> Option<Flow> {
    let cells: Vec<&str> = line
        .trim()
        .strip_prefix('|')?
        .split('|')
        .map(str::trim)
        .collect();
    let name = cells.first()?.strip_prefix('`')?.strip_suffix('`')?;
    let phase = cells.get(1)?.parse().ok()?;
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    valid.then(|| Flow {
        name: name.to_owned(),
        phase,
    })
}

/// Every flow of the User flows section of `doc`.
pub fn flows(doc: &str) -> Vec<Flow> {
    let mut inside = false;
    let mut found = Vec::new();
    for line in doc.lines() {
        if line.starts_with("## ") {
            inside = line.trim() == HEADING;
            continue;
        }
        if inside && let Some(flow) = parse_row(line) {
            found.push(flow);
        }
    }
    found
}

/// The flows of the document in `root`, or the reason there are none.
pub fn load(root: &Path) -> Result<Vec<Flow>, String> {
    let doc = fs::read_to_string(root.join(DOC)).map_err(|e| format!("{DOC}: {e}"))?;
    let found = flows(&doc);
    if found.is_empty() {
        return Err(format!("{DOC} has no User flows table"));
    }
    Ok(found)
}

/// The `flow__` check names the oracle knows: one per flow of the table, any phase.
pub fn check_names(root: &Path) -> BTreeSet<String> {
    load(root)
        .unwrap_or_default()
        .into_iter()
        .map(|flow| format!("{PREFIX}{}", flow.name))
        .collect()
}

/// The problems the check reports: each flow of phase `phase` or earlier without a passing check,
/// and each `flow__` check, registered or reported, that names no flow.
pub fn problems(
    flows: &[Flow],
    phase: u8,
    registered: &BTreeSet<String>,
    outcomes: &BTreeMap<String, Outcome>,
) -> Vec<String> {
    let mut found = Vec::new();
    for flow in flows.iter().filter(|flow| flow.phase <= phase) {
        let check = format!("{PREFIX}{}", flow.name);
        match outcomes.get(&check) {
            Some(Outcome::Pass) => {}
            Some(outcome) => found.push(format!("{}: {check} is {outcome:?}", flow.name)),
            None if registered.contains(&check) => {
                found.push(format!("{}: {check} did not run", flow.name));
            }
            None => found.push(format!("{}: no {check} check", flow.name)),
        }
    }
    let names: BTreeSet<String> = flows.iter().map(|flow| flow.name.clone()).collect();
    let checks: BTreeSet<&String> = registered.iter().chain(outcomes.keys()).collect();
    for check in checks {
        if let Some(name) = check.strip_prefix(PREFIX)
            && !names.contains(name)
        {
            found.push(format!("{check} names no flow of the User flows table"));
        }
    }
    found
}

pub fn check(root: &Path, phase: u8, results: &[CheckResult]) -> CheckResult {
    let flows = match load(root) {
        Ok(flows) => flows,
        Err(error) => return CheckResult::error(NAME, LAYER, error),
    };
    let registered: BTreeSet<String> = match crate::web::Registry::load(root) {
        Ok(registry) => registry
            .layers
            .into_iter()
            .filter(|(check, _)| check.starts_with(PREFIX))
            .map(|(check, _)| check)
            .collect(),
        Err(error) => return CheckResult::error(NAME, LAYER, error),
    };
    let mut outcomes = crate::web::worst_outcomes(results);
    outcomes.retain(|check, _| check.starts_with(PREFIX));
    let found = problems(&flows, phase, &registered, &outcomes);
    if found.is_empty() {
        let count = flows.iter().filter(|flow| flow.phase <= phase).count();
        CheckResult::pass(
            NAME,
            LAYER,
            format!("{count} flows of phase {phase} or earlier passed"),
        )
    } else {
        CheckResult::assertion(NAME, LAYER, found.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::flow(
        "| `open_from_palette` | 1 | No tab open, run it. | X1, R1 |",
        Some(("open_from_palette", 1))
    )]
    #[case::later_phase("| `free_cancel` | 3 | Cancel. | X12 |", Some(("free_cancel", 3)))]
    #[case::header("| Flow | Phase | Steps and expected result | IDs |", None)]
    #[case::rule("|---|---|---|---|", None)]
    #[case::bad_name("| `Open-It` | 1 | x | X1 |", None)]
    #[case::prose("`open_from_palette` runs in phase 1.", None)]
    fn parse_row__should_read_name_and_phase(
        #[case] line: &str,
        #[case] expected: Option<(&str, u8)>,
    ) {
        // Given
        let text = line;

        // When
        let flow = parse_row(text);

        // Then
        assert_eq!(
            flow,
            expected.map(|(name, phase)| Flow {
                name: name.to_owned(),
                phase
            })
        );
    }

    #[test]
    fn flows__should_read_only_the_user_flows_section() {
        // Given
        let doc = "## Scenario catalog\n| `x_flow` | 1 | a | b |\n## User flows\n\n\
                   | Flow | Phase | Steps | IDs |\n|---|---|---|---|\n\
                   | `open_from_palette` | 1 | a | X1 |\n| `free_confirm` | 3 | b | X12 |\n\
                   ## Trace list\n| `later` | 1 | c | d |\n";

        // When
        let found = flows(doc);

        // Then
        let names: Vec<(&str, u8)> = found.iter().map(|f| (f.name.as_str(), f.phase)).collect();
        assert_eq!(names, vec![("open_from_palette", 1), ("free_confirm", 3)]);
    }

    fn flow(name: &str, phase: u8) -> Flow {
        Flow {
            name: name.to_owned(),
            phase,
        }
    }

    #[rstest]
    #[case::all_passed(&[("flow__a", Outcome::Pass)], &["flow__a"], &[])]
    #[case::failed(
        &[("flow__a", Outcome::Fail)],
        &["flow__a"],
        &["a: flow__a is Fail"]
    )]
    #[case::not_run(&[], &["flow__a"], &["a: flow__a did not run"])]
    #[case::missing(&[], &[], &["a: no flow__a check"])]
    #[case::unknown_check(
        &[("flow__a", Outcome::Pass), ("flow__zz", Outcome::Pass)],
        &["flow__a"],
        &["flow__zz names no flow of the User flows table"]
    )]
    #[case::unknown_registered(
        &[("flow__a", Outcome::Pass)],
        &["flow__a", "flow__yy"],
        &["flow__yy names no flow of the User flows table"]
    )]
    fn problems__should_need_a_passing_check_per_flow_of_the_phase(
        #[case] outcomes: &[(&str, Outcome)],
        #[case] registered: &[&str],
        #[case] expected: &[&str],
    ) {
        // Given
        let flows = vec![flow("a", 1), flow("later", 2)];
        let outcomes: BTreeMap<String, Outcome> = outcomes
            .iter()
            .map(|(c, o)| ((*c).to_owned(), *o))
            .collect();
        let registered: BTreeSet<String> = registered.iter().map(|c| (*c).to_owned()).collect();

        // When
        let found = problems(&flows, 1, &registered, &outcomes);

        // Then
        assert_eq!(found, expected);
    }
}
