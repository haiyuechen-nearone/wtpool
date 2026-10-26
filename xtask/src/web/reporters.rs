//! The JSON files the JavaScript layers' reporters write: Vitest, Mocha and Playwright, and the
//! test titles they match for `--only`.

use std::path::Path;

use crate::results::{FailKind, Outcome};

/// One test of a JavaScript layer as its reporter wrote it.
#[derive(Debug, Clone, PartialEq)]
pub struct TestOutcome {
    pub title: String,
    pub outcome: Outcome,
    pub kind: Option<FailKind>,
    pub message: String,
    /// It passed only on a retry.
    pub flaky: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Vitest's `json` reporter.
    Vitest,
    /// Mocha's `json` reporter.
    Mocha,
    /// Playwright's `json` reporter.
    Playwright,
    /// Outcomes a check writes for other checks ([`parse_extra`]).
    Extra,
}

/// The check a test title names: its first word. `rows__should_follow_the_grammar idle` and
/// `idle_pool` name `rows__should_follow_the_grammar` and `idle_pool`.
pub fn check_of(title: &str) -> String {
    title
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Removes terminal colour sequences, as Playwright puts them in its error messages.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The kind of a failure: an assertion about the product, a timeout or an error of the check.
/// Checks assert with `node:assert` or `expect`, and fail their setup with a `setup:` message.
pub fn classify(text: &str) -> FailKind {
    let text = strip_ansi(text);
    let first = text.trim_start();
    let (name, rest) = match first.split_once(": ") {
        Some((name, rest))
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || " []_".contains(c)) =>
        {
            (name, rest)
        }
        _ => ("", first),
    };
    if rest.trim_start().starts_with("setup:") {
        return FailKind::Error;
    }
    if first.starts_with("Timeout of")
        || first.contains("Test timed out in")
        || first.contains("Timeout of ") && first.contains("ms exceeded")
    {
        return FailKind::Timeout;
    }
    if name.starts_with("AssertionError") || first.contains("expect(") {
        return FailKind::Assertion;
    }
    FailKind::Error
}

fn str_of<'a>(value: &'a serde_json::Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

fn array_of<'a>(value: &'a serde_json::Value, key: &str) -> &'a [serde_json::Value] {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

pub(crate) fn failed(title: &str, kind: FailKind, message: String) -> TestOutcome {
    TestOutcome {
        title: title.to_owned(),
        outcome: Outcome::Fail,
        kind: Some(kind),
        message,
        flaky: false,
    }
}

pub(crate) fn simple(title: &str, outcome: Outcome) -> TestOutcome {
    TestOutcome {
        title: title.to_owned(),
        outcome,
        kind: None,
        message: String::new(),
        flaky: false,
    }
}

/// A pass that needed `retries` retries, flaky when there was one.
pub(crate) fn passed(title: &str, retries: u64) -> TestOutcome {
    TestOutcome {
        flaky: retries > 0,
        ..simple(title, Outcome::Pass)
    }
}

fn first_lines(text: &str, count: usize) -> String {
    strip_ansi(text)
        .lines()
        .take(count)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Reads a reporter's JSON file. A file the layer could not even load shows as a test whose
/// title starts with `file:`.
pub fn parse(format: Format, text: &str) -> Result<Vec<TestOutcome>, String> {
    if format == Format::Extra {
        return parse_extra(text);
    }
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("the results file is no JSON: {e}"))?;
    let mut found = Vec::new();
    match format {
        Format::Vitest => {
            for file in array_of(&json, "testResults") {
                let tests = array_of(file, "assertionResults");
                if tests.is_empty() && str_of(file, "status") == "failed" {
                    let name = Path::new(str_of(file, "name"))
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let message = str_of(file, "message");
                    found.push(failed(
                        &format!("file:{name}"),
                        FailKind::Error,
                        first_lines(message, 8),
                    ));
                }
                for test in tests {
                    let title = str_of(test, "title");
                    found.push(match str_of(test, "status") {
                        "passed" => simple(title, Outcome::Pass),
                        "failed" => {
                            let message = array_of(test, "failureMessages")
                                .iter()
                                .filter_map(serde_json::Value::as_str)
                                .collect::<Vec<_>>()
                                .join("\n");
                            failed(title, classify(&message), first_lines(&message, 6))
                        }
                        _ => simple(title, Outcome::NotRun),
                    });
                }
            }
        }
        Format::Mocha => {
            for test in array_of(&json, "passes") {
                let retries = test
                    .get("currentRetry")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                found.push(passed(str_of(test, "title"), retries));
            }
            for test in array_of(&json, "pending") {
                found.push(simple(str_of(test, "title"), Outcome::NotRun));
            }
            for test in array_of(&json, "failures") {
                let err = test.get("err").unwrap_or(&serde_json::Value::Null);
                let stack = str_of(err, "stack");
                let message = str_of(err, "message");
                let code = str_of(err, "code");
                let text = if stack.is_empty() {
                    message.to_owned()
                } else {
                    stack.to_owned()
                };
                let mut kind = classify(&text);
                if kind == FailKind::Error
                    && code == "ERR_ASSERTION"
                    && !message.starts_with("setup:")
                {
                    kind = FailKind::Assertion;
                }
                if message.starts_with("Timeout of") {
                    kind = FailKind::Timeout;
                }
                found.push(failed(str_of(test, "title"), kind, first_lines(&text, 6)));
            }
        }
        Format::Playwright => {
            for error in array_of(&json, "errors") {
                let message = str_of(error, "message");
                found.push(failed("file:run", FailKind::Error, first_lines(message, 8)));
            }
            for suite in array_of(&json, "suites") {
                playwright_suite(suite, &mut found);
            }
        }
        Format::Extra => {}
    }
    Ok(found)
}

/// Reads the further outcomes a check writes for other checks, a JSON array of
/// `{"title", "outcome": "pass" | "fail", "kind": "assertion" | "error" | null, "message"}`:
/// each case of flow__every_activity_shows gives its scenario check its e2e result this way.
pub fn parse_extra(text: &str) -> Result<Vec<TestOutcome>, String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("no JSON: {e}"))?;
    let entries = json.as_array().ok_or("not a JSON array")?;
    entries
        .iter()
        .map(|entry| {
            let title = str_of(entry, "title");
            let message = str_of(entry, "message").to_owned();
            match (str_of(entry, "outcome"), str_of(entry, "kind")) {
                ("pass", _) => Ok(simple(title, Outcome::Pass)),
                ("fail", "assertion") => Ok(failed(title, FailKind::Assertion, message)),
                ("fail", _) => Ok(failed(title, FailKind::Error, message)),
                (other, _) => Err(format!("{title}: unknown outcome {other:?}")),
            }
        })
        .collect()
}

/// A regular expression the layers' test runners match against full test titles, for `--only`.
pub fn title_pattern(check: &str) -> String {
    let escaped: String = check
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_string()
            } else {
                format!("\\{c}")
            }
        })
        .collect();
    format!("(^|\\s){escaped}(\\s|$)")
}

fn playwright_suite(suite: &serde_json::Value, found: &mut Vec<TestOutcome>) {
    for spec in array_of(suite, "specs") {
        let title = str_of(spec, "title");
        for test in array_of(spec, "tests") {
            let last = array_of(test, "results")
                .last()
                .unwrap_or(&serde_json::Value::Null);
            found.push(match str_of(test, "status") {
                "expected" => simple(title, Outcome::Pass),
                "flaky" => passed(title, 1),
                "skipped" => simple(title, Outcome::NotRun),
                _ => {
                    let message = last
                        .get("error")
                        .map(|error| {
                            let stack = str_of(error, "stack");
                            if stack.is_empty() {
                                str_of(error, "message").to_owned()
                            } else {
                                stack.to_owned()
                            }
                        })
                        .unwrap_or_default();
                    let kind = if str_of(last, "status") == "timedOut" {
                        FailKind::Timeout
                    } else {
                        classify(&message)
                    };
                    failed(title, kind, first_lines(&message, 6))
                }
            });
        }
    }
    for child in array_of(suite, "suites") {
        playwright_suite(child, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::plain("idle_pool", "idle_pool")]
    #[case::with_case(
        "rows__should_follow_the_grammar clean_foreground",
        "rows__should_follow_the_grammar"
    )]
    #[case::empty("", "")]
    fn check_of__should_take_the_first_word(#[case] title: &str, #[case] expected: &str) {
        // Given
        let text = title;

        // When
        let check = check_of(text);

        // Then
        assert_eq!(check, expected);
    }

    #[test]
    fn title_pattern__should_match_the_check_as_a_whole_word() {
        // Given
        let check = "budget__b1";

        // When
        let pattern = title_pattern(check);

        // Then
        assert_eq!(pattern, "(^|\\s)budget__b1(\\s|$)");
    }

    #[rstest]
    #[case::node_assert(
        "AssertionError [ERR_ASSERTION]: the rows differ\n at x",
        FailKind::Assertion
    )]
    #[case::node_assert_setup("AssertionError [ERR_ASSERTION]: setup: no pool", FailKind::Error)]
    #[case::thrown_setup("Error: setup: WTPOOL_VERIFY_RUN_DIR is not set", FailKind::Error)]
    #[case::expect("Error: expect(Buffer).toMatchSnapshot(expected)", FailKind::Assertion)]
    #[case::coloured(
        "\u{1b}[31mAssertionError [ERR_ASSERTION]\u{1b}[39m: x",
        FailKind::Assertion
    )]
    #[case::mocha_timeout("Error: Timeout of 180000ms exceeded.", FailKind::Timeout)]
    #[case::vitest_timeout("Error: Test timed out in 60000ms.", FailKind::Timeout)]
    #[case::type_error("TypeError: cannot read properties of undefined", FailKind::Error)]
    #[case::empty("", FailKind::Error)]
    fn classify__should_split_assertions_timeouts_and_errors(
        #[case] text: &str,
        #[case] expected: FailKind,
    ) {
        // Given
        let message = text;

        // When
        let kind = classify(message);

        // Then
        assert_eq!(kind, expected);
    }

    #[test]
    fn parse__should_read_vitest_json() {
        // Given
        let text = r#"{"testResults": [
            {"name": "/x/render/rows.test.mjs", "status": "failed", "message": "", "assertionResults": [
                {"title": "rows__should_follow_the_grammar idle", "status": "passed", "failureMessages": []},
                {"title": "ages__should_use_the_format 5_s", "status": "failed", "failureMessages": ["AssertionError [ERR_ASSERTION]: no\n at y"]},
                {"title": "order__should_not_change_when_states_change", "status": "skipped", "failureMessages": []}
            ]},
            {"name": "/x/render/broken.test.mjs", "status": "failed", "message": "SyntaxError: bad", "assertionResults": []}
        ]}"#;

        // When
        let tests = parse(Format::Vitest, text).unwrap();

        // Then
        let summary: Vec<(&str, Outcome, Option<FailKind>)> = tests
            .iter()
            .map(|t| (t.title.as_str(), t.outcome, t.kind))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("rows__should_follow_the_grammar idle", Outcome::Pass, None),
                (
                    "ages__should_use_the_format 5_s",
                    Outcome::Fail,
                    Some(FailKind::Assertion)
                ),
                (
                    "order__should_not_change_when_states_change",
                    Outcome::NotRun,
                    None
                ),
                ("file:broken.test.mjs", Outcome::Fail, Some(FailKind::Error)),
            ]
        );
    }

    #[test]
    fn parse__should_read_mocha_json() {
        // Given
        let text = r#"{"passes": [{"title": "partial_lines", "currentRetry": 0, "err": {}}, {"title": "garbage_line", "currentRetry": 2, "err": {}}],
            "pending": [],
            "failures": [
                {"title": "early_exit", "err": {"message": "restarts", "stack": "AssertionError [ERR_ASSERTION]: restarts\n at z", "code": "ERR_ASSERTION"}},
                {"title": "stall_30s", "err": {"message": "Timeout of 180000ms exceeded.", "stack": "Error: Timeout of 180000ms exceeded."}},
                {"title": "open__should_reveal_or_create_beside", "err": {"message": "setup: no extension", "stack": "AssertionError [ERR_ASSERTION]: setup: no extension", "code": "ERR_ASSERTION"}}
            ]}"#;

        // When
        let tests = parse(Format::Mocha, text).unwrap();

        // Then
        let summary: Vec<(&str, Outcome, Option<FailKind>, bool)> = tests
            .iter()
            .map(|t| (t.title.as_str(), t.outcome, t.kind, t.flaky))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("partial_lines", Outcome::Pass, None, false),
                ("garbage_line", Outcome::Pass, None, true),
                (
                    "early_exit",
                    Outcome::Fail,
                    Some(FailKind::Assertion),
                    false
                ),
                ("stall_30s", Outcome::Fail, Some(FailKind::Timeout), false),
                (
                    "open__should_reveal_or_create_beside",
                    Outcome::Fail,
                    Some(FailKind::Error),
                    false
                ),
            ]
        );
    }

    #[test]
    fn parse_extra__should_read_each_outcome() {
        // Given
        let text = r#"[
            {"title": "idle_pool", "outcome": "pass", "kind": null, "message": ""},
            {"title": "agents_off", "outcome": "fail", "kind": "assertion", "message": "rows"},
            {"title": "unknown_git", "outcome": "fail", "kind": "error", "message": "setup: x"}
        ]"#;

        // When
        let tests = parse_extra(text).unwrap();

        // Then
        let summary: Vec<(&str, Outcome, Option<FailKind>)> = tests
            .iter()
            .map(|t| (t.title.as_str(), t.outcome, t.kind))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("idle_pool", Outcome::Pass, None),
                ("agents_off", Outcome::Fail, Some(FailKind::Assertion)),
                ("unknown_git", Outcome::Fail, Some(FailKind::Error)),
            ]
        );
    }

    #[test]
    fn parse__should_read_nested_playwright_suites() {
        // Given
        let text = r#"{"errors": [], "suites": [{"title": "panel.spec.mjs", "specs": [
                {"title": "csp__should_report_no_violation", "tests": [{"status": "expected", "results": [{"status": "passed"}]}]}
            ], "suites": [{"title": "themes", "specs": [
                {"title": "shot__should_render_every_theme dark", "tests": [{"status": "unexpected", "results": [
                    {"status": "failed", "error": {"message": "AssertionError [ERR_ASSERTION]: colours"}}]}]},
                {"title": "shot__should_render_every_theme light", "tests": [{"status": "unexpected", "results": [
                    {"status": "timedOut", "error": {"message": "Test timeout of 180000ms exceeded."}}]}]},
                {"title": "legend__should_cover_every_activity_value dark", "tests": [{"status": "flaky", "results": [
                    {"status": "failed"}, {"status": "passed"}]}]}
            ]}]}]}"#;

        // When
        let tests = parse(Format::Playwright, text).unwrap();

        // Then
        let summary: Vec<(&str, Outcome, Option<FailKind>, bool)> = tests
            .iter()
            .map(|t| (t.title.as_str(), t.outcome, t.kind, t.flaky))
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "csp__should_report_no_violation",
                    Outcome::Pass,
                    None,
                    false
                ),
                (
                    "shot__should_render_every_theme dark",
                    Outcome::Fail,
                    Some(FailKind::Assertion),
                    false
                ),
                (
                    "shot__should_render_every_theme light",
                    Outcome::Fail,
                    Some(FailKind::Timeout),
                    false
                ),
                (
                    "legend__should_cover_every_activity_value dark",
                    Outcome::Pass,
                    None,
                    true
                ),
            ]
        );
    }
}
