use rstest::rstest;
use serde_json::json;

use super::*;

/// The hash as a whole [`Value`] gives it, the text every stored hash was made from.
fn value_hash(value: &Value) -> String {
    let mut text = String::new();
    write_sorted(value, &mut text);
    sha256::hex(text.as_bytes())
}

fn write_sorted(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                if let Some(item) = map.get(key) {
                    write_sorted(item, out);
                }
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_sorted(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

fn must_input(text: &str) -> CallInput {
    serde_json::from_str(text).unwrap()
}

#[rstest]
#[case::command(r#"{"command":"git push","description":"Push"}"#)]
#[case::reordered_nested(r#"{"b":1,"a":{"y":[1,{"d":2,"c":3}],"x":"s"}}"#)]
#[case::empty_object("{}")]
#[case::empty_array("[]")]
#[case::empty_members(r#"{"a":{},"b":[],"c":""}"#)]
#[case::scalars(r#"[null,true,false,0,-0,1,-1]"#)]
#[case::floats(r#"[1.0,1e2,1E-7,0.1,-2.5e300,123.456e-3]"#)]
#[case::past_u64("[18446744073709551615,18446744073709551616,-9223372036854775809]")]
#[case::escapes(r#"{"a\"b":"é\n\t\\\/ 😀","\u0000":"\u001f"}"#)]
#[case::keys_by_bytes(r#"{"é":1,"z":2,"Z":3,"a":4,"":5}"#)]
#[case::duplicate_keys(r#"{"a":1,"b":2,"a":{"x":[3]}}"#)]
#[case::whitespace(" { \"a\" : [ 1 , 2 ] ,\n\"b\":null } ")]
#[case::string("\"text\"")]
#[case::number("42")]
#[case::null("null")]
fn hash__should_match_the_hash_of_the_whole_value(#[case] text: &str) {
    // Given
    let value: Value = serde_json::from_str(text).unwrap();

    // When
    let input = must_input(text);

    // Then
    assert_eq!(input.hash(), value_hash(&value));
}

#[rstest]
#[case::reordered_keys(
    json!({"b": 1, "a": {"y": [1, {"d": 2, "c": 3}], "x": "s"}}),
    json!({"a": {"x": "s", "y": [1, {"c": 3, "d": 2}]}, "b": 1}),
    true
)]
#[case::other_value(json!({"command": "git push"}), json!({"command": "git push -f"}), false)]
#[case::array_order(json!([1, 2]), json!([2, 1]), false)]
fn hash__should_ignore_key_order_only(
    #[case] first: Value,
    #[case] second: Value,
    #[case] same: bool,
) {
    // When
    let (a, b) = (
        must_input(&first.to_string()),
        must_input(&second.to_string()),
    );

    // Then
    assert_eq!(a.hash() == b.hash(), same);
    assert_eq!(a.hash().len(), 64);
}

#[test]
fn hash__should_hash_the_sorted_text() {
    // Given
    let text = r#"{"b":"x","a":null}"#;

    // When
    let input = must_input(text);

    // Then
    assert_eq!(input.hash(), sha256::hex(br#"{"a":null,"b":"x"}"#));
}

#[test]
fn null__should_hash_as_null() {
    // When
    let input = CallInput::null();

    // Then
    assert_eq!(input, must_input("null"));
    assert_eq!(input.string("command"), None);
}

#[rstest]
#[case::string(r#"{"command":"git status"}"#, Some("git status"))]
#[case::escaped(r#"{"command":"a\"bé"}"#, Some("a\"bé"))]
#[case::last_of_two(r#"{"command":"first","command":"second"}"#, Some("second"))]
#[case::string_then_number(r#"{"command":"first","command":7}"#, None)]
#[case::number(r#"{"command":7}"#, None)]
#[case::nested(r#"{"input":{"command":"git status"}}"#, None)]
#[case::in_an_array(r#"[{"command":"git status"}]"#, None)]
#[case::text(r#""command""#, None)]
fn string__should_read_only_top_level_string_fields(
    #[case] text: &str,
    #[case] expected: Option<&str>,
) {
    // When
    let input = must_input(text);

    // Then
    assert_eq!(input.string("command"), expected);
}

#[rstest]
#[case::true_value(r#"{"run_in_background":true}"#, true)]
#[case::false_value(r#"{"run_in_background":false}"#, false)]
#[case::a_string(r#"{"run_in_background":"true"}"#, false)]
#[case::last_of_two(r#"{"run_in_background":true,"run_in_background":1}"#, false)]
#[case::nested(r#"{"input":{"run_in_background":true}}"#, false)]
#[case::missing(r#"{"command":"x"}"#, false)]
fn is_true__should_read_only_a_top_level_true(#[case] text: &str, #[case] expected: bool) {
    // When
    let input = must_input(text);

    // Then
    assert_eq!(input.is_true("run_in_background"), expected);
}

#[rstest]
#[case::depth_126(126, true)]
#[case::depth_127(127, false)]
fn deserialize__should_refuse_the_depth_a_whole_value_refuses(
    #[case] depth: usize,
    #[case] parses: bool,
) {
    // Given
    let input = format!(
        r#"{{"tool_input":{}{}}}"#,
        "[".repeat(depth),
        "]".repeat(depth)
    );

    // When
    let streamed = serde_json::from_str::<BTreeMap<String, CallInput>>(&input).is_ok();
    let whole = serde_json::from_str::<BTreeMap<String, Value>>(&input).is_ok();

    // Then
    assert_eq!((streamed, whole), (parses, parses));
}

#[rstest]
#[case::lone_surrogate(r#"{"a":"\ud800"}"#)]
#[case::number_out_of_range("[1e400]")]
#[case::trailing_comma("[1,]")]
#[case::control_character("\"a\u{1}b\"")]
fn deserialize__should_refuse_what_a_whole_value_refuses(#[case] text: &str) {
    // When
    let streamed = serde_json::from_str::<CallInput>(text);

    // Then
    assert!(
        serde_json::from_str::<Value>(text).is_err(),
        "setup: {text}"
    );
    assert!(streamed.is_err(), "{text}");
}

#[test]
fn deserialize__should_say_what_it_expects() {
    // Given
    let bytes = serde::de::value::BytesDeserializer::<serde::de::value::Error>::new(b"x");

    // When
    let error = CallInput::deserialize(bytes).unwrap_err();

    // Then
    assert!(
        error.to_string().ends_with("expected any JSON value"),
        "{error}"
    );
}
