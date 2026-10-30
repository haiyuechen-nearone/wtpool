//! One payload of the plugin: `{"v":1,"kind","name","directory","parent"?,"call"?,"data"}`,
//! checked against the event table before anything reads it.

use std::fmt;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::table::{self, Entry};

/// The only payload version this wtpool reads.
const VERSION: u64 = 1;

/// A payload of the event table, with every field its entry needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Payload {
    pub entry: &'static Entry,
    /// The folder of the plugin instance that sent it, an absolute path.
    pub directory: String,
    /// The root session of a child session's event.
    pub parent: Option<String>,
    /// The call a reply answers.
    pub call: Option<String>,
    data: Map<String, Value>,
}

/// The reason a payload writes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejected {
    /// No JSON object of the payload's shape.
    Malformed(String),
    Version(String),
    /// A hook or event outside the table.
    Unknown {
        kind: String,
        name: String,
    },
    /// A field the entry needs is missing or no text.
    Missing {
        name: &'static str,
        field: &'static str,
    },
    RelativeDirectory,
}

impl fmt::Display for Rejected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(error) => write!(formatter, "the payload is malformed ({error})"),
            Self::Version(version) => write!(formatter, "the payload has version {version}"),
            Self::Unknown { kind, name } => {
                write!(formatter, "the {kind} {name:?} is not in the event table")
            }
            Self::Missing { name, field } => {
                write!(formatter, "the {name} payload lacks {field}")
            }
            Self::RelativeDirectory => write!(formatter, "the payload's directory is relative"),
        }
    }
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    v: Value,
    kind: String,
    name: String,
    directory: String,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    call: Option<String>,
    data: Map<String, Value>,
}

impl Payload {
    /// The payload in `input`.
    ///
    /// # Errors
    ///
    /// [`Rejected`] names the first fault: no payload's shape, another version, a name outside
    /// the table, a missing field its entry needs, or a relative `directory`.
    pub fn parse(input: &[u8]) -> Result<Self, Rejected> {
        let raw: Raw = serde_json::from_slice(input)
            .map_err(|error| Rejected::Malformed(error.to_string()))?;
        if raw.v.as_u64() != Some(VERSION) {
            return Err(Rejected::Version(raw.v.to_string()));
        }
        let entry = table::entry(&raw.kind, &raw.name).ok_or_else(|| Rejected::Unknown {
            kind: raw.kind.clone(),
            name: raw.name.clone(),
        })?;
        let payload = Self {
            entry,
            directory: raw.directory,
            parent: raw.parent.filter(|parent| !parent.is_empty()),
            call: raw.call.filter(|call| !call.is_empty()),
            data: raw.data,
        };
        if let Some(field) = entry
            .needs
            .iter()
            .find(|field| payload.string(field).is_none())
        {
            return Err(Rejected::Missing {
                name: entry.name,
                field,
            });
        }
        if !Path::new(&payload.directory).is_absolute() {
            return Err(Rejected::RelativeDirectory);
        }
        Ok(payload)
    }

    /// The kept field at the dotted `path`.
    #[must_use]
    pub fn field(&self, path: &str) -> Option<&Value> {
        let mut parts = path.split('.');
        let first = self.data.get(parts.next()?)?;
        parts.try_fold(first, |value, key| value.get(key))
    }

    /// The kept field at `path`, when it is text that is not empty.
    #[must_use]
    pub fn string(&self, path: &str) -> Option<&str> {
        self.field(path)
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
    }

    /// The id of the session the payload is about, present once [`Payload::parse`] accepted it.
    #[must_use]
    pub fn session(&self) -> &str {
        self.string(self.entry.session).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use serde_json::json;

    use super::*;
    use crate::table::Meaning;

    fn parse(payload: &Value) -> Result<Payload, Rejected> {
        Payload::parse(payload.to_string().as_bytes())
    }

    fn started() -> Value {
        json!({
            "v": 1, "kind": "event", "name": "session.execution.started", "directory": "/p/fg",
            "data": {"sessionID": "ses_1"},
        })
    }

    #[test]
    fn parse__should_read_a_payload_of_the_table() {
        // Given
        let payload = json!({
            "v": 1, "kind": "event", "name": "form.created", "directory": "/p/fg",
            "parent": "ses_root", "call": "c1",
            "data": {"form": {"id": "frm_1", "sessionID": "ses_child", "metadata": {"kind": "question", "tool": {"id": "call_1"}}}},
        });

        // When
        let parsed = parse(&payload).unwrap();

        // Then
        assert_eq!(parsed.entry.meaning, Meaning::FormCreated);
        assert_eq!(parsed.session(), "ses_child");
        assert_eq!(
            (parsed.parent.as_deref(), parsed.call.as_deref()),
            (Some("ses_root"), Some("c1"))
        );
        assert_eq!(parsed.string("form.metadata.tool.id"), Some("call_1"));
        assert_eq!(parsed.string("form.metadata.missing"), None);
        assert_eq!(parsed.directory, "/p/fg");
    }

    #[rstest]
    #[case::other_version(|p: &mut Value| p["v"] = json!(2), Rejected::Version("2".to_owned()))]
    #[case::no_version(|p: &mut Value| { p.as_object_mut().unwrap().remove("v"); }, Rejected::Version("null".to_owned()))]
    #[case::text_version(|p: &mut Value| p["v"] = json!("1"), Rejected::Version("\"1\"".to_owned()))]
    #[case::unknown_name(
        |p: &mut Value| p["name"] = json!("session.prompt"),
        Rejected::Unknown { kind: "event".to_owned(), name: "session.prompt".to_owned() }
    )]
    #[case::hook_as_event(
        |p: &mut Value| p["kind"] = json!("hook"),
        Rejected::Unknown { kind: "hook".to_owned(), name: "session.execution.started".to_owned() }
    )]
    #[case::no_session(
        |p: &mut Value| p["data"] = json!({}),
        Rejected::Missing { name: "session.execution.started", field: "sessionID" }
    )]
    #[case::empty_session(
        |p: &mut Value| p["data"]["sessionID"] = json!(""),
        Rejected::Missing { name: "session.execution.started", field: "sessionID" }
    )]
    #[case::number_session(
        |p: &mut Value| p["data"]["sessionID"] = json!(7),
        Rejected::Missing { name: "session.execution.started", field: "sessionID" }
    )]
    #[case::relative_directory(|p: &mut Value| p["directory"] = json!("fg"), Rejected::RelativeDirectory)]
    fn parse__should_reject_what_it_cannot_read(
        #[case] edit: fn(&mut Value),
        #[case] expected: Rejected,
    ) {
        // Given
        let mut payload = started();
        edit(&mut payload);

        // When
        let rejected = parse(&payload).unwrap_err();

        // Then
        assert_eq!(rejected, expected);
    }

    #[rstest]
    #[case::not_json(b"{".as_slice())]
    #[case::array(b"[]".as_slice())]
    #[case::no_data(br#"{"v":1,"kind":"event","name":"session.deleted","directory":"/p"}"#.as_slice())]
    #[case::data_not_an_object(br#"{"v":1,"kind":"event","name":"session.deleted","directory":"/p","data":[]}"#.as_slice())]
    #[case::parent_not_text(br#"{"v":1,"kind":"event","name":"session.deleted","directory":"/p","parent":1,"data":{"sessionID":"s"}}"#.as_slice())]
    #[case::trailing(br#"{"v":1,"kind":"event","name":"session.deleted","directory":"/p","data":{"sessionID":"s"}} x"#.as_slice())]
    fn parse__should_reject_another_shape(#[case] input: &[u8]) {
        // When
        let rejected = Payload::parse(input);

        // Then
        assert!(
            matches!(rejected, Err(Rejected::Malformed(_))),
            "{rejected:?}"
        );
    }

    #[test]
    fn parse__should_need_the_fields_of_its_entry() {
        // Given
        let payload = json!({
            "v": 1, "kind": "hook", "name": "tool.execute.before", "directory": "/p",
            "data": {"sessionID": "ses_1", "tool": "read"},
        });

        // When
        let rejected = parse(&payload).unwrap_err();

        // Then
        assert_eq!(
            rejected.to_string(),
            "the tool.execute.before payload lacks id"
        );
    }

    #[rstest]
    #[case::malformed(Rejected::Malformed("x".to_owned()), "the payload is malformed (x)")]
    #[case::version(Rejected::Version("2".to_owned()), "the payload has version 2")]
    #[case::unknown(
        Rejected::Unknown { kind: "event".to_owned(), name: "x\ny".to_owned() },
        "the event \"x\\ny\" is not in the event table"
    )]
    #[case::relative(Rejected::RelativeDirectory, "the payload's directory is relative")]
    fn rejected__should_say_why(#[case] rejected: Rejected, #[case] said: &str) {
        // When
        let text = rejected.to_string();

        // Then
        assert_eq!(text, said);
    }

    #[test]
    fn parse__should_drop_an_empty_parent_or_call() {
        // Given
        let mut payload = started();
        payload["parent"] = json!("");
        payload["call"] = json!("");

        // When
        let parsed = parse(&payload).unwrap();

        // Then
        assert_eq!((parsed.parent, parsed.call), (None, None));
    }
}
