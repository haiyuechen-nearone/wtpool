//! The fields of a Claude Code hook payload that `wtpool hook` reads (H5). Everything else, the
//! prompt, notification text and call results included, is never parsed into a string.

use std::fmt;

use serde::Deserialize;
use serde::de::{Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use wtpool_core::activity::{ActorId, CallId};

use crate::events::HookEvent;

/// A parsed payload. Its ids match their patterns, or it does not parse (H17).
#[derive(Debug, Clone, Deserialize)]
pub struct Payload {
    pub session_id: ActorId,
    pub hook_event_name: String,
    pub cwd: String,
    #[serde(default)]
    pub agent_id: Option<ActorId>,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub session_title: Option<String>,
    #[serde(default)]
    pub transcript_path: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_input: Option<Value>,
    #[serde(default)]
    pub tool_use_id: Option<CallId>,
    #[serde(default)]
    pub tool_response: Option<ToolResponse>,
    #[serde(default)]
    pub tool_calls: Option<Vec<BatchCall>>,
    #[serde(default)]
    pub notification_type: Option<String>,
    #[serde(default)]
    pub error: Option<Value>,
    #[serde(default)]
    pub new_cwd: Option<String>,
    /// The background work a Stop or SubagentStop lists in flight (H27).
    #[serde(default)]
    pub background_tasks: Option<Value>,
}

/// One call of a PostToolBatch.
#[derive(Debug, Clone, Deserialize)]
pub struct BatchCall {
    #[serde(default)]
    pub tool_use_id: Option<CallId>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_input: Option<Value>,
}

/// The fields of a call's `tool_response` that `wtpool hook` reads: the `agentId` of the subagent an
/// Agent call started (H16), and the `backgroundTaskId` of a Bash call that went to the
/// background (H27). A response of any other shape, or a field that is no string, reads as none,
/// never as a payload that does not parse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolResponse {
    pub agent_id: Option<String>,
    pub background_task_id: Option<String>,
}

impl<'de> Deserialize<'de> for ToolResponse {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ResponseVisitor)
    }
}

/// Reads a `tool_response` of any shape, keeping only an object's `agentId` and
/// `backgroundTaskId`.
struct ResponseVisitor;

impl<'de> Visitor<'de> for ResponseVisitor {
    type Value = ToolResponse;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any JSON value")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ToolResponse, A::Error> {
        let mut response = ToolResponse::default();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "agentId" => {
                    response.agent_id = map.next_value::<Value>()?.as_str().map(str::to_owned);
                }
                "backgroundTaskId" => {
                    response.background_task_id =
                        map.next_value::<Value>()?.as_str().map(str::to_owned);
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(response)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<ToolResponse, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(ToolResponse::default())
    }

    fn visit_str<E>(self, _: &str) -> Result<ToolResponse, E> {
        Ok(ToolResponse::default())
    }

    fn visit_bool<E>(self, _: bool) -> Result<ToolResponse, E> {
        Ok(ToolResponse::default())
    }

    fn visit_i64<E>(self, _: i64) -> Result<ToolResponse, E> {
        Ok(ToolResponse::default())
    }

    fn visit_u64<E>(self, _: u64) -> Result<ToolResponse, E> {
        Ok(ToolResponse::default())
    }

    fn visit_f64<E>(self, _: f64) -> Result<ToolResponse, E> {
        Ok(ToolResponse::default())
    }
}

impl Payload {
    /// The payload in `input`, or `None` when it is no JSON object with a string `session_id`,
    /// `hook_event_name` and `cwd`, has trailing data, or names an id outside its pattern.
    #[must_use]
    pub fn parse(input: &[u8]) -> Option<Self> {
        serde_json::from_slice(input).ok()
    }

    /// The payload's event, `None` for one the hook ignores.
    #[must_use]
    pub fn event(&self) -> Option<HookEvent> {
        HookEvent::named(&self.hook_event_name)
    }

    /// The text of `error`, a string or any other JSON value written out.
    #[must_use]
    pub fn error_text(&self) -> Option<String> {
        match self.error.as_ref()? {
            Value::String(text) => Some(text.clone()),
            other => Some(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    /// Writes what a visitor expects.
    struct Expecting<'a>(&'a ResponseVisitor);

    impl fmt::Display for Expecting<'_> {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            Visitor::expecting(self.0, formatter)
        }
    }

    const BASE: &str = r#""hook_event_name":"PreToolUse","cwd":"/p""#;

    #[rstest]
    #[case::minimal(format!(r#"{{"session_id":"s1",{BASE}}}"#), true)]
    #[case::sixty_four(format!(r#"{{"session_id":"{}",{BASE}}}"#, "s".repeat(64)), true)]
    #[case::sixty_five(format!(r#"{{"session_id":"{}",{BASE}}}"#, "s".repeat(65)), false)]
    #[case::dot_dot(format!(r#"{{"session_id":"../x",{BASE}}}"#), false)]
    #[case::unicode(format!(r#"{{"session_id":"sé",{BASE}}}"#), false)]
    #[case::empty_agent(format!(r#"{{"session_id":"s1","agent_id":"",{BASE}}}"#), false)]
    #[case::call_128(
        format!(r#"{{"session_id":"s1","tool_use_id":"{}_-",{BASE}}}"#, "t".repeat(126)),
        true
    )]
    #[case::call_slash(format!(r#"{{"session_id":"s1","tool_use_id":"a/b",{BASE}}}"#), false)]
    #[case::batch_call_slash(
        format!(r#"{{"session_id":"s1","tool_calls":[{{"tool_use_id":"a/b"}}],{BASE}}}"#),
        false
    )]
    #[case::number_session(format!(r#"{{"session_id":5,{BASE}}}"#), false)]
    #[case::trailing_garbage(format!(r#"{{"session_id":"s1",{BASE}}} x"#), false)]
    #[case::array("[]".to_owned(), false)]
    fn parse__should_accept_only_valid_payloads(#[case] input: String, #[case] valid: bool) {
        // When
        let payload = Payload::parse(input.as_bytes());

        // Then
        assert_eq!(payload.is_some(), valid);
    }

    #[rstest]
    #[case::agent(
        r#"{"isAsync":true,"agentId":"a1","status":"async_launched"}"#,
        Some("a1")
    )]
    #[case::agent_id_not_a_string(r#"{"agentId":7}"#, None)]
    #[case::other_object(r#"{"stdout":"x","file":{"agentId":"a9"}}"#, None)]
    #[case::text(r#""done""#, None)]
    #[case::list(r#"[{"agentId":"a1"},2]"#, None)]
    #[case::boolean("true", None)]
    #[case::negative("-1", None)]
    #[case::count("3", None)]
    #[case::fraction("1.5", None)]
    #[case::null("null", None)]
    fn parse__should_read_the_agent_id_of_any_tool_response(
        #[case] response: &str,
        #[case] expected: Option<&str>,
    ) {
        // Given
        let input = format!(r#"{{"session_id":"s1","tool_response":{response},{BASE}}}"#);

        // When
        let payload = Payload::parse(input.as_bytes()).unwrap();

        // Then
        let agent = payload.tool_response.and_then(|response| response.agent_id);
        assert_eq!(agent.as_deref(), expected);
    }

    #[test]
    fn parse__should_keep_the_transcript_path() {
        // Given
        let input = format!(r#"{{"session_id":"s1","transcript_path":"/t/s1.jsonl",{BASE}}}"#);

        // When
        let payload = Payload::parse(input.as_bytes()).unwrap();

        // Then
        assert_eq!(payload.transcript_path.as_deref(), Some("/t/s1.jsonl"));
    }

    #[test]
    fn tool_response__should_say_what_it_expects() {
        // When
        let text = Expecting(&ResponseVisitor).to_string();

        // Then
        assert_eq!(text, "any JSON value");
    }

    #[rstest]
    #[case::string(r#""rate_limit""#, Some("rate_limit"))]
    #[case::object(r#"{"code":1}"#, Some(r#"{"code":1}"#))]
    #[case::null("null", None)]
    fn error_text__should_write_any_value(#[case] error: &str, #[case] expected: Option<&str>) {
        // Given
        let input = format!(r#"{{"session_id":"s1","error":{error},{BASE}}}"#);
        let payload = Payload::parse(input.as_bytes()).unwrap();

        // When
        let text = payload.error_text();

        // Then
        assert_eq!(text.as_deref(), expected);
    }
}
