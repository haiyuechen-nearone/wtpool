//! OpenCode's recorded sessions as replay steps: the streams its plugin forwards
//! ([`crate::hooks::forwarded_streams`]), each at the time OpenCode recorded it.

use std::path::Path;

use serde_json::Value;

use super::{Step, field};

/// The pid of the fake OpenCode server, the parent of every hook its plugin starts.
pub(super) const SERVER: u32 = 7101;

/// The payload's event or hook name.
pub(super) fn name(payload: &Value) -> &str {
    field(payload, &["name"]).as_str().unwrap_or_default()
}

/// The folder of the plugin instance that sent the payload.
pub(super) fn directory(payload: &Value) -> Option<&str> {
    field(payload, &["directory"]).as_str()
}

/// The session a payload's event names: its subagent's, when the payload has a `parent`.
pub(super) fn session(payload: &Value) -> Option<&str> {
    let data = field(payload, &["data"]);
    field(data, &["sessionID"])
        .as_str()
        .or_else(|| field(data, &["form", "sessionID"]).as_str())
}

/// The replay session of the forwarded stream `stem`: `10-two-folders` is `two_folders`.
pub(super) fn session_name(stem: &str) -> String {
    stem.trim_start_matches(|c: char| c.is_ascii_digit())
        .trim_start_matches('-')
        .replace('-', "_")
}

/// The whole seconds after the stream's first line at which a line `t` milliseconds into the
/// recording runs, rounded up, so a gap of over 10 s in the recording stays over 10 s.
fn seconds_after(first: i64, t: i64) -> i64 {
    (t - first).max(0).div_euclid(1000) + i64::from((t - first).rem_euclid(1000) > 0)
}

/// Every forwarded stream of the newest OpenCode version: (session, steps).
pub(super) fn sessions(root: &Path) -> Vec<(String, Vec<Step>)> {
    crate::hooks::forwarded_streams(root)
        .unwrap_or_default()
        .into_iter()
        .map(|(stem, lines)| {
            let first = lines.first().map_or(0, |(t, _)| *t);
            let steps = lines
                .into_iter()
                .map(|(t, payload)| Step {
                    payload,
                    at: seconds_after(first, t),
                })
                .collect();
            (session_name(&stem), steps)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::two_digits("10-two-folders", "two_folders")]
    #[case::leading_zero("00-provider-error-401", "provider_error_401")]
    #[case::same_number("03-permission-run-autoreject", "permission_run_autoreject")]
    fn session_name__should_drop_the_number_and_use_underscores(
        #[case] stem: &str,
        #[case] expected: &str,
    ) {
        // When
        let found = session_name(stem);

        // Then
        assert_eq!(found, expected);
    }

    #[rstest]
    #[case::first(1000, 1000, 0)]
    #[case::within_a_second(1000, 1133, 1)]
    #[case::whole_seconds(1000, 11_000, 10)]
    #[case::just_past_ten(1000, 11_207, 11)]
    fn seconds_after__should_round_up(#[case] first: i64, #[case] t: i64, #[case] expected: i64) {
        // When
        let found = seconds_after(first, t);

        // Then
        assert_eq!(found, expected);
    }

    #[test]
    fn sessions__should_name_every_replay_of_the_design() {
        // Given
        let root = crate::repo::root();

        // When
        let mut names: Vec<String> = sessions(&root).into_iter().map(|(n, _)| n).collect();
        names.sort();

        // Then
        let mut expected = vec![
            "provider_error_401",
            "serve_shutdown",
            "run_read_edit_shell",
            "subagent",
            "permission_approve_reject",
            "permission_run_autoreject",
            "question",
            "interrupt",
            "title_given",
            "long_answer",
            "deny",
            "delete_api",
            "two_folders",
        ];
        expected.sort_unstable();
        assert_eq!(names, expected);
    }
}
