//! Strings from git, records and settings as the terminal shows them. A branch name may carry C1
//! controls such as OSC 52 that a terminal runs, so every line wtpool prints from such a string
//! follows X10's rule for the webview.

use std::fmt::Write as _;

/// Control, bidi, zero width and other invisible format characters, as inclusive ranges: the set
/// of the webview's `visible()` (X10). C0 and C1 controls, the soft hyphen, the Arabic letter mark,
/// the Mongolian vowel separator, zero width spaces and joiners with the LRM and RLM, the bidi
/// embeddings and overrides, the word joiner and invisible operators, the bidi isolates, the BOM,
/// and the tags.
const INVISIBLE: [(char, char); 11] = [
    ('\u{0}', '\u{1f}'),
    ('\u{7f}', '\u{9f}'),
    ('\u{ad}', '\u{ad}'),
    ('\u{61c}', '\u{61c}'),
    ('\u{180e}', '\u{180e}'),
    ('\u{200b}', '\u{200f}'),
    ('\u{202a}', '\u{202e}'),
    ('\u{2060}', '\u{2064}'),
    ('\u{2066}', '\u{2069}'),
    ('\u{feff}', '\u{feff}'),
    ('\u{e0000}', '\u{e007f}'),
];

/// `text` with each control, bidi and zero width character as `\u{XXXX}`, in upper case hex of at
/// least four digits (X10).
#[must_use]
pub fn visible(text: &str) -> String {
    let mut shown = String::with_capacity(text.len());
    for c in text.chars() {
        if INVISIBLE
            .iter()
            .any(|&(low, high)| (low..=high).contains(&c))
        {
            // Writing to a `String` cannot fail.
            let _ = write!(shown, "\\u{{{:04X}}}", u32::from(c));
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

    /// The characters just outside each range of [`INVISIBLE`].
    const NEIGHBOURS: &str = concat!(
        " ~\u{a0}\u{ac}\u{ae}\u{61b}\u{61d}\u{180d}\u{180f}\u{200a}\u{2010}\u{2029}",
        "\u{202f}\u{205f}\u{2065}\u{206a}\u{fefe}\u{ff00}\u{dffff}\u{e0080}",
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
    #[case::soft_hyphen_letter_mark_and_vowel_separator(
        "\u{ad}\u{61c}\u{180e}",
        "\\u{00AD}\\u{061C}\\u{180E}"
    )]
    #[case::zero_width_and_marks(
        "\u{200b}\u{200c}\u{200d}\u{200e}\u{200f}",
        "\\u{200B}\\u{200C}\\u{200D}\\u{200E}\\u{200F}"
    )]
    #[case::embeddings_and_overrides("\u{202a}\u{202e}", "\\u{202A}\\u{202E}")]
    #[case::word_joiner_and_invisible_operators("\u{2060}\u{2064}", "\\u{2060}\\u{2064}")]
    #[case::isolates("\u{2066}\u{2069}", "\\u{2066}\\u{2069}")]
    #[case::byte_order_mark("a\u{feff}b", "a\\u{FEFF}b")]
    #[case::tags("\u{e0000}\u{e0041}\u{e007f}", "\\u{E0000}\\u{E0041}\\u{E007F}")]
    fn visible__should_escape_controls_bidi_and_zero_width_only(
        #[case] text: &str,
        #[case] expected: &str,
    ) {
        // When
        let shown = visible(text);

        // Then
        assert_eq!(shown, expected);
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
