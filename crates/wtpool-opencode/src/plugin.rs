//! The plugin `wtpool hook install --harness opencode` prints: the source in
//! `plugin/wtpool.ts` with its one `CONFIG` line holding this wtpool's path and the event table.

use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};
use serde_json::Value;

use crate::ids;
use crate::table::{Entry, Role, Source, TABLE};
use crate::tools;

/// The plugin's source as committed, its `CONFIG` line rendered for [`SAMPLE_BIN`].
pub const TEMPLATE: &str = include_str!("../plugin/wtpool.ts");

/// The `bin` of the committed `CONFIG` line.
pub const SAMPLE_BIN: &str = "/usr/local/bin/wtpool";

/// The start of the `CONFIG` line; the JSON and a `;` end it.
const CONFIG_START: &str = "const CONFIG: Config = ";

/// The sessions an instance remembers, the least recently seen dropped first.
pub const SESSIONS: usize = 1024;
/// The open permissions, and apart from them the open forms, an instance remembers.
pub const ASKS: usize = 256;
/// The shell calls an instance remembers until their shell starts.
pub const SHELLS: usize = 64;
/// The time a shell call waits for its shell before the instance forgets it.
pub const SHELL_MS: u64 = 10_000;
/// The least time between two streaming events the instance sends for one session.
pub const PROGRESS_MS: u64 = 10_000;
/// The time a shell call waits for `wtpool hook policy`, as Claude Code's policy entry does.
pub const POLICY_MS: u64 = 2_000;
/// The largest payload the instance sends.
pub const BYTES: usize = 1024 * 1024;

/// The plugin's source for `binary`, with the shell call policy on when `policy` is set.
#[must_use]
pub fn render(binary: &str, policy: bool) -> String {
    let config = serde_json::to_string(&Config::new(binary, policy)).unwrap_or_default();
    let mut text = String::with_capacity(TEMPLATE.len() + config.len());
    for line in TEMPLATE.split_inclusive('\n') {
        if line.starts_with(CONFIG_START) {
            text.push_str(CONFIG_START);
            text.push_str(&config);
            text.push_str(";\n");
        } else {
            text.push_str(line);
        }
    }
    text
}

/// The `bin` and `policy` of the `CONFIG` line of a plugin's `source`, `None` when it holds no
/// such line.
#[must_use]
pub fn config_of(source: &str) -> Option<(String, bool)> {
    let line = source
        .lines()
        .find_map(|line| line.strip_prefix(CONFIG_START))?;
    let config: Value = serde_json::from_str(line.strip_suffix(';')?).ok()?;
    Some((
        config.get("bin")?.as_str()?.to_owned(),
        config.get("policy")?.as_bool()?,
    ))
}

#[derive(Serialize)]
struct Config<'a> {
    bin: &'a str,
    policy: bool,
    shell: ShellCall,
    ids: IdForm,
    hooks: Entries,
    events: Entries,
    lookups: Entries,
    limits: Limits,
}

impl<'a> Config<'a> {
    fn new(bin: &'a str, policy: bool) -> Self {
        Self {
            bin,
            policy,
            shell: ShellCall {
                tool: tools::SHELL,
                command: tools::COMMAND,
            },
            ids: IdForm {
                prefix: ids::STEM_PREFIX,
                letters: ids::LETTERS,
                max: ids::MAX_LEN,
            },
            hooks: Entries(Source::Hook),
            events: Entries(Source::Event),
            lookups: Entries(Source::Lookup),
            limits: Limits {
                sessions: SESSIONS,
                asks: ASKS,
                shells: SHELLS,
                shell_ms: SHELL_MS,
                progress_ms: PROGRESS_MS,
                policy_ms: POLICY_MS,
                bytes: BYTES,
            },
        }
    }
}

/// The tool whose calls start a shell, and the field of its command.
#[derive(Serialize)]
struct ShellCall {
    tool: &'static str,
    command: &'static str,
}

/// OpenCode's own form of an id, and the stem prefix of its readable form.
#[derive(Serialize)]
struct IdForm {
    prefix: &'static str,
    letters: usize,
    max: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Limits {
    sessions: usize,
    asks: usize,
    shells: usize,
    shell_ms: u64,
    progress_ms: u64,
    policy_ms: u64,
    bytes: usize,
}

/// The entries of [`TABLE`] from one [`Source`], by name in table order.
struct Entries(Source);

impl Serialize for Entries {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(
            TABLE
                .iter()
                .filter(|entry| entry.source == self.0)
                .map(|entry| (entry.name, Forwarded(entry))),
        )
    }
}

/// One entry as the plugin reads it: its kept fields, its session field and its role, and for a
/// lookup the fields it is made for, its `needs` but the session.
struct Forwarded<'a>(&'a Entry);

impl Serialize for Forwarded<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let entry = self.0;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("keep", entry.keep)?;
        map.serialize_entry("session", entry.session)?;
        match entry.role {
            Role::Plain => {}
            Role::Parent(field) => map.serialize_entry("parent", field)?,
            Role::Ask {
                map: name,
                id,
                call,
            } => {
                let ask = Ask {
                    map: name,
                    id,
                    call,
                };
                map.serialize_entry("ask", &ask)?;
            }
            Role::Reply { map: name, id } => {
                map.serialize_entry("reply", &Reply { map: name, id })?;
            }
            Role::Throttle => map.serialize_entry("throttle", &true)?,
            Role::Changes(fields) => map.serialize_entry("changes", fields)?,
        }
        if entry.source == Source::Lookup {
            let wanted: Vec<&str> = entry
                .needs
                .iter()
                .copied()
                .filter(|field| *field != entry.session)
                .collect();
            map.serialize_entry("needs", &wanted)?;
        }
        map.end()
    }
}

#[derive(Serialize)]
struct Ask {
    map: &'static str,
    id: &'static str,
    call: &'static str,
}

#[derive(Serialize)]
struct Reply {
    map: &'static str,
    id: &'static str,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::*;

    fn config_json(source: &str) -> Value {
        let line = source
            .lines()
            .find_map(|line| line.strip_prefix(CONFIG_START))
            .unwrap();
        serde_json::from_str(line.strip_suffix(';').unwrap()).unwrap()
    }

    #[test]
    fn template__should_hold_the_config_rendered_for_the_sample_binary() {
        // Given
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("plugin/wtpool.ts");
        let mismatch = format!(
            "{} is not what the event table makes. Write it with cargo run -q -p \
             wtpool-opencode --example plugin > {}",
            path.display(),
            path.display()
        );

        // When
        let rendered = render(SAMPLE_BIN, false);

        // Then
        assert!(rendered == TEMPLATE, "{mismatch}");
    }

    #[test]
    fn render__should_change_only_the_config_line() {
        // When
        let rendered = render("/opt/w/wtpool", true);

        // Then
        let changed: Vec<(&str, &str)> = rendered
            .lines()
            .zip(TEMPLATE.lines())
            .filter(|(new, old)| new != old)
            .collect();
        assert_eq!(changed.len(), 1);
        assert_eq!(rendered.lines().count(), TEMPLATE.lines().count());
        assert_eq!(
            config_of(&rendered),
            Some(("/opt/w/wtpool".to_owned(), true))
        );
    }

    #[test]
    fn render__should_write_the_table_in_its_order() {
        // When
        let config = config_json(&render("/b/wtpool", false));

        // Then
        assert_eq!(
            config["hooks"],
            json!({"tool.execute.before": {
                "keep": ["sessionID", "id", "tool", "input.path", "input.command", "input.agent"],
                "session": "sessionID",
            }})
        );
        let line = render("/b/wtpool", false);
        let positions: Vec<usize> = TABLE
            .iter()
            .filter(|entry| entry.source == Source::Event)
            .map(|entry| line.find(&format!("\"{}\":{{", entry.name)).unwrap())
            .collect();
        assert!(positions.is_sorted(), "{positions:?}");
        assert_eq!(config["events"].as_object().unwrap().len(), TABLE.len() - 2);
        assert_eq!(
            config["lookups"],
            json!({"session.get": {
                "keep": ["id", "parentID", "title"],
                "session": "id",
                "parent": "parentID",
                "needs": ["title"],
            }})
        );
    }

    #[test]
    fn render__should_give_each_role_its_key() {
        // When
        let config = config_json(&render("/b/wtpool", false));

        // Then
        let events = &config["events"];
        assert_eq!(events["session.created"]["parent"], "parentID");
        assert_eq!(
            events["permission.asked"]["ask"],
            json!({"map": "permission", "id": "id", "call": "source.id"})
        );
        assert_eq!(
            events["form.created"]["ask"],
            json!({"map": "form", "id": "form.id", "call": "form.metadata.tool.id"})
        );
        assert_eq!(
            events["permission.replied"]["reply"],
            json!({"map": "permission", "id": "requestID"})
        );
        assert_eq!(
            events["form.replied"]["reply"],
            json!({"map": "form", "id": "id"})
        );
        assert_eq!(events["session.text.delta"]["throttle"], true);
        assert_eq!(
            events["session.step.started"]["changes"],
            json!(["model.id", "model.providerID"])
        );
        assert_eq!(
            events["session.deleted"],
            json!({"keep": ["sessionID"], "session": "sessionID"})
        );
        assert_eq!(events["form.created"]["session"], "form.sessionID");
    }

    #[test]
    fn render__should_write_the_shell_ids_and_limits() {
        // When
        let config = config_json(&render("/b/wtpool", true));

        // Then
        assert_eq!(config["bin"], "/b/wtpool");
        assert_eq!(config["policy"], true);
        assert_eq!(
            config["shell"],
            json!({"tool": "shell", "command": "input.command"})
        );
        assert_eq!(
            config["ids"],
            json!({"prefix": "opencode-", "letters": 8, "max": 64})
        );
        assert_eq!(
            config["limits"],
            json!({
                "sessions": 1024, "asks": 256, "shells": 64, "shellMs": 10000,
                "progressMs": 10000, "policyMs": 2000, "bytes": 1_048_576,
            })
        );
        let keys: Vec<&String> = config.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 8);
    }

    #[test]
    fn render__should_escape_the_binary_as_json() {
        // When
        let rendered = render("/p/with \"quote\"/wtpool", false);

        // Then
        assert_eq!(
            config_of(&rendered).map(|(bin, _)| bin),
            Some("/p/with \"quote\"/wtpool".to_owned())
        );
    }

    #[test]
    fn config_of__should_need_the_config_line() {
        // When
        let found = (
            config_of("const x = 1;\n"),
            config_of("const CONFIG: Config = {\"bin\":1};\n"),
            config_of("const CONFIG: Config = {\"bin\":\"/b\",\"policy\":true}\n"),
        );

        // Then
        assert_eq!(found, (None, None, None));
    }
}
