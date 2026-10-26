//! The kinds of the calls an older wtpool wrote without one. Only a wtpool that read Claude Code
//! alone wrote such calls, so the map stays as that adapter had it, and a new harness's records
//! always name their kinds.

use super::harness::Harness;
use super::values::CallKind;

/// The kind of a call of `tool` that an older wtpool wrote without one in a record of `harness`.
#[must_use]
pub fn legacy_kind(harness: Harness, tool: &str) -> CallKind {
    match harness {
        Harness::ClaudeCode => match tool {
            "Bash" => CallKind::Shell,
            "Edit" | "Write" | "NotebookEdit" => CallKind::Edit,
            "Agent" => CallKind::Agent,
            _ => CallKind::Other,
        },
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::bash("Bash", CallKind::Shell)]
    #[case::edit("Edit", CallKind::Edit)]
    #[case::write("Write", CallKind::Edit)]
    #[case::notebook("NotebookEdit", CallKind::Edit)]
    #[case::agent("Agent", CallKind::Agent)]
    #[case::read("Read", CallKind::Other)]
    #[case::lower_case("bash", CallKind::Other)]
    #[case::none("", CallKind::Other)]
    fn legacy_kind__should_map_the_tools_of_older_records(
        #[case] tool: &str,
        #[case] expected: CallKind,
    ) {
        // When
        let kind = legacy_kind(Harness::ClaudeCode, tool);

        // Then
        assert_eq!(kind, expected);
    }
}
