//! [`Suggestions`]: the heavy patterns a repository's stack markers suggest, from the names of
//! the top-level entries of its main worktree and the scripts of its `package.json`.

use std::path::Path;

use serde::Serialize;

use super::scripts::{self, PACKAGE_JSON};
use super::{HeavyList, Pattern, Suggestion};

/// Entries past this many are never listed, so a crowded folder stays quick.
const MAX_ENTRIES: usize = 10_000;

/// A name among the top-level entries, and the patterns it suggests.
struct Marker {
    name: Name,
    patterns: &'static [&'static str],
    /// A wrapper script whose presence beside the marker replaces [`Marker::patterns`] with its
    /// own.
    wrapper: Option<(&'static str, &'static [&'static str])>,
}

enum Name {
    Exact(&'static str),
    /// Any name with a stem before this ending, such as `App.csproj` for `.csproj`.
    Ending(&'static str),
}

const CARGO: &[&str] = &["cargo build", "cargo test", "cargo nextest", "cargo clippy"];
const GO: &[&str] = &["go build", "go test"];
const BAZEL: &[&str] = &["bazel build", "bazel test"];
const DOTNET: &[&str] = &["dotnet build", "dotnet test"];
const SWIFT: &[&str] = &["swift build", "swift test"];
const MAVEN_WRAPPER: Option<(&str, &[&str])> = Some(("mvnw", &["mvnw"]));
const GRADLE_WRAPPER: Option<(&str, &[&str])> = Some(("gradlew", &["gradlew"]));

/// `cargo check` stays out: editors run it on every save.
const MARKERS: &[Marker] = &[
    Marker::exact("Cargo.toml", CARGO),
    Marker::exact("go.mod", GO),
    Marker::exact("pom.xml", &["mvn"]).or_wrapper(MAVEN_WRAPPER),
    Marker::exact("build.gradle", &["gradle"]).or_wrapper(GRADLE_WRAPPER),
    Marker::exact("build.gradle.kts", &["gradle"]).or_wrapper(GRADLE_WRAPPER),
    Marker::exact("pyproject.toml", &["pytest"]),
    Marker::exact("pytest.ini", &["pytest"]),
    Marker::exact("tox.ini", &["tox", "pytest"]),
    Marker::exact("BUILD.bazel", BAZEL),
    Marker::exact("WORKSPACE", BAZEL),
    Marker::exact("MODULE.bazel", BAZEL),
    Marker::exact("Makefile", &["make"]),
    Marker::exact("makefile", &["make"]),
    Marker::exact("GNUmakefile", &["make"]),
    // `cmake --build` is no pattern, and its options are no command words.
    Marker::exact("CMakeLists.txt", &["cmake", "ninja"]),
    Marker::ending(".csproj", DOTNET),
    Marker::ending(".sln", DOTNET),
    Marker::exact("Package.swift", SWIFT),
    Marker::ending(".xcodeproj", &["xcodebuild"]),
];

/// The patterns suggested for a repository, which apply only once the user adds them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suggestions {
    /// In the order of the marker table, then `package.json`'s scripts by name; one per pattern.
    pub found: Vec<Suggestion>,
    /// Why `package.json` gave no suggestion, when it is there and cannot be read.
    pub fault: Option<String>,
}

impl Marker {
    const fn exact(name: &'static str, patterns: &'static [&'static str]) -> Self {
        Self {
            name: Name::Exact(name),
            patterns,
            wrapper: None,
        }
    }

    const fn ending(ending: &'static str, patterns: &'static [&'static str]) -> Self {
        Self {
            name: Name::Ending(ending),
            patterns,
            wrapper: None,
        }
    }

    const fn or_wrapper(self, wrapper: Option<(&'static str, &'static [&'static str])>) -> Self {
        Self { wrapper, ..self }
    }

    /// The first of the sorted `names` this marker names.
    fn find<'a>(&self, names: &'a [String]) -> Option<&'a str> {
        let named = |name: &&String| match self.name {
            Name::Exact(exact) => name.as_str() == exact,
            Name::Ending(ending) => name.len() > ending.len() && name.ends_with(ending),
        };
        names.iter().find(named).map(String::as_str)
    }

    fn patterns(&self, names: &[String]) -> &'static [&'static str] {
        match self.wrapper {
            Some((wrapper, patterns)) if has(names, wrapper) => patterns,
            _ => self.patterns,
        }
    }
}

impl Suggestions {
    /// The suggestions of the folder `folder`, read from one listing of its names and at most one
    /// read of its `package.json`. Nothing runs and nothing is written.
    #[must_use]
    pub fn read(folder: &Path) -> Self {
        let names = top_names(folder);
        let mut suggestions = Self::from_names(&names);
        if has(&names, PACKAGE_JSON) {
            let path = folder.join(PACKAGE_JSON);
            match scripts::read(&path).and_then(|text| scripts::suggestions(&text, &names)) {
                Ok(patterns) => suggestions.add_all(patterns, PACKAGE_JSON),
                Err(fault) => {
                    suggestions.fault = Some(format!(
                        "{}: {fault}, so its scripts suggest nothing",
                        path.display()
                    ));
                }
            }
        }
        suggestions
    }

    /// The suggestions of the marker table for the sorted top-level `names`.
    #[must_use]
    pub fn from_names(names: &[String]) -> Self {
        let mut suggestions = Self::default();
        for marker in MARKERS {
            if let Some(name) = marker.find(names) {
                let patterns = marker.patterns(names);
                suggestions.add_all(patterns.iter().filter_map(|p| Pattern::parse(p).ok()), name);
            }
        }
        suggestions
    }

    /// Those [`found`](Self::found) that `heavy` does not hold.
    #[must_use]
    pub fn lacking(&self, heavy: &HeavyList) -> Vec<Suggestion> {
        self.found
            .iter()
            .filter(|suggestion| !heavy.contains(&suggestion.pattern))
            .cloned()
            .collect()
    }

    fn add_all(&mut self, patterns: impl IntoIterator<Item = Pattern>, from: &str) {
        for pattern in patterns {
            if !self.found.iter().any(|known| known.pattern == pattern) {
                self.found.push(Suggestion {
                    pattern,
                    from: from.to_owned(),
                });
            }
        }
    }
}

#[derive(Serialize)]
struct SuggestedJson<'a> {
    suggested: Vec<SuggestionJson<'a>>,
}

#[derive(Serialize)]
struct SuggestionJson<'a> {
    pattern: &'a str,
    from: &'a str,
}

/// `suggestions` as one line of JSON, `{"suggested": [{"pattern": …, "from": …}, …]}`.
#[must_use]
pub fn suggested_json(suggestions: &[Suggestion]) -> String {
    let document = SuggestedJson {
        suggested: suggestions
            .iter()
            .map(|suggestion| SuggestionJson {
                pattern: suggestion.pattern.as_str(),
                from: &suggestion.from,
            })
            .collect(),
    };
    // A document of strings always serializes; `{}` keeps the line JSON.
    serde_json::to_string(&document).unwrap_or_else(|_| "{}".to_owned()) + "\n"
}

/// The names of the first [`MAX_ENTRIES`] entries of `folder`, sorted; none when it cannot be
/// listed. A name that is not Unicode is no marker.
fn top_names(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .take(MAX_ENTRIES)
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    names.sort_unstable();
    names
}

/// Whether the sorted `names` hold `name`.
fn has(names: &[String], name: &str) -> bool {
    names
        .binary_search_by(|probe| probe.as_str().cmp(name))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        let mut names: Vec<String> = list.iter().map(|name| (*name).to_owned()).collect();
        names.sort_unstable();
        names
    }

    fn found(suggestions: &Suggestions) -> Vec<(&str, &str)> {
        suggestions
            .found
            .iter()
            .map(|s| (s.pattern.as_str(), s.from.as_str()))
            .collect()
    }

    #[test]
    fn markers__should_hold_only_patterns() {
        // When
        let invalid: Vec<&str> = MARKERS
            .iter()
            .flat_map(|marker| {
                let wrapped = marker.wrapper.map_or(&[][..], |(_, patterns)| patterns);
                marker.patterns.iter().chain(wrapped)
            })
            .copied()
            .filter(|pattern| Pattern::parse(pattern).is_err() || *pattern == "cargo check")
            .collect();

        // Then
        assert!(invalid.is_empty(), "{invalid:?}");
    }

    #[rstest]
    #[case::wrapper_there(&["pom.xml", "mvnw"], vec![("mvnw", "pom.xml")])]
    #[case::wrapper_alone(&["mvnw"], vec![])]
    #[case::gradle_wrapper(&["build.gradle", "gradlew"], vec![("gradlew", "build.gradle")])]
    #[case::project_by_ending(&["Zed.csproj", "App.csproj"], vec![("dotnet build", "App.csproj"), ("dotnet test", "App.csproj")])]
    #[case::an_ending_alone(&[".csproj"], vec![])]
    #[case::one_pattern_per_marker_set(&["pytest.ini", "tox.ini"], vec![("pytest", "pytest.ini"), ("tox", "tox.ini")])]
    #[case::nothing(&["README.md", "Cargo.lock"], vec![])]
    fn suggestions_from_names__should_follow_the_marker_table(
        #[case] entries: &[&str],
        #[case] expected: Vec<(&str, &str)>,
    ) {
        // When
        let suggestions = Suggestions::from_names(&names(entries));

        // Then
        assert_eq!(found(&suggestions), expected);
    }

    #[test]
    fn suggestions_lacking__should_leave_out_listed_patterns() {
        // Given
        let suggestions = Suggestions::from_names(&names(&["Cargo.toml"]));
        let listed = [Pattern::parse("cargo test").unwrap()];
        let heavy = HeavyList::new(&listed, &[], &["cargo".to_owned()]);

        // When
        let lacking = suggestions.lacking(&heavy);

        // Then
        let patterns: Vec<&str> = lacking.iter().map(|s| s.pattern.as_str()).collect();
        assert_eq!(patterns, ["cargo build", "cargo nextest", "cargo clippy"]);
    }

    #[test]
    fn suggestions_read__should_read_the_markers_and_package_json_of_a_folder() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Makefile"), "").unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts": {"build": "make"}}"#,
        )
        .unwrap();

        // When
        let suggestions = Suggestions::read(dir.path());

        // Then
        assert_eq!(
            found(&suggestions),
            [
                ("make", "Makefile"),
                ("yarn run build", "package.json"),
                ("yarn build", "package.json")
            ]
        );
        assert_eq!(suggestions.fault, None);
    }

    #[test]
    fn suggestions_read__should_say_why_package_json_gave_nothing() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{").unwrap();

        // When
        let suggestions = Suggestions::read(dir.path());

        // Then
        assert!(suggestions.found.is_empty());
        let fault = suggestions.fault.unwrap_or_default();
        assert!(fault.contains("package.json"), "{fault}");
    }

    #[test]
    fn suggested_json__should_name_each_pattern_and_its_marker() {
        // Given
        let suggestions = Suggestions::from_names(&names(&["Makefile"]));

        // When
        let json = suggested_json(&suggestions.found);

        // Then
        assert_eq!(
            json,
            "{\"suggested\":[{\"pattern\":\"make\",\"from\":\"Makefile\"}]}\n"
        );
    }

    #[test]
    fn suggestions_read__should_suggest_nothing_for_a_missing_folder() {
        // Given
        let dir = tempfile::tempdir().unwrap();

        // When
        let suggestions = Suggestions::read(&dir.path().join("gone"));

        // Then
        assert_eq!(suggestions, Suggestions::default());
    }
}
