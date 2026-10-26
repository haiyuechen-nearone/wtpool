//! The ids of a hook input and of the records it writes, checked against their patterns before
//! any use. An id names a record file or a task's output file, so only a checked id reaches a
//! path.

use std::fmt;

use serde::{Deserialize, Serialize, Serializer};

/// An id that does not match its pattern.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{what} {id:?} does not match {pattern}")]
pub struct InvalidId {
    what: &'static str,
    pattern: &'static str,
    id: String,
}

fn is_valid(id: &str, max_len: usize, allow_underscore: bool) -> bool {
    (1..=max_len).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || (allow_underscore && b == b'_'))
}

/// Defines an id type that holds only a string matching its pattern.
macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident, $what:literal, $pattern:literal, $max_len:literal, $allow_underscore:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
        #[serde(try_from = "String")]
        pub struct $name(String);

        impl $name {
            /// The longest id the pattern allows.
            pub const MAX_LEN: usize = $max_len;

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = InvalidId;

            fn try_from(id: String) -> Result<Self, InvalidId> {
                if is_valid(&id, Self::MAX_LEN, $allow_underscore) {
                    Ok(Self(id))
                } else {
                    Err(InvalidId {
                        what: $what,
                        pattern: $pattern,
                        id,
                    })
                }
            }
        }

        impl TryFrom<&str> for $name {
            type Error = InvalidId;

            fn try_from(id: &str) -> Result<Self, InvalidId> {
                Self::try_from(id.to_owned())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

id_type!(
    /// A `session_id` or `agent_id`, `^[A-Za-z0-9-]{1,64}$`, the name of a record file.
    ActorId,
    "actor id",
    "^[A-Za-z0-9-]{1,64}$",
    64,
    false
);

id_type!(
    /// The id of a call, `^[A-Za-z0-9_-]{1,128}$`.
    CallId,
    "call id",
    "^[A-Za-z0-9_-]{1,128}$",
    128,
    true
);

id_type!(
    /// The id of a background task, `^[A-Za-z0-9_-]{1,64}$`, the name of its output file.
    TaskId,
    "task id",
    "^[A-Za-z0-9_-]{1,64}$",
    64,
    true
);

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::actor("s-1", true, true, true)]
    #[case::call_only("toolu_1", false, true, true)]
    #[case::task("b2rvgln6d", true, true, true)]
    #[case::longest_actor(&"a".repeat(64), true, true, true)]
    #[case::too_long_for_an_actor(&"a".repeat(65), false, true, false)]
    #[case::too_long_for_a_call(&"a".repeat(129), false, false, false)]
    #[case::empty("", false, false, false)]
    #[case::dot_dot("..", false, false, false)]
    #[case::slash("a/b", false, false, false)]
    #[case::dot("b.output", false, false, false)]
    #[case::unicode("bé", false, false, false)]
    fn ids__should_match_their_patterns(
        #[case] id: &str,
        #[case] actor: bool,
        #[case] call: bool,
        #[case] task: bool,
    ) {
        // When
        let found = (
            ActorId::try_from(id).is_ok(),
            CallId::try_from(id).is_ok(),
            TaskId::try_from(id).is_ok(),
        );

        // Then
        assert_eq!(found, (actor, call, task));
    }

    #[test]
    fn ids__should_read_only_a_valid_string_and_write_it_back() {
        // When
        let read: Result<ActorId, _> = serde_json::from_str(r#""s1""#);
        let invalid = serde_json::from_str::<ActorId>(r#""../x""#);
        let error = TaskId::try_from("a/b").unwrap_err().to_string();

        // Then
        let read = read.unwrap();
        assert_eq!((read.as_str(), read.to_string()), ("s1", "s1".to_owned()));
        assert_eq!(serde_json::to_string(&read).unwrap(), r#""s1""#);
        assert!(invalid.is_err());
        assert_eq!(
            error,
            r#"task id "a/b" does not match ^[A-Za-z0-9_-]{1,64}$"#
        );
    }
}
