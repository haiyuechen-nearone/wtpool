//! The hook block of `wtpool hook install`, built from [`HookEvent::ALL`], and the policy entry
//! that `wtpool hook policy` runs on shell calls.

use serde_json::{Map, Value, json};

use crate::events::HookEvent;

/// The arguments of the entry that judges a shell call before it runs.
pub const POLICY_ARGS: [&str; 2] = ["hook", "policy"];
/// The tools whose calls the policy entry judges.
const POLICY_MATCHER: &str = "Bash|PowerShell|Monitor";
/// Claude Code waits for the policy entry, so it ends well before Claude Code would give up.
const POLICY_TIMEOUT: u64 = 2;

/// The `{"hooks": {...}}` block that runs `binary hook` on each event, and `binary hook policy`
/// before each shell call.
#[must_use]
pub fn block(binary: &str) -> Value {
    let mut hooks = Map::new();
    for event in HookEvent::ALL {
        let mut entry = Map::new();
        entry.insert("type".to_owned(), json!("command"));
        entry.insert("command".to_owned(), json!(binary));
        entry.insert("args".to_owned(), json!(["hook"]));
        entry.insert("onFailure".to_owned(), json!("continue"));
        if event.is_async() {
            entry.insert("async".to_owned(), json!(true));
        } else if event != HookEvent::SessionEnd {
            entry.insert("timeout".to_owned(), json!(2));
        }
        hooks.insert(event.name().to_owned(), json!([{ "hooks": [entry] }]));
    }
    if let Some(Value::Array(groups)) = hooks.get_mut(HookEvent::PreToolUse.name()) {
        groups.push(json!({
            "matcher": POLICY_MATCHER,
            "hooks": [{
                "type": "command",
                "command": binary,
                "args": POLICY_ARGS,
                "timeout": POLICY_TIMEOUT,
                "onFailure": "continue",
            }],
        }));
    }
    json!({ "hooks": hooks })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::session_end("SessionEnd", None, None)]
    #[case::sync_event("Stop", None, Some(2))]
    #[case::async_event("PreToolUse", Some(true), None)]
    fn block__should_follow_the_event_table(
        #[case] event: &str,
        #[case] is_async: Option<bool>,
        #[case] timeout: Option<u64>,
    ) {
        // When
        let block = block("/bin/wtpool");

        // Then
        let entry = &block["hooks"][event][0]["hooks"][0];
        assert_eq!(entry["command"], "/bin/wtpool");
        assert_eq!(entry["args"], json!(["hook"]));
        assert_eq!(entry["onFailure"], "continue");
        assert_eq!(entry["async"].as_bool(), is_async);
        assert_eq!(entry["timeout"].as_u64(), timeout);
        assert_eq!(block["hooks"].as_object().unwrap().len(), 14);
    }

    #[test]
    fn block__should_hold_one_sync_policy_entry_for_shell_calls() {
        // When
        let block = block("/bin/wtpool");

        // Then
        let groups = block["hooks"]["PreToolUse"].as_array().unwrap();
        let policies: Vec<&Value> = groups
            .iter()
            .filter(|group| group["hooks"][0]["args"] == json!(POLICY_ARGS))
            .collect();
        assert_eq!(
            policies,
            vec![&json!({
                "matcher": "Bash|PowerShell|Monitor",
                "hooks": [{
                    "type": "command",
                    "command": "/bin/wtpool",
                    "args": ["hook", "policy"],
                    "timeout": 2,
                    "onFailure": "continue",
                }],
            })]
        );
        assert_eq!(groups.len(), 2);
    }
}
