//! The values a record holds, as `wtpool hook` writes them and `wtpool status` reads them.

use serde::{Deserialize, Serialize};

/// An actor's state, as hook events set it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Working,
    Waiting,
    Idle,
    Done,
    Ended,
}

/// The source of an actor's worktree: its first `cwd`, or a path it used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacedBy {
    Cwd,
    Path,
}

/// The cause of an actor's wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitReason {
    Permission,
    Question,
    Plan,
    Input,
}

/// The kind of work a call does, as the harness's adapter tells from its tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallKind {
    /// Runs a shell command, and may edit files too.
    Shell,
    /// Edits a file.
    Edit,
    /// Starts a subagent.
    Agent,
    /// Anything else, and any kind this wtpool does not know.
    #[serde(other)]
    Other,
}

impl PlacedBy {
    pub const ALL: [Self; 2] = [Self::Cwd, Self::Path];
}

impl WaitReason {
    pub const ALL: [Self; 4] = [Self::Permission, Self::Question, Self::Plan, Self::Input];
}

impl CallKind {
    /// `true` when the call may have changed files in its worktree.
    #[must_use]
    pub fn edits_files(self) -> bool {
        matches!(self, Self::Edit | Self::Shell)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::shell(r#""shell""#, CallKind::Shell, true)]
    #[case::edit(r#""edit""#, CallKind::Edit, true)]
    #[case::agent(r#""agent""#, CallKind::Agent, false)]
    #[case::other(r#""other""#, CallKind::Other, false)]
    #[case::unknown(r#""search""#, CallKind::Other, false)]
    fn call_kind__should_read_every_kind_and_tell_edits(
        #[case] json: &str,
        #[case] expected: CallKind,
        #[case] edits: bool,
    ) {
        // When
        let kind: CallKind = serde_json::from_str(json).unwrap();

        // Then
        assert_eq!(kind, expected);
        assert_eq!(kind.edits_files(), edits);
        assert_eq!(
            serde_json::to_string(&CallKind::Shell).unwrap(),
            r#""shell""#
        );
    }
}
