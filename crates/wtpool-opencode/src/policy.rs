//! The policy check of OpenCode's shell calls: the plugin's request for a `shell` call about to
//! run, `{tool, sessionID, id, input: {command}, directory}`, and the line that denies it.

use std::path::Path;

use serde::Deserialize;
use wtpool_core::policy::{Dialect, Refusal, ShellCall};

use crate::tools::SHELL;

#[derive(Deserialize)]
struct Request {
    tool: String,
    input: Input,
    directory: String,
}

#[derive(Deserialize)]
struct Input {
    command: String,
}

/// The shell call of a policy request, `None` for any other input. OpenCode's shell tool runs in
/// the session's folder and has no background mode.
#[must_use]
pub fn shell_call(input: &[u8]) -> Option<ShellCall> {
    let request: Request = serde_json::from_slice(input).ok()?;
    if request.tool != SHELL || !Path::new(&request.directory).is_absolute() {
        return None;
    }
    Some(ShellCall {
        cwd: request.directory,
        command: request.input.command,
        dialect: Dialect::Posix,
        background: false,
    })
}

/// The one line the plugin throws as the call's error for `refusal`.
#[must_use]
pub fn refusal(refusal: &Refusal) -> String {
    let reason = serde_json::Value::String(refusal.to_string());
    format!("{{\"decision\":\"deny\",\"reason\":{reason}}}\n")
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::{Value, json};

    use super::*;

    fn request(tool: &str, command: &Value, directory: &str) -> Vec<u8> {
        json!({
            "tool": tool,
            "sessionID": "ses_1",
            "id": "call_1",
            "input": {"command": command},
            "directory": directory,
        })
        .to_string()
        .into_bytes()
    }

    #[rstest]
    #[case::shell("shell", json!("cargo test"), "/p/fg", true)]
    #[case::read("read", json!("cargo test"), "/p/fg", false)]
    #[case::a_number("shell", json!(7), "/p/fg", false)]
    #[case::relative_directory("shell", json!("cargo test"), "fg", false)]
    fn shell_call__should_read_only_shell_calls(
        #[case] tool: &str,
        #[case] command: Value,
        #[case] directory: &str,
        #[case] read: bool,
    ) {
        // When
        let call = shell_call(&request(tool, &command, directory));

        // Then
        let expected = read.then(|| ShellCall {
            cwd: "/p/fg".to_owned(),
            command: "cargo test".to_owned(),
            dialect: Dialect::Posix,
            background: false,
        });
        assert_eq!(call, expected);
    }

    #[rstest]
    #[case::not_json(b"{".as_slice())]
    #[case::no_input(br#"{"tool":"shell","directory":"/p"}"#.as_slice())]
    #[case::event_payload(br#"{"v":1,"kind":"hook","name":"tool.execute.before","directory":"/p","data":{"tool":"shell","input":{"command":"make"}}}"#.as_slice())]
    fn shell_call__should_read_nothing_else(#[case] input: &[u8]) {
        // When
        let call = shell_call(input);

        // Then
        assert_eq!(call, None);
    }

    #[test]
    fn refusal__should_print_one_deny_line() {
        // Given
        let heavy = Refusal::Heavy {
            names: vec!["cargo test".to_owned()],
            wrapped: "wtpool job -- cargo test".to_owned(),
        };

        // When
        let line = refusal(&heavy);

        // Then
        assert_eq!(
            line,
            "{\"decision\":\"deny\",\"reason\":\"wtpool: cargo test is heavy on this machine \
             (enforce = require). Run it as: wtpool job -- cargo test\"}\n"
        );
        let parsed: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed.as_object().unwrap().len(), 2);
    }
}
