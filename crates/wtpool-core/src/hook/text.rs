//! Strings a record stores (H15), and the label of a session without a title (H16).

use crate::clock::Timestamp;
use crate::platform::zone;

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

/// `session HH:MM`, the label of a session without a title, at `at` in the local time zone, `TZ`
/// being `tz` (H16).
#[must_use]
pub fn session_label(at: Timestamp, tz: Option<&str>) -> String {
    match zone::local_time(at, tz) {
        Some(time) => format!("session {:02}:{:02}", time.hour(), time.minute()),
        None => "session".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

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
    #[case::utc(1_792_497_600, Some("UTC"), "session 12:00")]
    #[case::kolkata(1_792_497_600, Some("Asia/Kolkata"), "session 17:30")]
    #[case::past_the_last_date(i64::MAX, Some("UTC"), "session")]
    fn session_label__should_use_the_local_time(
        #[case] seconds: i64,
        #[case] tz: Option<&str>,
        #[case] expected: &str,
    ) {
        // When
        let label = session_label(Timestamp::from_seconds(seconds), tz);

        // Then
        assert_eq!(label, expected);
    }
}
