//! `trace_complete`: every ID and every harness check of [`TRACE`] up to the run's phase passed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::results::{CheckResult, Outcome};

pub const TRACE: &str = "verify/trace.toml";
pub const NAME: &str = "trace_complete";

#[derive(Deserialize, Debug, Default)]
pub struct Trace {
    pub harness: BTreeMap<String, u8>,
    #[serde(default)]
    pub check_phase: BTreeMap<String, u8>,
    pub ids: BTreeMap<String, IdEntry>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct IdEntry {
    pub phase: u8,
    pub checks: Vec<String>,
    #[serde(default)]
    pub ids: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Scenario {
    #[serde(skip)]
    pub name: String,
    pub phase: u8,
    pub covers: Vec<String>,
}

pub fn load(root: &Path) -> Result<Trace, String> {
    let text = fs::read_to_string(root.join(TRACE)).map_err(|e| format!("{TRACE}: {e}"))?;
    toml::from_str(&text).map_err(|e| format!("{TRACE}: {e}"))
}

pub fn load_scenarios(root: &Path) -> Result<Vec<Scenario>, String> {
    let dir = root.join("verify/scenarios");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut scenarios = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut scenario: Scenario =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        scenario.name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        scenarios.push(scenario);
    }
    scenarios.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(scenarios)
}

impl Trace {
    /// Every check name the trace and the scenarios know.
    pub fn known_checks(&self, scenarios: &[Scenario]) -> BTreeSet<String> {
        let mut known: BTreeSet<String> = self.harness.keys().cloned().collect();
        for entry in self.ids.values() {
            known.extend(entry.checks.iter().cloned());
        }
        known.extend(scenarios.iter().map(|s| s.name.clone()));
        known
    }

    /// The checks an ID needs at `phase`: its own and its scenarios, plus those of the IDs it
    /// names, each from the latest of the IDs' phases and the check's own phase.
    pub fn checks_of(&self, id: &str, scenarios: &[Scenario], phase: u8) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        let mut seen = BTreeSet::new();
        self.collect(id, scenarios, phase, 0, &mut seen, &mut found);
        found
    }

    fn collect(
        &self,
        id: &str,
        scenarios: &[Scenario],
        phase: u8,
        floor: u8,
        seen: &mut BTreeSet<String>,
        found: &mut BTreeSet<String>,
    ) {
        if !seen.insert(id.to_owned()) {
            return;
        }
        let Some(entry) = self.ids.get(id) else {
            return;
        };
        let floor = floor.max(entry.phase);
        for check in &entry.checks {
            let own = self.check_phase.get(check).copied().unwrap_or(0);
            if floor.max(own) <= phase {
                found.insert(check.clone());
            }
        }
        for scenario in scenarios
            .iter()
            .filter(|s| s.covers.iter().any(|c| c == id))
        {
            if floor.max(scenario.phase) <= phase {
                found.insert(scenario.name.clone());
            }
        }
        for named in &entry.ids {
            self.collect(named, scenarios, phase, floor, seen, found);
        }
    }

    /// The phase each check of the trace and the scenarios runs from. A check an ID names runs
    /// from the later of that ID's phase and its own in [`Trace::check_phase`], the earliest over
    /// the IDs that name it. A scenario runs from the later of its phase and its own.
    pub fn first_phases(&self, scenarios: &[Scenario]) -> BTreeMap<String, u8> {
        let mut found: BTreeMap<String, u8> = BTreeMap::new();
        let mut note = |check: &str, phase: u8| {
            let own = self.check_phase.get(check).copied().unwrap_or(0);
            let from = phase.max(own);
            found
                .entry(check.to_owned())
                .and_modify(|earliest| *earliest = (*earliest).min(from))
                .or_insert(from);
        };
        for entry in self.ids.values() {
            for check in &entry.checks {
                note(check, entry.phase);
            }
        }
        for scenario in scenarios {
            note(&scenario.name, scenario.phase);
        }
        found
    }

    /// The checks of the trace and the scenarios that run only from a phase later than `phase`.
    /// A part of `phase` leaves them out, so a run of an earlier phase never judges a later
    /// phase's checks.
    pub fn later_checks(&self, scenarios: &[Scenario], phase: u8) -> BTreeSet<String> {
        self.first_phases(scenarios)
            .into_iter()
            .filter(|(_, from)| *from > phase)
            .map(|(check, _)| check)
            .collect()
    }

    fn has_any_check(&self, id: &str, scenarios: &[Scenario]) -> bool {
        !self.checks_of(id, scenarios, u8::MAX).is_empty()
    }
}

/// One problem `trace_complete` reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceProblem {
    pub text: String,
    /// It is a needs you code, not a failure.
    pub needs_you: bool,
}

impl TraceProblem {
    fn failed(text: String) -> Self {
        Self {
            text,
            needs_you: false,
        }
    }
}

/// The problem of `owner`'s `check` by its outcome, `None` when it passed or was skipped.
fn judge(check: &str, owner: &str, outcome: Option<Outcome>) -> Option<TraceProblem> {
    match outcome {
        // A skipped check's subject does not exist on this machine. The report lists it.
        Some(Outcome::Pass | Outcome::Skipped) => None,
        Some(Outcome::NeedsYou) => Some(TraceProblem {
            text: format!("{owner}: {check} needs you"),
            needs_you: true,
        }),
        Some(Outcome::Fail) => Some(TraceProblem::failed(format!("{owner}: {check} failed"))),
        Some(Outcome::NotRun) | None => Some(TraceProblem::failed(format!(
            "{owner}: {check} did not run"
        ))),
    }
}

/// The problems `trace_complete` reports.
pub fn problems(
    trace: &Trace,
    scenarios: &[Scenario],
    phase: u8,
    outcomes: &BTreeMap<String, Outcome>,
) -> Vec<TraceProblem> {
    let outcome = |check: &str| outcomes.get(check).copied();
    let mut problems = Vec::new();
    for (id, entry) in &trace.ids {
        if entry.phase > phase {
            continue;
        }
        if !trace.has_any_check(id, scenarios) {
            problems.push(TraceProblem::failed(format!("{id}: no check")));
            continue;
        }
        for check in trace.checks_of(id, scenarios, phase) {
            problems.extend(judge(&check, id, outcome(&check)));
        }
    }
    for (check, check_phase) in &trace.harness {
        if *check_phase <= phase && check != NAME {
            problems.extend(judge(check, "harness", outcome(check)));
        }
    }
    problems
}

/// The checks of `root`'s trace and scenarios that run only from a phase after `phase`, none when
/// the trace cannot be read, a fault `trace_complete` reports. A flow check runs from the phase of
/// its flow in [`crate::flows::load`], also when an ID of a later phase names it.
pub fn later_in(root: &Path, phase: u8) -> BTreeSet<String> {
    let mut later = load(root)
        .and_then(|trace| Ok(trace.later_checks(&load_scenarios(root)?, phase)))
        .unwrap_or_default();
    for flow in crate::flows::load(root).unwrap_or_default() {
        if flow.phase <= phase {
            later.remove(&format!("{}{}", crate::flows::PREFIX, flow.name));
        }
    }
    later
}

/// The nextest filter ` and not test(...)` that leaves `later` out, or nothing for none.
pub fn nextest_exclusion(later: &BTreeSet<String>) -> String {
    if later.is_empty() {
        return String::new();
    }
    let names: Vec<&str> = later.iter().map(String::as_str).collect();
    format!(" and not test(/(^|::)({})($|::)/)", names.join("|"))
}

pub fn check(root: &Path, phase: u8, results: &[CheckResult]) -> CheckResult {
    let trace = match load(root) {
        Ok(trace) => trace,
        Err(error) => return CheckResult::error(NAME, "report", error),
    };
    let scenarios = match load_scenarios(root) {
        Ok(scenarios) => scenarios,
        Err(error) => return CheckResult::assertion(NAME, "report", error),
    };
    // A check that runs in several layers or parts, as a scenario does, counts by its worst
    // outcome.
    let outcomes = crate::web::worst_outcomes(results);
    let problems = problems(&trace, &scenarios, phase, &outcomes);
    let listed = problems
        .iter()
        .map(|problem| problem.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let skipped: Vec<&str> = outcomes
        .iter()
        .filter(|(_, outcome)| **outcome == Outcome::Skipped)
        .map(|(check, _)| check.as_str())
        .collect();
    if problems.is_empty() && skipped.is_empty() {
        CheckResult::pass(
            NAME,
            "report",
            format!("every ID and harness check of phase {phase} passed"),
        )
    } else if problems.is_empty() {
        CheckResult::pass(
            NAME,
            "report",
            format!(
                "every ID and harness check of phase {phase} passed or was skipped on this machine: {}",
                skipped.join(", ")
            ),
        )
    } else if problems.iter().all(|problem| problem.needs_you) {
        CheckResult::needs_you(NAME, "report", "needs_you", listed)
    } else {
        CheckResult::assertion(NAME, "report", listed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn trace() -> Trace {
        let text = r#"
            [harness]
            trace_complete = 0
            version__x = 0
            catalog__y = 1

            [check_phase]
            budget__b1 = 2

            [ids.R7]
            phase = 1
            checks = ["budget__b1", "budget__b5"]

            [ids.R8]
            phase = 1
            checks = []
            ids = ["S2", "X12"]

            [ids.S2]
            phase = 1
            checks = ["status__z"]

            [ids.X12]
            phase = 3
            checks = ["actions__a"]

            [ids.A5]
            phase = 1
            checks = []

            [ids.D9]
            phase = 1
            checks = []
        "#;
        toml::from_str(text).unwrap()
    }

    fn scenarios() -> Vec<Scenario> {
        vec![Scenario {
            name: "idle_pool".into(),
            phase: 1,
            covers: vec!["A5".into()],
        }]
    }

    #[rstest]
    #[case::own_phase_later_than_id("R7", 1, &["budget__b5"])]
    #[case::own_phase_reached("R7", 2, &["budget__b1", "budget__b5"])]
    #[case::named_id_from_its_own_phase("R8", 1, &["status__z"])]
    #[case::named_later_id_reached("R8", 3, &["actions__a", "status__z"])]
    #[case::scenario_only("A5", 1, &["idle_pool"])]
    #[case::before_the_scenario_phase("A5", 0, &[])]
    fn checks_of__should_follow_phases_and_named_ids(
        #[case] id: &str,
        #[case] phase: u8,
        #[case] expected: &[&str],
    ) {
        // Given
        let trace = trace();

        // When
        let checks = trace.checks_of(id, &scenarios(), phase);

        // Then
        assert_eq!(
            checks,
            expected
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<BTreeSet<_>>()
        );
    }

    #[rstest]
    #[case::phase_1(1, &["actions__a", "budget__b1"])]
    #[case::phase_2(2, &["actions__a"])]
    #[case::phase_3(3, &[])]
    fn later_checks__should_leave_out_what_runs_by_then(
        #[case] phase: u8,
        #[case] expected: &[&str],
    ) {
        // Given
        let trace = trace();

        // When
        let later = trace.later_checks(&scenarios(), phase);

        // Then
        assert_eq!(
            later,
            expected
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<BTreeSet<_>>()
        );
    }

    fn outcomes(pairs: &[(&str, Outcome)]) -> BTreeMap<String, Outcome> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
    }

    #[rstest]
    #[case::phase_0_all_pass(0, &[("version__x", Outcome::Pass)], &[])]
    #[case::phase_0_harness_failed(0, &[("version__x", Outcome::Fail)], &["harness: version__x failed"])]
    #[case::phase_0_harness_missing(0, &[], &["harness: version__x did not run"])]
    #[case::phase_0_harness_needs_you(0, &[("version__x", Outcome::NeedsYou)], &["harness: version__x needs you"])]
    #[case::phase_1_id_without_checks(
        1,
        &[
            ("version__x", Outcome::Pass),
            ("catalog__y", Outcome::Pass),
            ("budget__b5", Outcome::Pass),
            ("status__z", Outcome::Pass),
            ("idle_pool", Outcome::Pass),
        ],
        &["D9: no check"]
    )]
    fn problems__should_list_every_unpassed_check(
        #[case] phase: u8,
        #[case] given: &[(&str, Outcome)],
        #[case] expected: &[&str],
    ) {
        // Given
        let outcomes = outcomes(given);

        // When
        let problems = problems(&trace(), &scenarios(), phase, &outcomes);

        // Then
        let texts: Vec<String> = problems.into_iter().map(|problem| problem.text).collect();
        assert_eq!(
            texts,
            expected.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()
        );
    }

    /// A check name: snake case with a `__` or `_`, or `harness:` and a snake case name.
    fn is_check_name(name: &str) -> bool {
        let snake = |text: &str| {
            !text.is_empty()
                && text
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        };
        match name.strip_prefix("harness:") {
            Some(rest) => snake(rest),
            None => name.contains('_') && snake(name),
        }
    }

    #[rstest]
    #[case::snake("trace_complete", true)]
    #[case::harness("harness:fmt", true)]
    #[case::harness_snake("harness:self_test", true)]
    #[case::other_prefix("static:fmt", false)]
    #[case::bare_prefix("harness:", false)]
    #[case::one_word("skipped", false)]
    #[case::file("CLAUDE.md", false)]
    fn is_check_name__should_take_snake_case_and_harness_names(
        #[case] name: &str,
        #[case] expected: bool,
    ) {
        // When
        let found = is_check_name(name);

        // Then
        assert_eq!(found, expected);
    }

    /// The harness checks the paragraph of `text` that starts `Harness checks carry no ID` names,
    /// each with the phase of its `(phase N` note, else 0.
    fn doc_harness(text: &str) -> BTreeMap<String, u8> {
        let start = text.find("Harness checks carry no ID").unwrap();
        let paragraph = &text[start..];
        let paragraph = &paragraph[..paragraph.find("\n\n").unwrap_or(paragraph.len())];
        let mut found = BTreeMap::new();
        let mut rest = paragraph;
        while let Some(open) = rest.find('`') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('`') else {
                break;
            };
            let name = &after[..close];
            rest = &after[close + 1..];
            if is_check_name(name) {
                let phase = rest
                    .strip_prefix(" (phase ")
                    .and_then(|note| note.chars().next())
                    .and_then(|digit| digit.to_digit(10))
                    .map_or(0, |digit| u8::try_from(digit).unwrap());
                found.insert(name.to_owned(), phase);
            }
        }
        found
    }

    #[test]
    fn trace_file__should_list_the_harness_checks_of_the_trace_list() {
        // Given
        let root = crate::repo::root();
        let doc =
            fs::read_to_string(root.join("docs/design/status-panel/verification.md")).unwrap();

        // When
        let trace = load(&root).unwrap();

        // Then
        assert_eq!(trace.harness, doc_harness(&doc));
    }

    /// The IDs the scenario catalog covers, those every scenario covers among them.
    const CATALOG_COVERS: [&str; 26] = [
        "R1", "R2", "R3", "R4", "A1", "A2", "A3", "A4", "A5", "S3", "S4", "S8", "U1", "U3", "X10",
        "H6", "H9", "H10", "H11", "H12", "H13", "H15", "H19", "H20", "H21", "H24",
    ];

    /// The phase of `id`, kept apart from [`TRACE`] so a misphased entry there fails.
    fn spec_phase(id: &str) -> u8 {
        let later = [
            "R4", "D5", "A1", "A2", "A3", "A4", "A7", "S5", "S6", "W6", "X18",
        ];
        let jobs = [
            "H29", "H31", "A8", "S14", "S15", "W10", "B12", "B13", "B14", "U10", "U11", "U12",
            "U13", "U14", "U15", "U17", "X20", "X21", "X22",
        ];
        let job_row = id
            .strip_prefix('J')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n));
        if job_row || jobs.contains(&id) {
            4
        } else if ["X12", "H27", "H28", "S13", "R12", "U9", "X19"].contains(&id) {
            3
        } else if id.starts_with('H') || later.contains(&id) {
            2
        } else {
            1
        }
    }

    #[test]
    fn trace_file__should_name_every_id_once_with_a_check_or_scenario_owner() {
        // Given
        let root = crate::repo::root();

        // When
        let trace = load(&root).unwrap();

        // Then
        let ids_per_letter = [
            ("R", 11),
            ("D", 4),
            ("H", 30),
            ("A", 8),
            ("S", 15),
            ("W", 10),
            ("X", 22),
            ("U", 16),
            ("B", 14),
            ("J", 24),
        ];
        let expected_ids: usize = ids_per_letter.iter().map(|(_, count)| count).sum();
        assert_eq!(trace.ids.len(), expected_ids);
        let ownerless: Vec<&String> = trace
            .ids
            .iter()
            .filter(|(id, entry)| {
                entry.checks.is_empty()
                    && entry.ids.is_empty()
                    && !CATALOG_COVERS.contains(&id.as_str())
            })
            .map(|(id, _)| id)
            .collect();
        assert!(ownerless.is_empty(), "IDs without an owner: {ownerless:?}");
        let unknown: Vec<&String> = trace
            .ids
            .values()
            .flat_map(|entry| &entry.ids)
            .filter(|named| !trace.ids.contains_key(*named))
            .collect();
        assert!(unknown.is_empty(), "named IDs the trace lacks: {unknown:?}");
        let misphased: Vec<(&String, u8)> = trace
            .ids
            .iter()
            .filter(|(id, entry)| entry.phase != spec_phase(id))
            .map(|(id, entry)| (id, entry.phase))
            .collect();
        assert!(
            misphased.is_empty(),
            "IDs with another phase: {misphased:?}"
        );
    }

    #[test]
    fn later_in__should_keep_a_flow_from_the_phase_of_its_row() {
        // Given
        let root = crate::repo::root();

        // When
        let later = later_in(&root, 3);

        // Then
        assert!(!later.contains("flow__keyboard_only"), "{later:?}");
        assert!(later.contains("flow__jobs_first_open"), "{later:?}");
    }
}
