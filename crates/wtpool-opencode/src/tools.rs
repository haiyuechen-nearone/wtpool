//! OpenCode's tools: the kind of each call, the field that names its path, and the tools that wait
//! for the user.

use std::path::Path;

use wtpool_core::activity::{CallKind, WaitReason};
use wtpool_core::hook::{Shell, ToolPath, WaitingTools};

/// The tool that runs a command in a shell.
pub const SHELL: &str = "shell";
/// The field of a shell call's command, as the plugin keeps it.
pub const COMMAND: &str = "input.command";

/// The tools whose calls wait for the user.
pub const WAITING: WaitingTools = WaitingTools(&[("question", WaitReason::Question)]);

/// The tools whose `path` names the file or folder they work on.
const PATH_TOOLS: [&str; 5] = ["read", "edit", "write", "glob", "grep"];

#[must_use]
pub fn kind_of(tool: &str) -> CallKind {
    match tool {
        SHELL => CallKind::Shell,
        "edit" | "write" | "patch" => CallKind::Edit,
        "subagent" => CallKind::Agent,
        _ => CallKind::Other,
    }
}

/// The path of a call of `tool` with the input fields `path` and `command`, a relative path
/// joined to the session's folder `directory`. A command runs in `directory` whatever it `cd`s
/// to, as OpenCode starts every shell there.
#[must_use]
pub fn tool_path(
    tool: &str,
    path: Option<&str>,
    command: Option<&str>,
    directory: &str,
) -> ToolPath {
    let found = match tool {
        SHELL => command.map(|command| ToolPath::Command {
            command: command.to_owned(),
            shell: Shell::Posix,
        }),
        _ if PATH_TOOLS.contains(&tool) => path
            .filter(|path| !path.is_empty())
            .map(|path| ToolPath::File(Path::new(directory).join(path).display().to_string())),
        _ => None,
    };
    found.unwrap_or(ToolPath::None)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::shell("shell", CallKind::Shell)]
    #[case::edit("edit", CallKind::Edit)]
    #[case::write("write", CallKind::Edit)]
    #[case::patch("patch", CallKind::Edit)]
    #[case::subagent("subagent", CallKind::Agent)]
    #[case::read("read", CallKind::Other)]
    #[case::question("question", CallKind::Other)]
    #[case::bash_is_gone("bash", CallKind::Other)]
    #[case::none("", CallKind::Other)]
    fn kind_of__should_map_the_tools(#[case] tool: &str, #[case] expected: CallKind) {
        // When
        let kind = kind_of(tool);

        // Then
        assert_eq!(kind, expected);
    }

    #[rstest]
    #[case::relative("read", Some("../wt1/notes.txt"), None, ToolPath::File("/p/fg/../wt1/notes.txt".to_owned()))]
    #[case::absolute("edit", Some("/p/wt1/a.rs"), None, ToolPath::File("/p/wt1/a.rs".to_owned()))]
    #[case::write("write", Some("b.rs"), None, ToolPath::File("/p/fg/b.rs".to_owned()))]
    #[case::glob("glob", Some("src"), None, ToolPath::File("/p/fg/src".to_owned()))]
    #[case::grep("grep", Some("src"), None, ToolPath::File("/p/fg/src".to_owned()))]
    #[case::shell("shell", None, Some("cd ../wt1 && ls"), ToolPath::Command { command: "cd ../wt1 && ls".to_owned(), shell: Shell::Posix })]
    #[case::shell_without_command("shell", Some("x"), None, ToolPath::None)]
    #[case::patch("patch", Some("a.rs"), None, ToolPath::None)]
    #[case::empty_path("read", Some(""), None, ToolPath::None)]
    #[case::subagent("subagent", None, None, ToolPath::None)]
    fn tool_path__should_read_the_field_of_each_tool(
        #[case] tool: &str,
        #[case] path: Option<&str>,
        #[case] command: Option<&str>,
        #[case] expected: ToolPath,
    ) {
        // When
        let found = tool_path(tool, path, command, "/p/fg");

        // Then
        assert_eq!(found, expected);
    }

    #[rstest]
    #[case::question("question", Some(WaitReason::Question))]
    #[case::shell("shell", None)]
    fn waiting__should_name_the_question_tool(
        #[case] tool: &str,
        #[case] expected: Option<WaitReason>,
    ) {
        // When
        let reason = WAITING.reason(tool);

        // Then
        assert_eq!(reason, expected);
    }
}
