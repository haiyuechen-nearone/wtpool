//! The coding agent harness whose hooks write a record (H25).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// The harness whose hooks write a record. A record without one was written by Claude Code. Only
/// the harnesses that a wtpool adapter reads are listed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Harness {
    #[default]
    #[serde(rename = "claude-code")]
    ClaudeCode,
}

/// A harness name that no adapter of this wtpool reads.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("no wtpool adapter reads the harness {0:?}")]
pub struct UnknownHarness(pub String);

impl Harness {
    /// The name records and documents carry.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
        }
    }

    /// The start of the file stem of each of the harness's records (H25): none for Claude Code,
    /// `<harness>-` for every other harness.
    #[must_use]
    pub(super) fn stem_prefix(self) -> &'static str {
        match self {
            Self::ClaudeCode => "",
        }
    }
}

impl FromStr for Harness {
    type Err = UnknownHarness;

    fn from_str(name: &str) -> Result<Self, UnknownHarness> {
        if name == Self::ClaudeCode.name() {
            Ok(Self::ClaudeCode)
        } else {
            Err(UnknownHarness(name.to_owned()))
        }
    }
}

impl fmt::Display for Harness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::claude_code("claude-code", Some(Harness::ClaudeCode))]
    #[case::unknown("other-harness", None)]
    #[case::empty("", None)]
    fn from_str__should_know_only_harnesses_with_an_adapter(
        #[case] name: &str,
        #[case] expected: Option<Harness>,
    ) {
        // When
        let harness = name.parse::<Harness>();

        // Then
        assert_eq!(harness.clone().ok(), expected);
        assert_eq!(harness.is_err(), expected.is_none());
        assert_eq!(Harness::default().name(), "claude-code");
    }

    #[test]
    fn harness__should_serialize_and_display_by_name() {
        // When
        let json = serde_json::to_string(&Harness::ClaudeCode).unwrap();
        let shown = Harness::ClaudeCode.to_string();
        let unknown = "x".parse::<Harness>().unwrap_err().to_string();

        // Then
        assert_eq!(
            (json.as_str(), shown.as_str(), unknown.as_str()),
            (
                r#""claude-code""#,
                "claude-code",
                r#"no wtpool adapter reads the harness "x""#
            )
        );
    }
}
