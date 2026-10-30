//! Strings a record stores, the characters no stored or printed string shows, and the label of a
//! session without a title.

/// The most characters a stored string keeps.
pub const MAX_TEXT: usize = 80;
/// The most characters a label keeps.
pub const MAX_LABEL: usize = 40;

/// The format (Cf) characters of Unicode 17.0, the version of the Node in VS Code 1.143, as sorted
/// inclusive ranges. `xtask/tests/checks/cli_text.rs` compares them with the `\p{Cf}` of the Node
/// on `PATH`, and prints this table anew when they differ.
const FORMAT: [(char, char); 21] = [
    ('\u{ad}', '\u{ad}'),
    ('\u{600}', '\u{605}'),
    ('\u{61c}', '\u{61c}'),
    ('\u{6dd}', '\u{6dd}'),
    ('\u{70f}', '\u{70f}'),
    ('\u{890}', '\u{891}'),
    ('\u{8e2}', '\u{8e2}'),
    ('\u{180e}', '\u{180e}'),
    ('\u{200b}', '\u{200f}'),
    ('\u{202a}', '\u{202e}'),
    ('\u{2060}', '\u{2064}'),
    ('\u{2066}', '\u{206f}'),
    ('\u{feff}', '\u{feff}'),
    ('\u{fff9}', '\u{fffb}'),
    ('\u{110bd}', '\u{110bd}'),
    ('\u{110cd}', '\u{110cd}'),
    ('\u{13430}', '\u{1343f}'),
    ('\u{1bca0}', '\u{1bca3}'),
    ('\u{1d173}', '\u{1d17a}'),
    ('\u{e0001}', '\u{e0001}'),
    ('\u{e0020}', '\u{e007f}'),
];

fn is_format(c: char) -> bool {
    let next = FORMAT.partition_point(|&(_, high)| high < c);
    FORMAT.get(next).is_some_and(|&(low, _)| low <= c)
}

/// Control (Cc) and format (Cf) characters, the line and paragraph separators, and the whole tag
/// block, the code points it has not assigned yet included.
#[must_use]
pub fn is_invisible(c: char) -> bool {
    c.is_control()
        || is_format(c)
        || matches!(c, '\u{2028}' | '\u{2029}' | '\u{e0000}'..='\u{e007f}')
}

/// `text` without the characters [`is_invisible`] names, cut to `limit` characters.
#[must_use]
pub fn clean(text: &str, limit: usize) -> String {
    text.chars()
        .filter(|c| !is_invisible(*c))
        .take(limit)
        .collect()
}

/// `<noun> HH:MM`, the label of a session without a title, at the local time `local`.
#[must_use]
pub fn untitled_label(noun: &str, local: Option<jiff::civil::Time>) -> String {
    match local {
        Some(time) => format!("{noun} {:02}:{:02}", time.hour(), time.minute()),
        None => noun.to_owned(),
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
    #[case::soft_hyphen("in\u{ad}visible", 80, "invisible")]
    #[case::arabic_number_signs("\u{600}1\u{601}2\u{602}3\u{603}4\u{604}5\u{605}", 80, "12345")]
    #[case::interlinear_annotations("a\u{fff9}b\u{fffa}c\u{fffb}d", 80, "abcd")]
    #[case::tags_assigned_or_not("flag\u{e0000}\u{e0001}\u{e0020}\u{e0041}\u{e007f}", 80, "flag")]
    #[case::deprecated_formats("a\u{206a}b\u{206f}c", 80, "abc")]
    #[case::neighbours_kept(
        "\u{ac}\u{ae}\u{5ff}\u{606}\u{fff8}\u{fffc}\u{dffff}\u{e0080}",
        80,
        "\u{ac}\u{ae}\u{5ff}\u{606}\u{fff8}\u{fffc}\u{dffff}\u{e0080}"
    )]
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

    #[test]
    fn format__should_hold_sorted_disjoint_ranges() {
        // Then
        assert!(FORMAT.iter().all(|&(low, high)| low <= high));
        assert!(
            FORMAT
                .windows(2)
                .all(|pair| matches!(pair, [(_, high), (low, _)] if high < low))
        );
    }

    #[rstest]
    #[case::noon("session", jiff::civil::time(12, 0, 0, 0), "session 12:00")]
    #[case::evening("session", jiff::civil::time(17, 30, 59, 0), "session 17:30")]
    #[case::midnight("session", jiff::civil::time(0, 5, 0, 0), "session 00:05")]
    #[case::other_noun("agent", jiff::civil::time(14, 2, 0, 0), "agent 14:02")]
    fn untitled_label__should_name_the_noun_and_the_local_time(
        #[case] noun: &str,
        #[case] local: jiff::civil::Time,
        #[case] expected: &str,
    ) {
        // When
        let label = untitled_label(noun, Some(local));

        // Then
        assert_eq!(label, expected);
    }

    #[test]
    fn untitled_label__should_name_no_time_it_does_not_know() {
        // When
        let label = untitled_label("session", None);

        // Then
        assert_eq!(label, "session");
    }
}
