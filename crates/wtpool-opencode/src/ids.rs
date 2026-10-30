//! OpenCode's ids as wtpool keeps them. A session id of OpenCode's own form, such as
//! `ses_e76df7aa0ffe3vdlGbn0I4xCYq`, takes a readable form with a `-` for its `_`, so its record
//! names the session as OpenCode does; any other id is kept or hashed as core's ids are.

use wtpool_core::activity::{ActorId, CallId};

/// The stem prefix of every OpenCode record, and of the `WTPOOL_SESSION` the plugin sets.
pub const STEM_PREFIX: &str = "opencode-";
/// The most lowercase letters before the `_` of an id of OpenCode's own form.
pub const LETTERS: usize = 8;
/// The longest id of OpenCode's own form.
pub const MAX_LEN: usize = ActorId::MAX_LEN;

/// The readable form of `raw` when it has OpenCode's own form: 1 to [`LETTERS`] lowercase
/// letters, `_`, then ASCII letters and digits, [`MAX_LEN`] characters at most.
#[must_use]
pub fn readable(raw: &str) -> Option<String> {
    let (letters, rest) = raw.split_once('_')?;
    let own_form = raw.len() <= MAX_LEN
        && (1..=LETTERS).contains(&letters.len())
        && letters.bytes().all(|b| b.is_ascii_lowercase())
        && !rest.is_empty()
        && rest.bytes().all(|b| b.is_ascii_alphanumeric());
    own_form.then(|| format!("{letters}-{rest}"))
}

/// The actor id of a session or subagent id: its readable form, else `raw` when it matches the
/// actor pattern, else its hash.
#[must_use]
pub fn actor(raw: &str) -> ActorId {
    readable(raw)
        .and_then(|id| ActorId::try_from(id).ok())
        .unwrap_or_else(|| ActorId::hashed(raw))
}

/// The id of a call: `raw` when it matches the call pattern, else its hash.
#[must_use]
pub fn call(raw: &str) -> CallId {
    CallId::hashed(raw)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const SESSION: &str = "ses_e76df7aa0ffe3vdlGbn0I4xCYq";
    const DOT: &str = "h2e7336dc8eba87ef472df568c35482abf2575dc3";

    #[rstest]
    #[case::session(SESSION, "ses-e76df7aa0ffe3vdlGbn0I4xCYq")]
    #[case::eight_letters("abcdefgh_1", "abcdefgh-1")]
    #[case::longest(&format!("ses_{}", "a".repeat(60)), &format!("ses-{}", "a".repeat(60)))]
    #[case::without_underscore("s-1", "s-1")]
    #[case::dot("a.b", DOT)]
    fn actor__should_map_opencode_ids(#[case] raw: &str, #[case] expected: &str) {
        // When
        let id = actor(raw);

        // Then
        assert_eq!(id.as_str(), expected);
    }

    #[rstest]
    #[case::nine_letters("abcdefghi_1")]
    #[case::upper_case_letters("Ses_1")]
    #[case::no_letters("_1")]
    #[case::nothing_after("ses_")]
    #[case::two_underscores("ses_a_b")]
    #[case::a_dash_after("ses_a-b")]
    #[case::colon("ses_a:b")]
    #[case::unicode("ses_é")]
    #[case::too_long(&format!("ses_{}", "a".repeat(61)))]
    fn readable__should_take_only_opencodes_own_form(#[case] raw: &str) {
        // When
        let found = readable(raw);

        // Then
        assert_eq!(found, None);
        assert!(actor(raw).as_str().starts_with('h'), "{raw}");
        assert_eq!(actor(raw).as_str().len(), 41);
    }

    #[test]
    fn readable__should_give_a_stem_within_the_record_grammar() {
        // When
        let stem = format!("{STEM_PREFIX}{}", actor(SESSION));

        // Then
        assert_eq!(stem, "opencode-ses-e76df7aa0ffe3vdlGbn0I4xCYq");
        assert_eq!(stem.len(), 39);
    }

    #[test]
    fn call__should_keep_a_valid_id_and_hash_any_other() {
        // When
        let (kept, hashed) = (call("call_s1_0_0"), call("functions.read:0"));

        // Then
        assert_eq!(kept.as_str(), "call_s1_0_0");
        assert!(hashed.as_str().starts_with('h'));
        assert_eq!(hashed.as_str().len(), 41);
    }
}
