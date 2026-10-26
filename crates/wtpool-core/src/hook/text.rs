//! Strings a record stores (H15) and the input hash a wait matches calls by (H13).

use serde_json::Value;

/// Stored strings keep this many characters.
pub const MAX_TEXT: usize = 80;
/// Labels keep this many characters.
pub const MAX_LABEL: usize = 40;

/// `text` without control, bidi and zero width characters, cut to `limit` characters.
#[must_use]
pub fn clean(text: &str, limit: usize) -> String {
    text.chars()
        .filter(|c| !is_invisible(*c))
        .take(limit)
        .collect()
}

/// Control characters (C0, DEL, C1), bidi marks, embeddings, overrides and isolates, and zero
/// width characters.
fn is_invisible(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{061c}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'..='\u{2069}'
                | '\u{180e}'
                | '\u{feff}'
        )
}

/// The SHA 256 of `value` written as JSON with the keys of every object sorted, in lower case
/// hex, so the same input matches however its keys are ordered.
#[must_use]
pub fn input_hash(value: &Value) -> String {
    let mut text = String::new();
    canonical(value, &mut text);
    super::sha256::hex(text.as_bytes())
}

fn canonical(value: &Value, out: &mut String) {
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
                    canonical(item, out);
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
                canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::json;

    use super::*;

    #[rstest]
    #[case::plain("Fix the build", 80, "Fix the build")]
    #[case::controls("a\u{7}b\u{1b}c\nd\u{9c}e", 80, "abcde")]
    #[case::bidi("\u{202e}evil\u{2066}x\u{200f}\u{061c}", 80, "evilx")]
    #[case::zero_width("a\u{200b}b\u{feff}c\u{2060}d", 80, "abcd")]
    #[case::cut_after_cleaning("\u{7}abcdef", 3, "abc")]
    #[case::cut_by_characters("ééééé", 2, "éé")]
    fn clean__should_strip_invisible_characters_and_cut(
        #[case] text: &str,
        #[case] limit: usize,
        #[case] expected: &str,
    ) {
        // When
        let cleaned = clean(text, limit);

        // Then
        assert_eq!(cleaned, expected);
    }

    #[rstest]
    #[case::reordered_keys(
        json!({"b": 1, "a": {"y": [1, {"d": 2, "c": 3}], "x": "s"}}),
        json!({"a": {"x": "s", "y": [1, {"c": 3, "d": 2}]}, "b": 1}),
        true
    )]
    #[case::other_value(json!({"command": "git push"}), json!({"command": "git push -f"}), false)]
    #[case::array_order(json!([1, 2]), json!([2, 1]), false)]
    fn input_hash__should_ignore_key_order_only(
        #[case] first: Value,
        #[case] second: Value,
        #[case] same: bool,
    ) {
        // When
        let (a, b) = (input_hash(&first), input_hash(&second));

        // Then
        assert_eq!(a == b, same);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn input_hash__should_hash_the_sorted_text() {
        // Given
        let value = json!({"b": "x", "a": null});

        // When
        let hash = input_hash(&value);

        // Then
        assert_eq!(hash, super::super::sha256::hex(br#"{"a":null,"b":"x"}"#));
    }
}
