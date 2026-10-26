//! The schemas of `machine.jobs`: the definitions `jobs`, `heavy`, `suggestion` and `job`.

use serde_json::{Value, json};

use super::{array_of, object, one_of_values, reference, text};
use crate::heavy::PatternSource;
use crate::jobs::{ConfigSource, Enforce, LIMIT_BOUNDS, MAX_DEFAULT_LIMIT};

/// The definitions the schema of `machine.jobs` adds, by name.
pub(super) fn definitions() -> [(&'static str, Value); 4] {
    [
        ("jobs", jobs()),
        ("heavy", heavy()),
        ("suggestion", suggestion()),
        ("job", job()),
    ]
}

fn jobs() -> Value {
    let (min, max) = LIMIT_BOUNDS;
    let source = object(vec![
        (
            "limit",
            one_of_values(&ConfigSource::ALL, false),
            "The source of the limit.",
        ),
        (
            "enforce",
            one_of_values(&ConfigSource::ALL, false),
            "The source of the policy.",
        ),
    ]);
    let jobs_of = |description| (array_of(reference("job")), description);
    let (running, running_text) = jobs_of("The running jobs.");
    let (waiting, waiting_text) = jobs_of("The waiting jobs in queue order.");
    let (outside, outside_text) =
        jobs_of("The outermost heavy processes no job holds, newest first.");
    object(vec![
        (
            "limit",
            json!({ "type": ["integer", "null"], "minimum": min, "maximum": max }),
            "How many jobs run at once, null while error is set.",
        ),
        (
            "default",
            json!({ "type": "integer", "minimum": min, "maximum": MAX_DEFAULT_LIMIT }),
            "The limit without a setting: a quarter of the cores, kept from 1 to 8.",
        ),
        ("min", json!({ "const": min }), "The lowest limit."),
        ("max", json!({ "const": max }), "The highest limit."),
        (
            "enforce",
            one_of_values(&Enforce::ALL, false),
            "The policy for heavy commands outside a job.",
        ),
        (
            "source",
            source,
            "Where the limit and the policy each come from: `machine.toml` or the defaults.",
        ),
        (
            "config",
            text(),
            "The path of `machine.toml`, whether it exists or not.",
        ),
        (
            "error",
            json!({ "type": ["string", "null"] }),
            "Why the machine config or the job folder cannot be read, else null.",
        ),
        (
            "heavy",
            array_of(reference("heavy")),
            "Each heavy pattern with the list it comes from.",
        ),
        (
            "suggested",
            array_of(reference("suggestion")),
            "This repository's suggested patterns that the heavy list lacks.",
        ),
        ("running", running, running_text),
        ("waiting", waiting, waiting_text),
        ("outside", outside, outside_text),
    ])
}

fn heavy() -> Value {
    object(vec![
        ("pattern", text(), "The pattern, one to three words."),
        (
            "source",
            one_of_values(&PatternSource::ALL, false),
            "The list it comes from: the machine's, the repository's `heavy` or its `build_processes`.",
        ),
    ])
}

fn suggestion() -> Value {
    object(vec![
        ("pattern", text(), "The suggested pattern."),
        (
            "from",
            text(),
            "What suggested it, such as the marker `Cargo.toml` or `package.json`; an open string.",
        ),
    ])
}

fn job() -> Value {
    let nullable_text = || json!({ "type": ["string", "null"] });
    object(vec![
        (
            "id",
            text(),
            "The job's id, or an id of a heavy process outside any job.",
        ),
        (
            "summary",
            json!({ "type": "string" }),
            "The words of the heavy pattern it matches, else the summary of its arguments.",
        ),
        ("label", nullable_text(), "Its label, else null."),
        (
            "worktree",
            nullable_text(),
            "The path of the worktree of this pool its folder lies in, else null.",
        ),
        (
            "actor",
            nullable_text(),
            "The key of the actor of this pool it belongs to, else null.",
        ),
        (
            "since",
            reference("time"),
            "When it started, or for a waiting job when it queued.",
        ),
    ])
}
