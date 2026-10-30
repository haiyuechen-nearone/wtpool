use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Serialize, Serializer};

/// The name of the harness whose hooks write a record, `^[a-z][a-z0-9]*(-[a-z0-9]+)*$` in at
/// most [`HarnessName::MAX_LEN`] characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct HarnessName(Cow<'static, str>);

/// A harness name that does not match its pattern.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "harness name {0:?} does not match ^[a-z][a-z0-9]*(-[a-z0-9]+)*$ in at most {max} characters",
    max = HarnessName::MAX_LEN
)]
pub struct InvalidHarnessName(String);

impl HarnessName {
    /// The longest name the pattern allows.
    pub const MAX_LEN: usize = 24;

    /// The name of every record written before records named their harness. A record without a
    /// name reads as this one, the stems of its records carry no prefix, and
    /// [`legacy_kind`](super::legacy_kind) maps the tools of its calls without a kind.
    pub const LEGACY: Self = Self::new("claude-code");

    /// The name `name`. Fails to compile in a constant, and panics elsewhere, when `name` does not
    /// match the pattern:
    ///
    /// ```compile_fail,E0080
    /// use wtpool_core::activity::HarnessName;
    ///
    /// const NAME: HarnessName = HarnessName::new("Claude Code");
    /// let _ = NAME;
    /// ```
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        assert!(
            is_valid(name),
            "a harness name matches ^[a-z][a-z0-9]*(-[a-z0-9]+)*$ in at most 24 characters"
        );
        Self(Cow::Borrowed(name))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `true` for [`HarnessName::LEGACY`].
    #[must_use]
    pub fn is_legacy(&self) -> bool {
        *self == Self::LEGACY
    }
}

/// `true` for a name that matches the pattern in at most [`HarnessName::MAX_LEN`] characters.
const fn is_valid(name: &str) -> bool {
    let bytes = name.as_bytes();
    let Some((first, mut rest)) = bytes.split_first() else {
        return false;
    };
    if bytes.len() > HarnessName::MAX_LEN || !first.is_ascii_lowercase() {
        return false;
    }
    let mut after_dash = false;
    while let [byte, others @ ..] = rest {
        after_dash = match *byte {
            b'-' if after_dash => return false,
            b'-' => true,
            b'a'..=b'z' | b'0'..=b'9' => false,
            _ => return false,
        };
        rest = others;
    }
    !after_dash
}

impl Serialize for HarnessName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl TryFrom<String> for HarnessName {
    type Error = InvalidHarnessName;

    fn try_from(name: String) -> Result<Self, InvalidHarnessName> {
        if is_valid(&name) {
            Ok(Self(Cow::Owned(name)))
        } else {
            Err(InvalidHarnessName(name))
        }
    }
}

impl TryFrom<&str> for HarnessName {
    type Error = InvalidHarnessName;

    fn try_from(name: &str) -> Result<Self, InvalidHarnessName> {
        Self::try_from(name.to_owned())
    }
}

impl fmt::Display for HarnessName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::legacy("claude-code", true)]
    #[case::one_letter("x", true)]
    #[case::digits("h2o-v2", true)]
    #[case::longest(&"a".repeat(24), true)]
    #[case::too_long(&"a".repeat(25), false)]
    #[case::empty("", false)]
    #[case::leading_digit("2x", false)]
    #[case::leading_dash("-x", false)]
    #[case::trailing_dash("x-", false)]
    #[case::double_dash("x--y", false)]
    #[case::upper_case("Claude", false)]
    #[case::underscore("x_y", false)]
    #[case::dot("x.y", false)]
    #[case::unicode("é", false)]
    fn try_from__should_take_only_a_name_matching_the_pattern(
        #[case] name: &str,
        #[case] valid: bool,
    ) {
        // When
        let parsed = HarnessName::try_from(name);

        // Then
        assert_eq!(
            parsed.map(|name| name.to_string()).ok(),
            valid.then(|| name.to_owned())
        );
    }

    #[test]
    fn harness_name__should_serialize_display_and_refuse_by_name() {
        // When
        let json = serde_json::to_string(&HarnessName::LEGACY).unwrap();
        let read: HarnessName = serde_json::from_str(r#""other-harness""#).unwrap();
        let invalid = serde_json::from_str::<HarnessName>(r#""Other""#);
        let error = HarnessName::try_from("x_y").unwrap_err().to_string();

        // Then
        assert_eq!(json, r#""claude-code""#);
        assert_eq!(
            (
                read.as_str(),
                read.is_legacy(),
                HarnessName::LEGACY.is_legacy()
            ),
            ("other-harness", false, true)
        );
        assert!(invalid.is_err());
        assert_eq!(
            error,
            r#"harness name "x_y" does not match ^[a-z][a-z0-9]*(-[a-z0-9]+)*$ in at most 24 characters"#
        );
    }
}
