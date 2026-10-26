//! [`TomlError`]: the parser's error for a TOML text, flattened to the line it names and one
//! line of text.

/// A TOML parse error without the input the parser quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TomlError {
    /// Counting from 1, `None` when the parser names no place.
    pub line: Option<usize>,
    /// The parser's message on one line, the key it concerns included.
    pub reason: String,
}

impl TomlError {
    /// The `error` the parser gave for `text`.
    pub(crate) fn new(error: toml::de::Error, text: &str) -> Self {
        let line = error.span().map(|span| line_of(text, span.start));
        let mut detail = error;
        detail.set_input(None);
        Self {
            line,
            reason: detail.to_string().trim_end().replace('\n', " "),
        }
    }
}

/// The line that byte `offset` of `text` is on, counting from 1.
pub(crate) fn line_of(text: &str, offset: usize) -> usize {
    text.bytes()
        .take(offset)
        .filter(|&byte| byte == b'\n')
        .count()
        .saturating_add(1)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::first_line("a\nb\n", 0, 1)]
    #[case::later_line("a\nb\n", 2, 2)]
    #[case::inside_a_character("é\n", 1, 1)]
    fn line_of__should_count_the_lines_before_the_offset(
        #[case] text: &str,
        #[case] offset: usize,
        #[case] expected: usize,
    ) {
        // When
        let line = line_of(text, offset);

        // Then
        assert_eq!(line, expected);
    }

    #[test]
    fn toml_error_new__should_name_the_line_and_keep_one_line_of_text() {
        // Given
        let text = "a = 1\nb = \n";
        let error = toml::from_str::<toml::Table>(text).unwrap_err();

        // When
        let flat = TomlError::new(error, text);

        // Then
        assert_eq!(
            flat,
            TomlError {
                line: Some(2),
                reason: "string values must be quoted, expected literal string".to_owned(),
            }
        );
    }
}
