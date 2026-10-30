//! The JSON Schema of [`Document`], generated from the document types and printed by
//! `wtpool status --schema`. The VS Code extension generates its types and validator from the
//! same text, printed by `cargo run -q -p wtpool-core --example status_schema`.
//!
//! Every property has a description. No object forbids other properties: within version 1 a
//! document only gains properties, and consumers ignore the ones they do not know.

mod jobs;

use serde::Serialize;
use serde_json::{Map, Value, json};

use super::document::{
    Activity, Actor, ActorKind, ActorState, Agents, Build, Document, GitStatus, OperationName,
    Output, PoolStatus, Reason, Repository, WorktreeStatus,
};
use super::record_time::RecordTime;
use crate::activity::{PlacedBy, WaitReason};
use crate::clock::Timestamp;
use crate::worktree_state::Role;

/// A type whose JSON form a JSON Schema describes.
pub trait ToSchema {
    /// The schema of one value. It may refer to the `#/$defs` of [`status_schema`].
    fn to_schema() -> Value;
}

const ROLES: [Role; 3] = [Role::Foreground, Role::Slot, Role::Unmanaged];

/// The schema of the status document, as pretty printed JSON with a final newline. Its keys are
/// sorted, so the text is the same on every run.
#[must_use]
pub fn status_schema_text() -> String {
    let mut text = serde_json::to_string_pretty(&status_schema()).unwrap_or_default();
    text.push('\n');
    text
}

/// The schema of the status document.
#[must_use]
pub fn status_schema() -> Value {
    let mut schema = Document::to_schema();
    if let Value::Object(map) = &mut schema {
        map.insert(
            "$schema".to_owned(),
            json!("https://json-schema.org/draft/2020-12/schema"),
        );
        map.insert(
            "$id".to_owned(),
            json!("https://wtpool.invalid/status.schema.json"),
        );
        map.insert("title".to_owned(), json!("wtpool status document"));
        map.insert(
            "description".to_owned(),
            json!(
                "One line of `wtpool status --json`, or a line of `wtpool status --watch` with type \
                 status. Generated from the types of wtpool-core and printed by `wtpool status --schema`. \
                 Within version 1 documents only gain properties; ignore the ones you do not know."
            ),
        );
        map.insert("$defs".to_owned(), definitions());
    }
    schema
}

/// The definitions the schemas refer to by `#/$defs/<name>`.
fn definitions() -> Value {
    let mut definitions = json!({
        "time": {
            "description": "An RFC 3339 time in UTC.",
            "type": "string",
            "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\\.[0-9]{1,9})?Z$",
        },
        "count": {
            "description": "A count, null when git could not tell.",
            "type": ["integer", "null"],
            "minimum": 0,
        },
        "pool": PoolStatus::to_schema(),
        "worktree": WorktreeStatus::to_schema(),
        "git": GitStatus::to_schema(),
        "actor": Actor::to_schema(),
        "build": Build::to_schema(),
        "output": Output::to_schema(),
    })
    .as_object_mut()
    .map_or_else(Map::new, std::mem::take);
    for (name, schema) in jobs::definitions() {
        definitions.insert(name.to_owned(), schema);
    }
    Value::Object(definitions)
}

/// An object that requires these properties, each given with its schema and description, and
/// allows others.
fn object(properties: Vec<(&str, Value, &str)>) -> Value {
    let required: Vec<&str> = properties.iter().map(|(name, _, _)| *name).collect();
    let properties: Map<String, Value> = properties
        .into_iter()
        .map(|(name, schema, description)| (name.to_owned(), described(schema, description)))
        .collect();
    json!({ "type": "object", "required": required, "properties": properties })
}

/// [`object`] with one more property that it does not require: a property wtpool omits when it
/// is empty, such as `outputs`, or one an older wtpool lacks.
fn with_optional(mut object: Value, name: &str, schema: Value, description: &str) -> Value {
    if let Some(Value::Object(properties)) = object.get_mut("properties") {
        properties.insert(name.to_owned(), described(schema, description));
    }
    object
}

fn described(mut schema: Value, description: &str) -> Value {
    if let Value::Object(map) = &mut schema {
        map.insert("description".to_owned(), json!(description));
    }
    schema
}

fn reference(name: &str) -> Value {
    json!({ "$ref": format!("#/$defs/{name}") })
}

fn array_of(items: Value) -> Value {
    json!({ "type": "array", "items": items })
}

fn text() -> Value {
    json!({ "type": "string", "minLength": 1 })
}

/// The JSON forms of `values`, followed by `null` when `nullable`.
fn one_of_values<T: Serialize>(values: &[T], nullable: bool) -> Value {
    let mut names: Vec<Value> = values
        .iter()
        .filter_map(|value| serde_json::to_value(value).ok())
        .collect();
    if nullable {
        names.push(Value::Null);
    }
    json!({ "enum": names })
}

fn nullable_time() -> Value {
    json!({ "oneOf": [RecordTime::to_schema(), { "type": "null" }] })
}

impl ToSchema for Timestamp {
    fn to_schema() -> Value {
        reference("time")
    }
}

impl ToSchema for RecordTime {
    fn to_schema() -> Value {
        reference("time")
    }
}

impl ToSchema for Document {
    fn to_schema() -> Value {
        let machine = with_optional(
            object(vec![(
                "name",
                text(),
                "The machine's host name, `localhost` when it has none.",
            )]),
            "jobs",
            reference("jobs"),
            "This user's heavy jobs on the machine; absent without a folder for their settings.",
        );
        object(vec![
            ("type", json!({ "const": "status" }), "Always `status`."),
            (
                "version",
                json!({ "const": 1 }),
                "The document version, bumped only by a removal or a rename.",
            ),
            (
                "generated",
                Timestamp::to_schema(),
                "When wtpool made the document.",
            ),
            ("machine", machine, "The machine wtpool ran on."),
            (
                "pools",
                json!({ "type": "array", "minItems": 1, "items": reference("pool") }),
                "The pools of the document; `wtpool status` prints one.",
            ),
        ])
    }
}

impl ToSchema for PoolStatus {
    fn to_schema() -> Value {
        object(vec![
            (
                "common_dir",
                text(),
                "The repository's common git directory.",
            ),
            (
                "agents",
                one_of_values(&Agents::ALL, false),
                "`on` when `wtpool-activity/` exists in the common git directory, so agent hooks record activity (S8).",
            ),
            (
                "skipped_records",
                json!({ "type": "integer", "minimum": 0 }),
                "Activity records skipped for an unknown version or a parse error (S6).",
            ),
            (
                "worktrees",
                array_of(reference("worktree")),
                "Every worktree of the repository, in `git worktree list` order.",
            ),
            (
                "repository",
                Repository::to_schema(),
                "The repository of the pool (S10).",
            ),
        ])
    }
}

impl ToSchema for Repository {
    fn to_schema() -> Value {
        object(vec![
            ("name", text(), "The folder name of the main worktree."),
            (
                "github",
                json!({ "type": ["string", "null"] }),
                "`<owner>/<repo>`, read offline from the remote's URL; null when that is no github.com URL.",
            ),
        ])
    }
}

impl ToSchema for WorktreeStatus {
    fn to_schema() -> Value {
        let mut reasons = array_of(one_of_values(&Reason::ALL, false));
        if let Value::Object(map) = &mut reasons {
            map.insert("uniqueItems".to_owned(), Value::Bool(true));
        }
        object(vec![
            ("path", text(), "The worktree's folder, as git lists it."),
            ("name", text(), "The folder's name."),
            (
                "role",
                one_of_values(&ROLES, false),
                "`foreground`, a `slot` of the pool, or `unmanaged` (S3).",
            ),
            (
                "missing",
                json!({ "type": "boolean" }),
                "Whether the folder is gone (S3).",
            ),
            (
                "branch",
                json!({ "type": ["string", "null"] }),
                "The branch checked out, null when detached or unknown.",
            ),
            (
                "upstream",
                json!({ "type": ["string", "null"] }),
                "The branch's upstream, null without one.",
            ),
            ("git", reference("git"), "What git says of the worktree."),
            (
                "free",
                json!({ "type": "boolean" }),
                "Whether `wtpool bg` would take the worktree (S9).",
            ),
            (
                "reasons",
                reasons,
                "Why `wtpool bg` would not take it, in this order (S9).",
            ),
            (
                "activity",
                one_of_values(&Activity::ALL, false),
                "What the worktree is doing or, without activity, whether it is available (A4, A5).",
            ),
            (
                "actors",
                array_of(reference("actor")),
                "The agent sessions and subagents shown in the worktree, newest event first.",
            ),
            (
                "builds",
                array_of(reference("build")),
                "The build processes working in the worktree (S4).",
            ),
            (
                "last_event",
                nullable_time(),
                "The newest `updated` of any record placed in the worktree, shown or not (A6).",
            ),
            (
                "last_change",
                nullable_time(),
                "The last change of the worktree's `HEAD` or index (A6).",
            ),
        ])
    }
}

impl ToSchema for GitStatus {
    fn to_schema() -> Value {
        object(vec![
            (
                "dirty",
                reference("count"),
                "Files `git status` lists as changed or untracked.",
            ),
            (
                "ahead",
                reference("count"),
                "Commits ahead of the upstream.",
            ),
            ("behind", reference("count"), "Commits behind the upstream."),
            (
                "unreferenced",
                reference("count"),
                "Commits on a detached HEAD that no branch, tag, remote branch or wtpool backup holds.",
            ),
            (
                "operation",
                one_of_values(&OperationName::ALL, true),
                "A git operation stopped half way, null when there is none.",
            ),
            (
                "unknown",
                json!({ "type": "boolean" }),
                "Whether git could not tell some of these facts.",
            ),
        ])
    }
}

impl ToSchema for Actor {
    fn to_schema() -> Value {
        let action = object(vec![
            ("tool", text(), "The tool, as the agent harness names it."),
            (
                "target",
                json!({ "type": "string" }),
                "What the tool works on: a path in the worktree or a command's summary (H14).",
            ),
        ]);
        let actor = object(vec![
            (
                "key",
                text(),
                "The record's file name without `.json`, unique in the pool.",
            ),
            (
                "kind",
                one_of_values(&ActorKind::ALL, false),
                "A `session`, or a `subagent` of one.",
            ),
            (
                "label",
                json!({ "type": "string" }),
                "The session title, `session HH:MM`, or the subagent's type (H16).",
            ),
            (
                "state",
                one_of_values(&ActorState::ALL, false),
                "The state shown (A1, A2).",
            ),
            (
                "reason",
                one_of_values(&WaitReason::ALL, true),
                "What a waiting actor waits for, else null.",
            ),
            (
                "action",
                json!({ "oneOf": [action, { "type": "null" }] }),
                "The tool the actor runs or waits on, and its target; null before any (H14).",
            ),
            (
                "placed_by",
                one_of_values(&PlacedBy::ALL, false),
                "`cwd` when placed by its working folder, `path` when a path it used moved it (H10).",
            ),
            (
                "error",
                json!({ "type": ["string", "null"] }),
                "The error that ended the last turn, else null.",
            ),
            (
                "since",
                RecordTime::to_schema(),
                "When the record's state last changed.",
            ),
            (
                "updated",
                RecordTime::to_schema(),
                "When the record's last event came.",
            ),
            (
                "harness",
                text(),
                "The coding agent harness whose hooks wrote the record, such as `claude-code` (S11).",
            ),
        ]);
        let actor = with_optional(
            actor,
            "model",
            text(),
            "The newest model the actor used, as its harness names it, such as `claude-opus-5-5`; absent when unknown (S11, H32).",
        );
        let actor = with_optional(
            actor,
            "launched_by",
            text(),
            "The `key` of the session of another agent whose harness process this session's harness process descends from, read from the process table; absent without one (S17).",
        );
        with_optional(
            actor,
            "outputs",
            array_of(reference("output")),
            "The actor's running background tasks, newest first, then the one that ended last; absent when it has none (S13).",
        )
    }
}

impl ToSchema for Output {
    fn to_schema() -> Value {
        object(vec![
            (
                "task",
                text(),
                "The task's id, as the agent harness names it.",
            ),
            (
                "path",
                text(),
                "The absolute path of the file the harness writes the task's output to, on this machine; wtpool names it and never reads it (H27).",
            ),
            (
                "summary",
                json!({ "type": "string" }),
                "The command's summary: its program and one subcommand (H14).",
            ),
            (
                "running",
                json!({ "type": "boolean" }),
                "Whether the task still runs, by the harness's own list of tasks in flight (H27).",
            ),
            ("started", RecordTime::to_schema(), "When the task started."),
            (
                "ended",
                nullable_time(),
                "When wtpool learned that the task ended (H27); null while it runs.",
            ),
        ])
    }
}

impl ToSchema for Build {
    fn to_schema() -> Value {
        let build = object(vec![
            (
                "pid",
                json!({ "type": "integer", "minimum": 1 }),
                "The build process's id.",
            ),
            (
                "name",
                text(),
                "The words of the heavy pattern the build process matches.",
            ),
        ]);
        with_optional(
            build,
            "queued",
            json!({ "type": "boolean" }),
            "Whether a live job's process is the build or one of its ancestors, also true for a build only a suggestion names.",
        )
    }
}

#[cfg(test)]
mod tests;
