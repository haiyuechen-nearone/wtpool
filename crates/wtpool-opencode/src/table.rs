//! The OpenCode hooks, events and lookups that wtpool reads, as one table. The plugin's
//! configuration ([`crate::plugin`]) and the reader of its payloads ([`crate::events`]) both come
//! from it, so the plugin never forwards a field the reader does not know.

/// The way OpenCode hands a plugin an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// An event of `ctx.event.subscribe()`, its fields under `data`.
    Event,
    /// A hook the plugin registers, its fields in the object OpenCode passes.
    Hook,
    /// The plugin's read of a session it first saw without its title, its fields in the answer
    /// of `ctx.session.get`.
    Lookup,
}

impl Source {
    /// The payload's `kind`.
    #[must_use]
    pub fn kind(self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::Hook => "hook",
            Self::Lookup => "lookup",
        }
    }
}

/// The work the plugin does for an entry beyond keeping its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Plain,
    /// The field names the parent of a child session; the plugin maps the child to its root.
    Parent(&'static str),
    /// A request the user answers later: the plugin keeps, in its map `map`, the request `id`
    /// with the `call` it holds.
    Ask {
        map: &'static str,
        id: &'static str,
        call: &'static str,
    },
    /// The answer to the request `id` of the map `map`: the plugin attaches that request's call.
    Reply {
        map: &'static str,
        id: &'static str,
    },
    /// Sent at most once per session per [`crate::plugin::PROGRESS_MS`].
    Throttle,
    /// Sent only when these fields differ from the ones last sent for the session.
    Changes(&'static [&'static str]),
}

/// The neutral event an entry gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meaning {
    /// A session starts, or with a parent a subagent does.
    Created,
    /// A turn starts.
    Started,
    /// A turn ends, done or interrupted.
    Ended,
    /// A turn ends with an error.
    Failed,
    CallStart,
    CallEnd,
    /// The call waits for the user's permission.
    PermissionAsked,
    /// The user answered a permission or a form.
    Replied,
    /// The call waits for the user's answer to a form.
    FormCreated,
    /// The session's title changed, or a lookup found it.
    Renamed,
    /// A step names its model.
    Step,
    /// The model streams.
    Progress,
    /// The session is gone.
    Deleted,
}

/// One forwarded hook or event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub source: Source,
    /// OpenCode's name of the hook or event.
    pub name: &'static str,
    /// The fields the plugin keeps, as dotted paths. Everything else stays in OpenCode.
    pub keep: &'static [&'static str],
    /// The kept field that names the session.
    pub session: &'static str,
    /// The kept fields the entry cannot do without.
    pub needs: &'static [&'static str],
    pub role: Role,
    pub meaning: Meaning,
}

const SESSION: &str = "sessionID";
const PERMISSIONS: &str = "permission";
const FORMS: &str = "form";

/// An event with the session's id alone, plus `keep`.
const fn event(name: &'static str, keep: &'static [&'static str], meaning: Meaning) -> Entry {
    Entry {
        source: Source::Event,
        name,
        keep,
        session: SESSION,
        needs: &[SESSION],
        role: Role::Plain,
        meaning,
    }
}

const fn with_role(entry: Entry, role: Role) -> Entry {
    Entry { role, ..entry }
}

const fn with_needs(entry: Entry, needs: &'static [&'static str]) -> Entry {
    Entry { needs, ..entry }
}

/// Every hook, event and lookup the plugin forwards, in the order its configuration lists them.
pub const TABLE: [Entry; 19] = [
    with_role(
        event(
            "session.created",
            &[SESSION, "parentID", "title", "agent"],
            Meaning::Created,
        ),
        Role::Parent("parentID"),
    ),
    event("session.execution.started", &[SESSION], Meaning::Started),
    event(
        "session.execution.succeeded",
        &[SESSION, "reason"],
        Meaning::Ended,
    ),
    event(
        "session.execution.interrupted",
        &[SESSION, "reason"],
        Meaning::Ended,
    ),
    event(
        "session.execution.failed",
        &[SESSION, "error.type"],
        Meaning::Failed,
    ),
    Entry {
        source: Source::Hook,
        name: "tool.execute.before",
        keep: &[
            SESSION,
            "id",
            "tool",
            "input.path",
            "input.command",
            "input.agent",
        ],
        session: SESSION,
        needs: &[SESSION, "id", "tool"],
        role: Role::Plain,
        meaning: Meaning::CallStart,
    },
    with_needs(
        event("session.tool.success", &[SESSION, "id"], Meaning::CallEnd),
        &[SESSION, "id"],
    ),
    with_needs(
        event("session.tool.failed", &[SESSION, "id"], Meaning::CallEnd),
        &[SESSION, "id"],
    ),
    with_role(
        event(
            "permission.asked",
            &[SESSION, "id", "source.id"],
            Meaning::PermissionAsked,
        ),
        Role::Ask {
            map: PERMISSIONS,
            id: "id",
            call: "source.id",
        },
    ),
    with_role(
        event(
            "permission.replied",
            &[SESSION, "requestID", "reply"],
            Meaning::Replied,
        ),
        Role::Reply {
            map: PERMISSIONS,
            id: "requestID",
        },
    ),
    Entry {
        source: Source::Event,
        name: "form.created",
        keep: &[
            "form.id",
            "form.sessionID",
            "form.metadata.kind",
            "form.metadata.tool.id",
        ],
        session: "form.sessionID",
        needs: &["form.sessionID"],
        role: Role::Ask {
            map: FORMS,
            id: "form.id",
            call: "form.metadata.tool.id",
        },
        meaning: Meaning::FormCreated,
    },
    with_role(
        event("form.replied", &["id", SESSION], Meaning::Replied),
        Role::Reply {
            map: FORMS,
            id: "id",
        },
    ),
    event("session.renamed", &[SESSION, "title"], Meaning::Renamed),
    Entry {
        source: Source::Lookup,
        name: "session.get",
        keep: &["id", "parentID", "title"],
        session: "id",
        needs: &["id", "title"],
        role: Role::Parent("parentID"),
        meaning: Meaning::Renamed,
    },
    with_role(
        with_needs(
            event(
                "session.step.started",
                &[SESSION, "model.id", "model.providerID"],
                Meaning::Step,
            ),
            &[SESSION, "model.id", "model.providerID"],
        ),
        Role::Changes(&["model.id", "model.providerID"]),
    ),
    with_role(
        event("session.text.delta", &[SESSION], Meaning::Progress),
        Role::Throttle,
    ),
    with_role(
        event("session.reasoning.delta", &[SESSION], Meaning::Progress),
        Role::Throttle,
    ),
    with_role(
        event("session.tool.input.delta", &[SESSION], Meaning::Progress),
        Role::Throttle,
    ),
    event("session.deleted", &[SESSION], Meaning::Deleted),
];

/// The entry of `kind` (`event` or `hook`) called `name`, `None` for anything the plugin never
/// forwards.
#[must_use]
pub fn entry(kind: &str, name: &str) -> Option<&'static Entry> {
    TABLE
        .iter()
        .find(|entry| entry.source.kind() == kind && entry.name == name)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use rstest::rstest;

    use super::*;

    #[test]
    fn table__should_name_each_entry_once_and_keep_what_it_needs() {
        // When
        let names: HashSet<(&str, &str)> = TABLE
            .iter()
            .map(|entry| (entry.source.kind(), entry.name))
            .collect();

        // Then
        assert_eq!(names.len(), TABLE.len());
        for entry in TABLE {
            assert!(entry.keep.contains(&entry.session), "{}", entry.name);
            assert!(entry.needs.contains(&entry.session), "{}", entry.name);
            assert!(
                entry.needs.iter().all(|field| entry.keep.contains(field)),
                "{}",
                entry.name
            );
        }
    }

    #[test]
    fn table__should_keep_no_text_output_or_environment() {
        // Given
        let never = [
            "text", "delta", "content", "output", "env", "answer", "prompt", "headers",
        ];

        // When
        let kept: Vec<&str> = TABLE
            .iter()
            .flat_map(|entry| entry.keep.iter().copied())
            .collect();

        // Then
        for field in kept {
            let leaf = field.rsplit('.').next().unwrap_or(field);
            assert!(!never.contains(&leaf), "{field}");
        }
    }

    #[rstest]
    #[case::event("event", "session.created", Some(Meaning::Created))]
    #[case::hook("hook", "tool.execute.before", Some(Meaning::CallStart))]
    #[case::a_hook_named_as_an_event("event", "tool.execute.before", None)]
    #[case::after_the_call("hook", "tool.execute.after", None)]
    #[case::not_forwarded("event", "session.prompt", None)]
    #[case::lookup("lookup", "session.get", Some(Meaning::Renamed))]
    #[case::a_lookup_named_as_an_event("event", "session.get", None)]
    fn entry__should_find_only_forwarded_names(
        #[case] kind: &str,
        #[case] name: &str,
        #[case] meaning: Option<Meaning>,
    ) {
        // When
        let found = entry(kind, name);

        // Then
        assert_eq!(found.map(|entry| entry.meaning), meaning);
    }

    #[test]
    fn source__should_name_its_kind() {
        // When
        let kinds = (
            Source::Event.kind(),
            Source::Hook.kind(),
            Source::Lookup.kind(),
        );

        // Then
        assert_eq!(kinds, ("event", "hook", "lookup"));
    }

    #[test]
    fn event__should_build_an_event_of_the_session_alone() {
        // When
        let built = event("x.created", &["a.b"], Meaning::Deleted);
        let with = (
            with_role(built, Role::Parent("parentID")),
            with_needs(built, &["x"]),
        );

        // Then
        assert_eq!(
            (
                built.source,
                built.name,
                built.keep,
                built.session,
                built.needs
            ),
            (
                Source::Event,
                "x.created",
                &["a.b"][..],
                SESSION,
                &[SESSION][..]
            )
        );
        assert_eq!((built.role, built.meaning), (Role::Plain, Meaning::Deleted));
        assert_eq!(with.0.role, Role::Parent("parentID"));
        assert_eq!(with.0.meaning, Meaning::Deleted);
        assert_eq!(with.1.needs, &["x"][..]);
        assert_eq!(with.1.role, Role::Plain);
    }
}
