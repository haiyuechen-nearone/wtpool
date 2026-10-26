//! The text of `wtpool --version`: the version, the commit and the cargo features of this build.

/// This binary's build, as build.rs records it.
pub(crate) const BUILD: Build<'static> = Build {
    version: env!("CARGO_PKG_VERSION"),
    commit: env!("WTPOOL_BUILD_COMMIT"),
    dirty: matches!(env!("WTPOOL_BUILD_DIRTY").as_bytes(), b"true"),
    features: env!("WTPOOL_BUILD_FEATURES"),
};

#[derive(Debug)]
pub(crate) struct Build<'a> {
    version: &'a str,
    /// Empty when git could not tell.
    commit: &'a str,
    dirty: bool,
    /// Comma separated, empty for none.
    features: &'a str,
}

pub(crate) fn version_text(build: &Build<'_>) -> String {
    let commit = if build.commit.is_empty() {
        "unknown"
    } else {
        build.commit
    };
    let dirty = if build.dirty { " (dirty)" } else { "" };
    let features = if build.features.is_empty() {
        "none"
    } else {
        build.features
    };
    format!(
        "wtpool {}\ncommit {commit}{dirty}\nfeatures: {features}\n",
        build.version
    )
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    #[rstest]
    #[case::known_commit_clean_no_features(
        COMMIT,
        false,
        "",
        "wtpool 0.1.0\ncommit 0123456789abcdef0123456789abcdef01234567\nfeatures: none\n"
    )]
    #[case::known_commit_clean_features(
        COMMIT,
        false,
        "a, b",
        "wtpool 0.1.0\ncommit 0123456789abcdef0123456789abcdef01234567\nfeatures: a, b\n"
    )]
    #[case::known_commit_dirty_no_features(
        COMMIT,
        true,
        "",
        "wtpool 0.1.0\ncommit 0123456789abcdef0123456789abcdef01234567 (dirty)\nfeatures: none\n"
    )]
    #[case::known_commit_dirty_features(
        COMMIT,
        true,
        "a, b",
        "wtpool 0.1.0\ncommit 0123456789abcdef0123456789abcdef01234567 (dirty)\nfeatures: a, b\n"
    )]
    #[case::unknown_commit_clean_no_features(
        "",
        false,
        "",
        "wtpool 0.1.0\ncommit unknown\nfeatures: none\n"
    )]
    #[case::unknown_commit_clean_features(
        "",
        false,
        "a, b",
        "wtpool 0.1.0\ncommit unknown\nfeatures: a, b\n"
    )]
    #[case::unknown_commit_dirty_no_features(
        "",
        true,
        "",
        "wtpool 0.1.0\ncommit unknown (dirty)\nfeatures: none\n"
    )]
    #[case::unknown_commit_dirty_features(
        "",
        true,
        "a, b",
        "wtpool 0.1.0\ncommit unknown (dirty)\nfeatures: a, b\n"
    )]
    fn version_text__should_list_version_commit_state_and_features(
        #[case] commit: &str,
        #[case] dirty: bool,
        #[case] features: &str,
        #[case] expected: &str,
    ) {
        // Given
        let build = Build {
            version: "0.1.0",
            commit,
            dirty,
            features,
        };

        // When
        let text = version_text(&build);

        // Then
        assert_eq!(text, expected);
    }
}
