//! Claude Code's tools: the field that names a call's path (H8), the tools that wait for the
//! user (H13) and the kind of each call (H26).

use serde_json::Value;
use wtpool_core::activity::{CallKind, WaitReason};
use wtpool_core::hook::{Shell, ToolPath, WaitingTools};

/// The tools whose calls wait for the user (H13): `AskUserQuestion` asks a question,
/// `ExitPlanMode` for a plan's approval.
pub const WAITING: WaitingTools = WaitingTools(&[
    ("AskUserQuestion", WaitReason::Question),
    ("ExitPlanMode", WaitReason::Plan),
]);

/// The kind of a call of `tool` (H26): Bash runs a shell, Edit, Write and NotebookEdit edit a
/// file, Agent starts a subagent.
#[must_use]
pub fn kind_of(tool: &str) -> CallKind {
    match tool {
        "Bash" => CallKind::Shell,
        "Edit" | "Write" | "NotebookEdit" => CallKind::Edit,
        "Agent" => CallKind::Agent,
        _ => CallKind::Other,
    }
}

/// The path field of `tool` in `input` (H8): `file_path` (Read, Edit, Write), `path` (Grep,
/// Glob, EnterWorktree), `notebook_path` (NotebookEdit), `command` (Bash, PowerShell, Monitor).
/// Every command reads as a POSIX shell's, PowerShell's included.
#[must_use]
pub fn tool_path(tool: &str, input: Option<&Value>) -> ToolPath {
    let field = |name: &str| {
        input
            .and_then(|input| input.get(name))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let command = |command| ToolPath::Command {
        command,
        shell: Shell::Posix,
    };
    let found = match tool {
        "Read" | "Edit" | "Write" => field("file_path").map(ToolPath::File),
        "Grep" | "Glob" | "EnterWorktree" => field("path").map(ToolPath::File),
        "NotebookEdit" => field("notebook_path").map(ToolPath::File),
        "Bash" | "PowerShell" | "Monitor" => field("command").map(command),
        _ => None,
    };
    found.unwrap_or(ToolPath::None)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::json;
    use wtpool_core::activity::Harness;
    use wtpool_core::activity::legacy_kind;

    use super::*;

    #[rstest]
    #[case::bash("Bash", CallKind::Shell)]
    #[case::edit("Edit", CallKind::Edit)]
    #[case::write("Write", CallKind::Edit)]
    #[case::notebook("NotebookEdit", CallKind::Edit)]
    #[case::agent("Agent", CallKind::Agent)]
    #[case::read("Read", CallKind::Other)]
    #[case::power_shell("PowerShell", CallKind::Other)]
    #[case::monitor("Monitor", CallKind::Other)]
    #[case::lower_case("bash", CallKind::Other)]
    #[case::none("", CallKind::Other)]
    fn kind_of__should_map_the_tools(#[case] tool: &str, #[case] expected: CallKind) {
        // When
        let kind = kind_of(tool);

        // Then
        assert_eq!(kind, expected);
        assert_eq!(legacy_kind(Harness::ClaudeCode, tool), expected);
    }

    #[rstest]
    #[case::question("AskUserQuestion", Some(WaitReason::Question))]
    #[case::plan("ExitPlanMode", Some(WaitReason::Plan))]
    #[case::bash("Bash", None)]
    fn waiting__should_name_the_tools_that_wait_for_the_user(
        #[case] tool: &str,
        #[case] expected: Option<WaitReason>,
    ) {
        // When
        let reason = WAITING.reason(tool);

        // Then
        assert_eq!(reason, expected);
    }

    #[rstest]
    #[case::read("Read", "file_path", ToolPath::File("/p".to_owned()))]
    #[case::write("Write", "file_path", ToolPath::File("/p".to_owned()))]
    #[case::grep("Grep", "path", ToolPath::File("/p".to_owned()))]
    #[case::enter_worktree("EnterWorktree", "path", ToolPath::File("/p".to_owned()))]
    #[case::notebook("NotebookEdit", "notebook_path", ToolPath::File("/p".to_owned()))]
    #[case::bash("Bash", "command", ToolPath::Command { command: "/p".to_owned(), shell: Shell::Posix })]
    #[case::power_shell("PowerShell", "command", ToolPath::Command { command: "/p".to_owned(), shell: Shell::Posix })]
    #[case::monitor("Monitor", "command", ToolPath::Command { command: "/p".to_owned(), shell: Shell::Posix })]
    #[case::read_with_path("Read", "path", ToolPath::None)]
    #[case::web_search("WebSearch", "query", ToolPath::None)]
    fn tool_path__should_read_the_field_of_each_tool(
        #[case] tool: &str,
        #[case] field: &str,
        #[case] expected: ToolPath,
    ) {
        // Given
        let input = json!({ field: "/p" });

        // When
        let found = tool_path(tool, Some(&input));

        // Then
        assert_eq!(found, expected);
    }
}
