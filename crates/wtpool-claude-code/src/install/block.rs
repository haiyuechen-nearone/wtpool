//! The hook block of `wtpool hook install`, built from [`HookEvent::ALL`].

use serde_json::{Map, Value, json};

use crate::events::HookEvent;

/// The `{"hooks": {...}}` block that runs `binary hook` on each event.
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
}
