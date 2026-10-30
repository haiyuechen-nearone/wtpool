use rstest::rstest;

use super::*;

/// An answer as Claude Code writes it: the model first in `message`, the entry's `type` last.
fn answer(model: &str, content: &str) -> String {
    format!(
        r#"{{"parentUuid":"p","message":{{"model":"{model}","id":"msg_1","type":"message","role":"assistant","content":[{{"type":"text","text":"{content}"}}]}},"type":"assistant","uuid":"u"}}"#
    )
}

const USER: &str = r#"{"type":"user","message":{"role":"user","content":"{\"model\":\"no\"}"}}"#;
const TITLE: &str = r#"{"type":"ai-title","aiTitle":"Fix the build","sessionId":"s1"}"#;

fn lines(lines: &[String]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn own(line: &str) -> String {
    line.to_owned()
}

#[rstest]
#[case::one_answer(vec![answer("claude-opus-5-5", "hi")], Some("claude-opus-5-5"))]
#[case::newest_answer(
    vec![answer("claude-sonnet-4-5", "a"), own(USER), answer("claude-opus-5-5", "b"), own(TITLE)],
    Some("claude-opus-5-5")
)]
#[case::synthetic_skipped(
    vec![answer("claude-opus-5-5", "a"), answer("<synthetic>", "b")],
    Some("claude-opus-5-5")
)]
#[case::only_synthetic(vec![answer("<synthetic>", "b")], None)]
#[case::no_answer(vec![own(USER), own(TITLE)], None)]
#[case::type_first(
    vec![own(r#"{"type":"assistant","message":{"role":"assistant","model":"m-1"}}"#)],
    Some("m-1")
)]
#[case::other_type(
    vec![own(r#"{"type":"user","message":{"role":"assistant","model":"m-1"}}"#)],
    None
)]
#[case::model_outside_message(
    vec![own(r#"{"type":"assistant","model":"m-1","message":{}}"#)],
    None
)]
#[case::model_not_a_string(vec![own(r#"{"type":"assistant","message":{"model":7}}"#)], None)]
#[case::not_json(vec![own(r#"{"model":"m-1""#)], None)]
#[case::a_line_that_is_a_list(vec![own(r#"["model"]"#)], None)]
#[case::a_message_that_is_a_list(vec![own(r#"{"type":"assistant","message":["model"]}"#)], None)]
fn newest_model__should_take_the_newest_answer_model(
    #[case] given: Vec<String>,
    #[case] expected: Option<&str>,
) {
    // When
    let model = newest_model(lines(&given).as_bytes());

    // Then
    assert_eq!(model.as_deref(), expected);
}

#[test]
fn newest_model__should_read_a_model_before_a_long_content() {
    // Given
    let long = answer("claude-opus-5-5", &"a".repeat(3 * HEAD));

    // When
    let model = newest_model(lines(&[long]).as_bytes());

    // Then
    assert_eq!(model.as_deref(), Some("claude-opus-5-5"));
}

#[test]
fn newest_model__should_not_read_a_model_past_the_head_of_its_line() {
    // Given
    let padding = "a".repeat(HEAD);
    let late = format!(r#"{{"type":"assistant","x":"{padding}","message":{{"model":"late"}}}}"#);
    let older = answer("claude-sonnet-4-5", "a");

    // When
    let model = newest_model(lines(&[older, late]).as_bytes());

    // Then
    assert_eq!(model.as_deref(), Some("claude-sonnet-4-5"));
}
