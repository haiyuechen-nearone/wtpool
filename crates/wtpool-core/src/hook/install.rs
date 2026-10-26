//! The text of `wtpool hook install`, the same shape for every harness's adapter (H1, H22).

use std::path::{Component, Path};

/// What `wtpool hook install` prints, one part per stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Printed {
    /// The hook block alone, valid JSON, so that `wtpool hook install > hooks.json` is a file the
    /// harness can load.
    pub stdout: String,
    /// Whether the harness runs the hooks, and else what is missing (H22).
    pub stderr: String,
}

/// Whether `binary` lies under a folder called `target`, a build that `cargo` may replace, so
/// no hook may run it (H1).
#[must_use]
pub fn is_under_target(binary: &Path) -> bool {
    binary
        .components()
        .any(|part| part == Component::Normal("target".as_ref()))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::release_build("/w/wtpool/target/release/wtpool", true)]
    #[case::nested_target("/x/target/debug/deps/wtpool", true)]
    #[case::installed("/Users/h/.local/bin/wtpool", false)]
    #[case::target_in_a_name("/opt/targets/wtpool", false)]
    fn is_under_target__should_find_a_target_component(#[case] path: &str, #[case] under: bool) {
        // When
        let found = is_under_target(Path::new(path));

        // Then
        assert_eq!(found, under);
    }
}
