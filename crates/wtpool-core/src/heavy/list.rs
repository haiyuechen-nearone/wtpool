//! [`HeavyList`]: the heavy patterns of the machine and of a repository, each with its source.

use std::fmt;

use serde::Serialize;

use super::{CommandWords, Pattern, RepositoryPatterns};
use crate::jobs::MachineConfig;

/// The list a heavy pattern comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternSource {
    /// `heavy` in the machine config.
    Machine,
    /// `heavy` in the repository's `wtpool.toml`.
    Repository,
    /// A name of `build_processes` in the repository's `wtpool.toml`, read as a one-word pattern.
    BuildProcesses,
}

/// A heavy pattern and the list it comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeavyPattern {
    pub pattern: Pattern,
    pub source: PatternSource,
}

/// The heavy patterns in effect, the machine's first. Nothing is heavy that no list names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeavyList(Vec<HeavyPattern>);

impl PatternSource {
    pub const ALL: [Self; 3] = [Self::Machine, Self::Repository, Self::BuildProcesses];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Machine => "machine",
            Self::Repository => "repository",
            Self::BuildProcesses => "build_processes",
        }
    }
}

impl fmt::Display for PatternSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl HeavyList {
    /// The machine's patterns, then the repository's, then each name of `build_processes` that
    /// is a one-word pattern.
    #[must_use]
    pub fn new(machine: &[Pattern], repository: &[Pattern], build_processes: &[String]) -> Self {
        let tagged = |patterns: &[Pattern], source| {
            patterns
                .iter()
                .cloned()
                .map(move |pattern| HeavyPattern { pattern, source })
                .collect::<Vec<_>>()
        };
        let names: Vec<Pattern> = build_processes
            .iter()
            .filter_map(|name| Pattern::parse(name).ok())
            .filter(|pattern| pattern.word_count() == 1)
            .collect();
        let mut list = tagged(machine, PatternSource::Machine);
        list.extend(tagged(repository, PatternSource::Repository));
        list.extend(tagged(&names, PatternSource::BuildProcesses));
        Self(list)
    }

    /// The list of `machine` and `repository`.
    #[must_use]
    pub fn of(machine: &MachineConfig, repository: &RepositoryPatterns) -> Self {
        Self::new(
            &machine.heavy,
            &repository.heavy,
            &repository.build_processes,
        )
    }

    #[must_use]
    pub fn patterns(&self) -> &[HeavyPattern] {
        &self.0
    }

    #[must_use]
    pub fn contains(&self, pattern: &Pattern) -> bool {
        self.0.iter().any(|listed| listed.pattern == *pattern)
    }

    /// The name of `command` by the matching pattern of the most words, the first of equals,
    /// and `None` when none matches.
    #[must_use]
    pub fn name_of(&self, command: &CommandWords) -> Option<String> {
        self.0
            .iter()
            .filter_map(|listed| {
                let name = listed.pattern.matched(command)?;
                Some((listed.pattern.word_count(), name))
            })
            .rev()
            .max_by_key(|(words, _)| *words)
            .map(|(_, name)| name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(texts: &[&str]) -> Vec<Pattern> {
        texts.iter().map(|t| Pattern::parse(t).unwrap()).collect()
    }

    fn command(args: &[&str]) -> CommandWords {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        CommandWords::of_command(&args)
    }

    #[test]
    fn heavy_list_new__should_tag_each_source_and_read_build_processes_as_one_word() {
        // Given
        let build_processes = [
            "protoc".to_owned(),
            "two words".to_owned(),
            "a/b".to_owned(),
        ];

        // When
        let list = HeavyList::new(
            &patterns(&["cargo test"]),
            &patterns(&["make"]),
            &build_processes,
        );

        // Then
        let found: Vec<(&str, &str)> = list
            .patterns()
            .iter()
            .map(|listed| (listed.pattern.as_str(), listed.source.as_str()))
            .collect();
        assert_eq!(
            found,
            vec![
                ("cargo test", "machine"),
                ("make", "repository"),
                ("protoc", "build_processes")
            ]
        );
        assert!(list.contains(&Pattern::parse("make").unwrap()));
        assert!(!list.contains(&Pattern::parse("cargo").unwrap()));
    }

    #[test]
    fn heavy_list_name_of__should_prefer_the_pattern_of_the_most_words() {
        // Given
        let list = HeavyList::new(&patterns(&["cargo", "cargo test", "cargo-*"]), &[], &[]);

        // When
        let names = [
            list.name_of(&command(&["cargo", "test", "--locked"])),
            list.name_of(&command(&["cargo", "build"])),
            list.name_of(&command(&["cargo-nextest", "run"])),
            list.name_of(&command(&["make"])),
        ];

        // Then
        assert_eq!(
            names,
            [
                Some("cargo test".to_owned()),
                Some("cargo".to_owned()),
                Some("cargo-nextest".to_owned()),
                None
            ]
        );
    }

    #[test]
    fn heavy_list_name_of__should_take_the_first_of_equal_patterns() {
        // Given
        let list = HeavyList::new(&patterns(&["cargo-*", "cargo-nextest"]), &[], &[]);

        // When
        let name = list.name_of(&command(&["cargo-nextest"]));

        // Then
        assert_eq!(name.as_deref(), Some("cargo-nextest"));
    }
}
