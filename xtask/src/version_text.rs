//! Reading `wtpool --version` output.

use std::collections::{BTreeMap, BTreeSet};

/// The features a build with `requested` and the default features enables, `default` itself
/// left out.
pub fn enabled_features(
    declared: &BTreeMap<String, Vec<String>>,
    requested: &[&str],
) -> BTreeSet<String> {
    let mut enabled: BTreeSet<String> = requested.iter().map(|f| (*f).to_owned()).collect();
    if declared.contains_key("default") {
        enabled.insert("default".to_owned());
    }
    let mut pending: Vec<String> = enabled.iter().cloned().collect();
    while let Some(feature) = pending.pop() {
        for value in declared.get(&feature).into_iter().flatten() {
            let plain = !value.contains('/') && !value.starts_with("dep:");
            if plain && declared.contains_key(value) && enabled.insert(value.clone()) {
                pending.push(value.clone());
            }
        }
    }
    enabled.remove("default");
    enabled
}

/// The feature list of a `--version` text: the rest of the first line that says `features`,
/// `none` or `-` for an empty list, `default` left out. `None` when no line lists features.
pub fn listed_features(text: &str) -> Option<BTreeSet<String>> {
    let line = text
        .lines()
        .find(|line| line.to_lowercase().contains("features"))?;
    let at = line.to_lowercase().find("features")?;
    let rest = line.get(at + "features".len()..)?;
    let rest = rest.trim_start_matches([':', '=', ' ', '\t']);
    let names: BTreeSet<String> = rest
        .split([',', ' ', '\t'])
        .map(|word| word.trim_matches(|c: char| "[](){}\"'".contains(c)))
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect();
    let empty_markers: BTreeSet<String> = ["none", "-"].iter().map(|s| (*s).to_owned()).collect();
    if names.is_subset(&empty_markers) {
        return Some(BTreeSet::new());
    }
    Some(names.into_iter().filter(|name| name != "default").collect())
}

/// Whether `text` holds `word` as a whole token, also as `v<word>`.
pub fn has_token(text: &str, word: &str) -> bool {
    text.split(|c: char| c.is_whitespace() || "(),;:[]".contains(c))
        .map(|token| token.strip_prefix('v').unwrap_or(token))
        .any(|token| token == word)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[rstest]
    #[case::clap_style("wtpool 0.1.0\ncommit 0123abcd\nfeatures: verify\n", Some(&["verify"][..]))]
    #[case::comma_list("wtpool 0.1.0 (abc) features [default, verify, json]", Some(&["verify", "json"][..]))]
    #[case::none_word("wtpool 0.1.0\nfeatures: none", Some(&[][..]))]
    #[case::dash("wtpool 0.1.0\nFeatures - ", Some(&[][..]))]
    #[case::empty_after_colon("wtpool 0.1.0\nfeatures:", Some(&[][..]))]
    #[case::no_list("wtpool 0.1.0", None)]
    fn listed_features__should_read_the_feature_line(
        #[case] text: &str,
        #[case] expected: Option<&[&str]>,
    ) {
        // Given
        let version = text;

        // When
        let listed = listed_features(version);

        // Then
        assert_eq!(listed, expected.map(set));
    }

    #[rstest]
    #[case::no_default(&[("verify", &[][..])], &["verify"], &["verify"])]
    #[case::default_expands(&[("default", &["json"][..]), ("json", &[][..]), ("verify", &[][..])], &[], &["json"])]
    #[case::transitive(&[("verify", &["clock"][..]), ("clock", &["dep:x", "x/y"][..])], &["verify"], &["clock", "verify"])]
    fn enabled_features__should_expand_defaults_and_requests(
        #[case] declared: &[(&str, &[&str])],
        #[case] requested: &[&str],
        #[case] expected: &[&str],
    ) {
        // Given
        let declared: BTreeMap<String, Vec<String>> = declared
            .iter()
            .map(|(name, values)| {
                (
                    (*name).to_owned(),
                    values.iter().map(|v| (*v).to_owned()).collect(),
                )
            })
            .collect();

        // When
        let enabled = enabled_features(&declared, requested);

        // Then
        assert_eq!(enabled, set(expected));
    }

    #[rstest]
    #[case::plain("wtpool 0.1.0", "0.1.0", true)]
    #[case::prefixed("wtpool v0.1.0 (abc)", "0.1.0", true)]
    #[case::longer_version("wtpool 0.1.0-rc1", "0.1.0", false)]
    #[case::absent("wtpool", "0.1.0", false)]
    fn has_token__should_match_whole_tokens(
        #[case] text: &str,
        #[case] word: &str,
        #[case] expected: bool,
    ) {
        // Given
        let haystack = text;

        // When
        let found = has_token(haystack, word);

        // Then
        assert_eq!(found, expected);
    }
}
