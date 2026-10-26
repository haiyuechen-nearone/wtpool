use std::fmt;

/// A pull request number, kept as the digits the user typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrNumber(String);

impl PrNumber {
    /// `None` unless `digits` is one or more ASCII digits.
    pub(crate) fn new(digits: &str) -> Option<Self> {
        (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
            .then(|| Self(digits.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PrNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A GitHub repository, `<owner>/<name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepoSlug {
    owner: String,
    name: String,
}

impl RepoSlug {
    /// `None` when either part is empty or contains a `/`.
    pub(crate) fn new(owner: &str, name: &str) -> Option<Self> {
        let valid = |part: &str| !part.is_empty() && !part.contains('/');
        (valid(owner) && valid(name)).then(|| Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }

    /// Compares with another slug, ignoring case as GitHub does.
    pub(crate) fn matches(&self, other: &RepoSlug) -> bool {
        self.owner.eq_ignore_ascii_case(&other.owner) && self.name.eq_ignore_ascii_case(&other.name)
    }
}

impl fmt::Display for RepoSlug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::digits("1234", Some("1234"))]
    #[case::leading_zeros("0042", Some("0042"))]
    #[case::empty("", None)]
    #[case::letter("12a", None)]
    #[case::sign("-1", None)]
    #[case::hash("#12", None)]
    fn pr_number__should_accept_only_digits(#[case] digits: &str, #[case] expected: Option<&str>) {
        // When
        let number = PrNumber::new(digits);

        // Then
        assert_eq!(number.as_ref().map(PrNumber::as_str), expected);
        assert_eq!(number.map(|n| n.to_string()).as_deref(), expected);
    }

    #[rstest]
    #[case::same("acme", "widget", true)]
    #[case::other_case("Acme", "WIDGET", true)]
    #[case::other_repository("acme", "widget-private", false)]
    #[case::other_owner("acmecorp", "widget", false)]
    fn repo_slug__should_match_ignoring_case(
        #[case] owner: &str,
        #[case] name: &str,
        #[case] expected: bool,
    ) {
        // Given
        let slug = RepoSlug::new("acme", "widget").unwrap();

        // When
        let matches = slug.matches(&RepoSlug::new(owner, name).unwrap());

        // Then
        assert_eq!(matches, expected);
    }

    #[rstest]
    #[case::empty_owner("", "widget")]
    #[case::empty_name("acme", "")]
    #[case::slash_in_owner("a/b", "widget")]
    #[case::slash_in_name("acme", "a/b")]
    fn repo_slug__should_refuse_an_empty_part_or_a_slash(#[case] owner: &str, #[case] name: &str) {
        // When
        let slug = RepoSlug::new(owner, name);

        // Then
        assert_eq!(slug, None);
    }
}
