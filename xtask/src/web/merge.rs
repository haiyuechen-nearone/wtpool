//! One result per check from the tests and layers that report it.

use std::collections::{BTreeMap, BTreeSet};

use super::registry::CHECKS;
use super::reporters::{TestOutcome, check_of};
use crate::results::{CheckResult, FailKind, Outcome};

/// The rank of an outcome when several tests or layers report one check: the worst wins.
fn severity(result: &CheckResult) -> u8 {
    match (result.outcome, result.kind) {
        (Outcome::Fail, Some(FailKind::Assertion)) => 6,
        (Outcome::Fail, Some(FailKind::Timeout)) => 5,
        (Outcome::Fail, _) => 4,
        (Outcome::NotRun, _) => 3,
        (Outcome::NeedsYou, _) => 2,
        (Outcome::Skipped, _) => 1,
        (Outcome::Pass, _) => 0,
    }
}

/// `first`, then `second` on a line of its own unless it is empty or the same text.
fn joined(first: &str, second: &str) -> String {
    if second.is_empty() || second == first {
        first.to_owned()
    } else if first.is_empty() {
        second.to_owned()
    } else {
        format!("{first}\n{second}")
    }
}

/// One result per check: the worst of its tests and layers, with every failure's message.
pub fn merge(results: Vec<CheckResult>) -> Vec<CheckResult> {
    let mut by_check: BTreeMap<String, CheckResult> = BTreeMap::new();
    for result in results {
        let Some(earlier) = by_check.get_mut(&result.check) else {
            by_check.insert(result.check.clone(), result);
            continue;
        };
        let mut layers: BTreeSet<&str> = earlier.layer.split('+').collect();
        layers.insert(&result.layer);
        let layer = layers.into_iter().collect::<Vec<_>>().join("+");
        let message = joined(&earlier.message, &result.message);
        let seconds = earlier.seconds + result.seconds;
        let flaky = earlier.flaky || result.flaky;
        if severity(&result) > severity(earlier) {
            *earlier = result;
        }
        earlier.flaky = flaky;
        earlier.layer = layer;
        earlier.message = message;
        earlier.seconds = seconds;
    }
    by_check.into_values().collect()
}

/// The outcome [`merge`] keeps for each check, without copying the results.
pub fn worst_outcomes(results: &[CheckResult]) -> BTreeMap<String, Outcome> {
    let mut worst: BTreeMap<&str, &CheckResult> = BTreeMap::new();
    for result in results {
        let kept = worst.entry(result.check.as_str()).or_insert(result);
        if severity(result) > severity(kept) {
            *kept = result;
        }
    }
    worst
        .into_iter()
        .map(|(check, result)| (check.to_owned(), result.outcome))
        .collect()
}

/// The result of one test of `layer` under its `check`. The message names the test when its
/// title is more than the check.
pub fn test_result(check: &str, layer: &str, test: TestOutcome) -> CheckResult {
    let message = if test.title == check || test.message.is_empty() {
        test.message
    } else {
        format!("{}: {}", test.title, test.message)
    };
    match (test.outcome, test.kind) {
        (Outcome::Fail, Some(kind)) => CheckResult::fail(check, layer, kind, message),
        (Outcome::Pass, _) => {
            let mut result = CheckResult::pass(check, layer, message);
            result.flaky = test.flaky;
            result
        }
        (Outcome::NotRun, _) => CheckResult::not_run(check, layer, "skipped"),
        (outcome, _) => {
            let mut result = CheckResult::error(check, layer, message);
            result.outcome = outcome;
            result
        }
    }
}

/// The results of one layer run: each test under its check, an error for a check the
/// registry does not give this layer, and for each wanted check of the layer that reported
/// nothing. A test of a check that `wanted` leaves out, such as one of a later phase that the
/// runner ran all the same, is not counted.
pub fn layer_results(
    layer: &str,
    tests: Vec<TestOutcome>,
    registered: &BTreeSet<String>,
    wanted: impl Fn(&str) -> bool,
) -> Vec<CheckResult> {
    let mut results = Vec::new();
    let mut seen = BTreeSet::new();
    for test in tests {
        let check = check_of(&test.title);
        if !registered.contains(&check) {
            let mut message = format!("`{}` names no check of {layer} in {CHECKS}", test.title);
            if !test.message.is_empty() {
                message.push('\n');
                message.push_str(&test.message);
            }
            results.push(CheckResult::error(
                &format!("{layer}:unregistered"),
                layer,
                message,
            ));
            continue;
        }
        if !wanted(&check) {
            continue;
        }
        results.push(test_result(&check, layer, test));
        seen.insert(check);
    }
    for check in registered {
        if wanted(check) && !seen.contains(check) {
            results.push(CheckResult::error(
                check,
                layer,
                format!("the {layer} layer reported no result for it"),
            ));
        }
    }
    merge(results)
}

#[cfg(test)]
mod tests {
    use super::super::reporters::{failed, simple};
    use super::*;
    use rstest::rstest;

    #[test]
    fn layer_results__should_merge_cases_and_flag_unknown_and_silent_checks() {
        // Given
        let tests = vec![
            simple("rows__should_follow_the_grammar a", Outcome::Pass),
            failed(
                "rows__should_follow_the_grammar b",
                FailKind::Assertion,
                "differs".into(),
            ),
            simple("typo__check", Outcome::Pass),
        ];
        let registered: BTreeSet<String> = [
            "rows__should_follow_the_grammar",
            "ages__should_use_the_format",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();

        // When
        let results = layer_results("render", tests, &registered, |_| true);

        // Then
        let summary: Vec<(String, Outcome, Option<FailKind>)> = results
            .iter()
            .map(|r| (r.check.clone(), r.outcome, r.kind))
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "ages__should_use_the_format".to_owned(),
                    Outcome::Fail,
                    Some(FailKind::Error)
                ),
                (
                    "render:unregistered".to_owned(),
                    Outcome::Fail,
                    Some(FailKind::Error)
                ),
                (
                    "rows__should_follow_the_grammar".to_owned(),
                    Outcome::Fail,
                    Some(FailKind::Assertion)
                ),
            ]
        );
    }

    #[test]
    fn layer_results__should_not_count_a_check_left_out() {
        // Given
        let tests = vec![
            simple("rows__should_follow_the_grammar", Outcome::Pass),
            failed(
                "heading__should_count_jobs",
                FailKind::Assertion,
                "no jobs".into(),
            ),
        ];
        let registered: BTreeSet<String> = [
            "rows__should_follow_the_grammar",
            "heading__should_count_jobs",
            "machine_strip__should_update_in_place",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        let later = [
            "heading__should_count_jobs",
            "machine_strip__should_update_in_place",
        ];

        // When
        let results = layer_results("render", tests, &registered, |check| {
            !later.contains(&check)
        });

        // Then
        let summary: Vec<(&str, Outcome)> = results
            .iter()
            .map(|r| (r.check.as_str(), r.outcome))
            .collect();
        assert_eq!(
            summary,
            vec![("rows__should_follow_the_grammar", Outcome::Pass)]
        );
    }

    #[rstest]
    #[case::one_flaky_case(true, false, true)]
    #[case::no_flaky_case(false, false, false)]
    fn layer_results__should_keep_a_pass_on_a_retry_flaky(
        #[case] first: bool,
        #[case] second: bool,
        #[case] expected: bool,
    ) {
        // Given
        let flaky = |title: &str, flaky: bool| TestOutcome {
            flaky,
            ..simple(title, Outcome::Pass)
        };
        let tests = vec![
            flaky("budget__b10", first),
            flaky("budget__b10 ignoring_eof", second),
        ];
        let registered = BTreeSet::from(["budget__b10".to_owned()]);

        // When
        let results = layer_results("host", tests, &registered, |_| true);

        // Then
        assert_eq!(
            results
                .iter()
                .map(|r| (r.check.as_str(), r.outcome, r.flaky))
                .collect::<Vec<_>>(),
            vec![("budget__b10", Outcome::Pass, expected)]
        );
    }

    #[rstest]
    #[case::assertion_beats_pass(
        Outcome::Pass,
        None,
        Outcome::Fail,
        Some(FailKind::Assertion),
        Some(FailKind::Assertion)
    )]
    #[case::assertion_beats_error(
        Outcome::Fail,
        Some(FailKind::Error),
        Outcome::Fail,
        Some(FailKind::Assertion),
        Some(FailKind::Assertion)
    )]
    #[case::not_run_beats_pass(Outcome::Pass, None, Outcome::NotRun, None, None)]
    fn merge__should_keep_the_worst_outcome(
        #[case] first: Outcome,
        #[case] first_kind: Option<FailKind>,
        #[case] second: Outcome,
        #[case] second_kind: Option<FailKind>,
        #[case] expected_kind: Option<FailKind>,
    ) {
        // Given
        let make = |outcome: Outcome, kind: Option<FailKind>, layer: &str| {
            let mut result = CheckResult::pass("idle_pool", layer, layer);
            result.outcome = outcome;
            result.kind = kind;
            result
        };
        let results = vec![
            make(first, first_kind, "int"),
            make(second, second_kind, "render"),
        ];

        // When
        let merged = merge(results.clone());
        let outcomes = worst_outcomes(&results);

        // Then
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].outcome, second);
        assert_eq!(merged[0].kind, expected_kind);
        assert_eq!(merged[0].layer, "int+render");
        assert_eq!(merged[0].message, "int\nrender");
        assert_eq!(outcomes, BTreeMap::from([("idle_pool".to_owned(), second)]));
    }

    #[rstest]
    #[case::both("a", "b", "a\nb")]
    #[case::same("a", "a", "a")]
    #[case::first_empty("", "b", "b")]
    #[case::second_empty("a", "", "a")]
    #[case::none("", "", "")]
    fn joined__should_keep_each_distinct_message_once(
        #[case] first: &str,
        #[case] second: &str,
        #[case] expected: &str,
    ) {
        // When
        let message = joined(first, second);

        // Then
        assert_eq!(message, expected);
    }
}
