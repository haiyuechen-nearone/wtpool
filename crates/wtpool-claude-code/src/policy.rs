//! The policy check of Claude Code's shell calls: the call a PreToolUse payload is about to run,
//! and the decision that refuses it.

use serde_json::json;
use wtpool_core::hook::ShellCall;

use crate::events::HookEvent;
use crate::payload::Payload;

/// The tools whose `command` runs in a shell.
const SHELL_TOOLS: [&str; 3] = ["Bash", "PowerShell", "Monitor"];

/// The shell call of a PreToolUse payload of a shell tool, `None` for any other input.
#[must_use]
pub fn shell_call(input: &[u8]) -> Option<ShellCall> {
    let payload = Payload::parse(input)?;
    if payload.event() != Some(HookEvent::PreToolUse) {
        return None;
    }
    let tool = payload.tool_name.as_deref()?;
    if !SHELL_TOOLS.contains(&tool) {
        return None;
    }
    let command = payload.tool_input.string("command")?.to_owned();
    Some(ShellCall {
        cwd: payload.cwd,
        command,
    })
}

/// The one line that denies the call with `reason`.
#[must_use]
pub fn refusal(reason: &str) -> String {
    let decision = json!({
        "hookSpecificOutput": {
            "hookEventName": HookEvent::PreToolUse.name(),
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    });
    format!("{decision}\n")
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::Value;

    use super::*;

    fn payload(event: &str, tool: &str, input: &Value) -> Vec<u8> {
        json!({
            "session_id": "s1",
            "hook_event_name": event,
            "cwd": "/p/wt1",
            "tool_name": tool,
            "tool_input": input,
        })
        .to_string()
        .into_bytes()
    }

    #[rstest]
    #[case::bash("PreToolUse", "Bash", json!({"command": "make"}), true)]
    #[case::power_shell("PreToolUse", "PowerShell", json!({"command": "make"}), true)]
    #[case::monitor("PreToolUse", "Monitor", json!({"command": "make"}), true)]
    #[case::read("PreToolUse", "Read", json!({"file_path": "/p/f"}), false)]
    #[case::after_the_call("PostToolUse", "Bash", json!({"command": "make"}), false)]
    #[case::no_command("PreToolUse", "Bash", json!({"description": "x"}), false)]
    #[case::a_number("PreToolUse", "Bash", json!({"command": 42}), false)]
    fn shell_call__should_read_only_shell_calls_before_they_run(
        #[case] event: &str,
        #[case] tool: &str,
        #[case] input: Value,
        #[case] read: bool,
    ) {
        // When
        let call = shell_call(&payload(event, tool, &input));

        // Then
        let expected = read.then(|| ShellCall {
            cwd: "/p/wt1".to_owned(),
            command: "make".to_owned(),
        });
        assert_eq!(call, expected);
    }

    #[test]
    fn refusal__should_print_one_deny_decision() {
        // When
        let line = refusal("wtpool: make is heavy");

        // Then
        assert_eq!(
            line,
            "{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"deny\",\"permissionDecisionReason\":\"wtpool: make is heavy\"}}\n"
        );
    }
}
