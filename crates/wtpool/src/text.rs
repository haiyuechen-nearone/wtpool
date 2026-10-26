//! Strings from git, records and settings as the terminal shows them. A branch name may carry
//! controls such as OSC 52 that a terminal runs, or bidi overrides that reorder the line.

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
fn is_invisible(c: char) -> bool {
    c.is_control()
        || is_format(c)
        || matches!(c, '\u{2028}' | '\u{2029}' | '\u{e0000}'..='\u{e007f}')
}

/// `text` with each control, format, line separator, paragraph separator and tag character as
/// `\u{XXXX}`, in upper case hex of at least four digits.
#[must_use]
pub fn visible(text: &str) -> String {
    let mut shown = String::with_capacity(text.len());
    for c in text.chars() {
        if is_invisible(c) {
            shown.push_str(&format!("\\u{{{:04X}}}", u32::from(c)));
        } else {
            shown.push(c);
        }
    }
    shown
}

/// `text` with each line as [`visible`] shows it, its line ends kept.
#[must_use]
pub fn visible_lines(text: &str) -> String {
    text.split('\n').map(visible).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    /// The characters just outside each range of the characters [`visible`] escapes.
    const NEIGHBOURS: &str = concat!(
        " ~\u{a0}\u{ac}\u{ae}\u{5ff}\u{606}\u{61b}\u{61d}\u{6dc}\u{6de}\u{70e}\u{710}\u{88f}",
        "\u{892}\u{8e1}\u{8e3}\u{180d}\u{180f}\u{200a}\u{2010}\u{2027}\u{202f}\u{205f}\u{2065}",
        "\u{2070}\u{fefe}\u{ff00}\u{fff8}\u{fffc}\u{110bc}\u{110be}\u{110cc}\u{110ce}\u{1342f}",
        "\u{13440}\u{1bc9f}\u{1bca4}\u{1d172}\u{1d17b}\u{dffff}\u{e0080}",
    );

    #[rstest]
    #[case::ascii("feature/x-1.2_y", "feature/x-1.2_y")]
    #[case::letters_of_other_scripts("ação-日本-שלום-مرحبا", "ação-日本-שלום-مرحبا")]
    #[case::neighbours_of_the_ranges(NEIGHBOURS, NEIGHBOURS)]
    #[case::c0_controls(
        "\u{0}\t\n\r\u{7}\u{1b}\u{1f}",
        "\\u{0000}\\u{0009}\\u{000A}\\u{000D}\\u{0007}\\u{001B}\\u{001F}"
    )]
    #[case::delete_and_c1_controls(
        "\u{7f}\u{80}\u{9b}\u{9c}\u{9d}\u{9f}",
        "\\u{007F}\\u{0080}\\u{009B}\\u{009C}\\u{009D}\\u{009F}"
    )]
    #[case::soft_hyphen_and_vowel_separator("\u{ad}\u{180e}", "\\u{00AD}\\u{180E}")]
    #[case::arabic_and_syriac_signs_and_letter_mark(
        "\u{600}\u{605}\u{61c}\u{6dd}\u{70f}\u{890}\u{891}\u{8e2}",
        "\\u{0600}\\u{0605}\\u{061C}\\u{06DD}\\u{070F}\\u{0890}\\u{0891}\\u{08E2}"
    )]
    #[case::zero_width_and_marks(
        "\u{200b}\u{200c}\u{200d}\u{200e}\u{200f}",
        "\\u{200B}\\u{200C}\\u{200D}\\u{200E}\\u{200F}"
    )]
    #[case::separators_embeddings_and_overrides(
        "\u{2028}\u{2029}\u{202a}\u{202e}",
        "\\u{2028}\\u{2029}\\u{202A}\\u{202E}"
    )]
    #[case::word_joiner_and_invisible_operators("\u{2060}\u{2064}", "\\u{2060}\\u{2064}")]
    #[case::isolates_and_deprecated_formats(
        "\u{2066}\u{2069}\u{206a}\u{206f}",
        "\\u{2066}\\u{2069}\\u{206A}\\u{206F}"
    )]
    #[case::byte_order_mark("a\u{feff}b", "a\\u{FEFF}b")]
    #[case::interlinear_annotations("\u{fff9}\u{fffb}", "\\u{FFF9}\\u{FFFB}")]
    #[case::formats_outside_the_basic_plane(
        "\u{110bd}\u{110cd}\u{13430}\u{1343f}\u{1bca0}\u{1bca3}\u{1d173}\u{1d17a}",
        "\\u{110BD}\\u{110CD}\\u{13430}\\u{1343F}\\u{1BCA0}\\u{1BCA3}\\u{1D173}\\u{1D17A}"
    )]
    #[case::tags_assigned_or_not(
        "\u{e0000}\u{e0001}\u{e0002}\u{e001f}\u{e0020}\u{e0041}\u{e007f}",
        "\\u{E0000}\\u{E0001}\\u{E0002}\\u{E001F}\\u{E0020}\\u{E0041}\\u{E007F}"
    )]
    fn visible__should_escape_the_invisible_characters_only(
        #[case] text: &str,
        #[case] expected: &str,
    ) {
        // When
        let shown = visible(text);

        // Then
        assert_eq!(shown, expected);
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
    #[case::empty("", "")]
    #[case::lines("a\u{1b}b\n\u{202e}c\n", "a\\u{001B}b\n\\u{202E}c\n")]
    #[case::last_line_without_end("a\nb\tc", "a\nb\\u{0009}c")]
    #[case::empty_lines("\n\n", "\n\n")]
    fn visible_lines__should_escape_each_line_and_keep_the_line_ends(
        #[case] text: &str,
        #[case] expected: &str,
    ) {
        // When
        let shown = visible_lines(text);

        // Then
        assert_eq!(shown, expected);
    }
}
