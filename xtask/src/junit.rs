//! The JUnit reports nextest writes for the unit, int and bench tests.

use crate::results::{FailKind, Outcome};
use crate::web::TestOutcome;

/// The failure of a test case.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Failure {
    pub kind: FailKind,
    pub message: String,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TestCase {
    /// The test function name, with module path and rstest case removed.
    pub check: String,
    pub full_name: String,
    pub failure: Option<Failure>,
    pub skipped: bool,
}

/// Each test case of a nextest JUnit report as an outcome titled with its check. A failure's
/// message starts with the test case's full name.
pub fn to_outcomes(xml: &str) -> Vec<TestOutcome> {
    parse(xml).into_iter().map(outcome).collect()
}

/// A test case as an outcome of its check, as [`to_outcomes`] gives it.
pub fn outcome(case: TestCase) -> TestOutcome {
    match case.failure {
        Some(failure) => TestOutcome {
            message: format!("{}: {}", case.full_name, failure.message),
            title: case.check,
            outcome: Outcome::Fail,
            kind: Some(failure.kind),
            flaky: false,
        },
        None => TestOutcome {
            title: case.check,
            outcome: if case.skipped {
                Outcome::NotRun
            } else {
                Outcome::Pass
            },
            kind: None,
            message: String::new(),
            flaky: false,
        },
    }
}

/// Reads the `testcase` elements of a nextest JUnit report.
pub fn parse(xml: &str) -> Vec<TestCase> {
    let mut cases = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<testcase ") {
        let Some(after) = rest.get(start..) else {
            break;
        };
        let Some(open_end) = after.find('>') else {
            break;
        };
        let open = after.get(..open_end).unwrap_or_default();
        let self_closing = open.ends_with('/');
        let (body, next) = if self_closing {
            (
                "",
                after.get(open_end.saturating_add(1)..).unwrap_or_default(),
            )
        } else {
            let close = after.find("</testcase>").unwrap_or(after.len());
            (
                after
                    .get(open_end.saturating_add(1)..close)
                    .unwrap_or_default(),
                after.get(close..).unwrap_or_default(),
            )
        };
        let full_name = unescape(&attribute(open, "name").unwrap_or_default());
        let failure = failure_of(body);
        cases.push(TestCase {
            check: check_name(&full_name),
            full_name,
            failure,
            skipped: body.contains("<skipped"),
        });
        rest = next.get(1..).unwrap_or_default();
    }
    cases
}

/// `module::name::case_2_x` names the check `name`.
pub fn check_name(full_name: &str) -> String {
    full_name
        .split("::")
        .filter(|part| !part.starts_with("case_"))
        .last()
        .unwrap_or(full_name)
        .to_owned()
}

fn failure_of(body: &str) -> Option<Failure> {
    let tag = ["<failure", "<error"]
        .iter()
        .find_map(|tag| body.find(tag).map(|at| (at, *tag)))?;
    let element = body.get(tag.0..).unwrap_or_default();
    let open = element
        .get(..element.find('>').unwrap_or(element.len()))
        .unwrap_or_default();
    let kind_text = unescape(&attribute(open, "type").unwrap_or_default());
    let output = unescape(body);
    let message = panic_message(&output).unwrap_or_else(|| kind_text.clone());
    let kind = if kind_text.contains("timeout") {
        FailKind::Timeout
    } else if panic_kind_is_assertion(&message) {
        FailKind::Assertion
    } else {
        FailKind::Error
    };
    Some(Failure { kind, message })
}

/// True when the panic `message` is an assertion failure. Checks assert through `assert!` and its
/// relatives, whose message is free text, and fail their setup with a `setup:` message. Runtime
/// panics of the check's own code are errors too.
pub fn panic_kind_is_assertion(message: &str) -> bool {
    const ERRORS: [&str; 8] = [
        "setup:",
        "called `Option::unwrap()`",
        "called `Result::unwrap()`",
        "index out of bounds",
        "attempt to ",
        "explicit panic",
        "not implemented",
        "internal error",
    ];
    !message.is_empty() && !ERRORS.iter().any(|prefix| message.starts_with(prefix))
}

/// The first line of the panic message under `panicked at <location>:`.
pub fn panic_message(output: &str) -> Option<String> {
    let at = output.find("panicked at ")?;
    let after = output.get(at..)?;
    let mut lines = after.lines();
    let header = lines.next()?;
    if let Some((_, inline)) = header.split_once("': ") {
        return Some(inline.to_owned());
    }
    lines.next().map(str::to_owned)
}

fn attribute(open: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let start = open.find(&key)?.checked_add(key.len())?;
    let value = open.get(start..)?;
    let end = value.find('"')?;
    value.get(..end).map(str::to_owned)
}

pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(rest.get(..at).unwrap_or_default());
        let after = rest.get(at..).unwrap_or_default();
        let Some(end) = after.find(';') else {
            out.push_str(after);
            return out;
        };
        let entity = after.get(1..end).unwrap_or_default();
        let decoded = match entity {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = after.get(end.saturating_add(1)..).unwrap_or_default();
            }
            None => {
                out.push('&');
                rest = after.get(1..).unwrap_or_default();
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    const REPORT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="3" failures="2" errors="0">
<testsuite name="xtask::int" tests="3">
<testcase name="version__should_list_version_commit_and_features" classname="xtask::int" time="0.1">
<failure message="test failure" type="test failure">thread &apos;version&apos; panicked at tests/int.rs:9:5:
assertion `left == right` failed: no commit
  left: 1</failure>
<system-err>thread &apos;version&apos; panicked at tests/int.rs:9:5:
assertion `left == right` failed: no commit</system-err>
</testcase>
<testcase name="release__should_have_no_verify_feature" classname="xtask::int" time="0.1">
<failure message="test failure" type="test failure">thread &apos;r&apos; panicked at tests/support/mod.rs:3:5:
setup: WTPOOL_RELEASE_BIN is not set
</failure>
</testcase>
<testcase name="m::x__should_y::case_1_a" classname="xtask::int" time="0.1"/>
<testcase name="m::z__should_w" classname="xtask::int" time="0.0">
<skipped/>
</testcase>
</testsuite>
</testsuites>"#;

    #[test]
    fn parse__should_classify_each_testcase() {
        // Given
        let xml = REPORT;

        // When
        let cases = parse(xml);

        // Then
        let summary: Vec<(&str, Option<FailKind>, bool)> = cases
            .iter()
            .map(|case| {
                (
                    case.check.as_str(),
                    case.failure.as_ref().map(|failure| failure.kind),
                    case.skipped,
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "version__should_list_version_commit_and_features",
                    Some(FailKind::Assertion),
                    false
                ),
                (
                    "release__should_have_no_verify_feature",
                    Some(FailKind::Error),
                    false
                ),
                ("x__should_y", None, false),
                ("z__should_w", None, true),
            ]
        );
    }

    #[test]
    fn to_outcomes__should_name_the_test_case_in_each_failure() {
        // Given
        let xml = REPORT;

        // When
        let outcomes = to_outcomes(xml);

        // Then
        let summary: Vec<(&str, Outcome, Option<FailKind>, &str)> = outcomes
            .iter()
            .map(|t| (t.title.as_str(), t.outcome, t.kind, t.message.as_str()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "version__should_list_version_commit_and_features",
                    Outcome::Fail,
                    Some(FailKind::Assertion),
                    "version__should_list_version_commit_and_features: assertion `left == right` failed: no commit"
                ),
                (
                    "release__should_have_no_verify_feature",
                    Outcome::Fail,
                    Some(FailKind::Error),
                    "release__should_have_no_verify_feature: setup: WTPOOL_RELEASE_BIN is not set"
                ),
                ("x__should_y", Outcome::Pass, None, ""),
                ("z__should_w", Outcome::NotRun, None, ""),
            ]
        );
    }

    #[rstest]
    #[case::assert_eq("assertion `left == right` failed: x", true)]
    #[case::assert_with_message("the package declares no `verify` feature", true)]
    #[case::setup("setup: WTPOOL_BIN is not set", false)]
    #[case::unwrap("called `Option::unwrap()` on a `None` value", false)]
    #[case::overflow("attempt to add with overflow", false)]
    #[case::empty("", false)]
    fn panic_kind_is_assertion__should_split_assertions_from_errors(
        #[case] message: &str,
        #[case] expected: bool,
    ) {
        // Given
        let text = message;

        // When
        let assertion = panic_kind_is_assertion(text);

        // Then
        assert_eq!(assertion, expected);
    }

    #[rstest]
    #[case::plain("a__should_b", "a__should_b")]
    #[case::module("tests::a__should_b", "a__should_b")]
    #[case::rstest_case("a__should_b::case_3_text", "a__should_b")]
    fn check_name__should_drop_module_and_case(#[case] full: &str, #[case] expected: &str) {
        // Given
        let name = full;

        // When
        let check = check_name(name);

        // Then
        assert_eq!(check, expected);
    }

    #[rstest]
    #[case::named("&lt;a&gt; &amp; &quot;b&quot; &apos;c&apos;", "<a> & \"b\" 'c'")]
    #[case::numeric("&#65;&#x42;", "AB")]
    #[case::stray("a & b", "a & b")]
    fn unescape__should_decode_entities(#[case] text: &str, #[case] expected: &str) {
        // Given
        let escaped = text;

        // When
        let plain = unescape(escaped);

        // Then
        assert_eq!(plain, expected);
    }
}
