//! OpenCode's session titles. OpenCode sends `null` for a session nobody named, and its 1.x line
//! named one `New session - <ISO time>`: neither is a title.

use wtpool_core::hook::text;

const PLACEHOLDER: &str = "New session - ";

/// `raw` as a title, `None` when it is missing, blank or the 1.x placeholder.
#[must_use]
pub fn title(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let blank = raw
        .chars()
        .all(|c| c.is_whitespace() || text::is_invisible(c));
    (!blank && !is_placeholder(raw)).then(|| raw.to_owned())
}

/// `true` for `New session - ` and a time as `Date.toISOString` writes it.
fn is_placeholder(title: &str) -> bool {
    title.strip_prefix(PLACEHOLDER).is_some_and(|time| {
        time.len() >= "2025-01-01T00:00:00Z".len()
            && time.starts_with(|c: char| c.is_ascii_digit())
            && time.contains('T')
            && time
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '-' | ':' | 'T' | '.' | 'Z' | '+'))
    })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::given(Some("Read slot two notes"), Some("Read slot two notes"))]
    #[case::null(None, None)]
    #[case::empty(Some(""), None)]
    #[case::blank(Some("  \t "), None)]
    #[case::invisible(Some("\u{200b}\u{202e}"), None)]
    #[case::placeholder(Some("New session - 2025-06-01T12:34:56.789Z"), None)]
    #[case::placeholder_without_millis(Some("New session - 2025-06-01T12:34:56Z"), None)]
    #[case::like_the_placeholder(Some("New session - planning"), Some("New session - planning"))]
    #[case::placeholder_alone(Some("New session - "), Some("New session - "))]
    fn title__should_drop_missing_blank_and_placeholder_titles(
        #[case] raw: Option<&str>,
        #[case] expected: Option<&str>,
    ) {
        // When
        let found = title(raw);

        // Then
        assert_eq!(found.as_deref(), expected);
    }
}
