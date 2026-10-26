//! `catalog__should_match_the_files`: the scenario files against the scenario catalog, read from
//! the harness's design document.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::results::CheckResult;

pub const NAME: &str = "catalog__should_match_the_files";
const DOC: &str = "docs/design/status-panel/verification.md";
const HEADING: &str = "## Scenario catalog";
/// Every scenario also covers these.
const ALWAYS_COVERED: [&str; 2] = ["R3", "U1"];
const ACTIVITIES: [&str; 10] = [
    "missing",
    "waiting",
    "working",
    "building",
    "idle",
    "foreground",
    "free",
    "not_reusable",
    "unknown",
    "unused",
];

/// One numbered entry of the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub phase: u8,
    /// The IDs in brackets plus those every scenario covers.
    pub covers: BTreeSet<String>,
    /// Each ``<worktree> `<activity>` `` the entry names, activity in snake case.
    pub rows: BTreeSet<(String, String)>,
}

fn is_worktree_name(word: &str) -> bool {
    word == "fg"
        || word == "x"
        || word
            .strip_prefix("wt")
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// The worktree and activity pairs of an entry's text after its bracket.
pub fn rows_of(text: &str) -> BTreeSet<(String, String)> {
    let mut rows = BTreeSet::new();
    let parts: Vec<&str> = text.split('`').collect();
    for (index, quoted) in parts.iter().enumerate().skip(1).step_by(2) {
        let Some(before) = index.checked_sub(1).and_then(|i| parts.get(i)) else {
            continue;
        };
        let Some(word) = before.split_whitespace().last() else {
            continue;
        };
        let activity = quoted.trim().replace(' ', "_");
        if is_worktree_name(word) && ACTIVITIES.contains(&activity.as_str()) {
            rows.insert((word.to_owned(), activity));
        }
    }
    rows
}

/// Reads one catalog line, `N. \`name\` (P: ID, ID): text`.
pub fn parse_entry(line: &str) -> Option<Entry> {
    let (number, rest) = line.trim_start().split_once(". `")?;
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (name, rest) = rest.split_once('`')?;
    let rest = rest.trim_start().strip_prefix('(')?;
    let (bracket, text) = rest.split_once("):")?;
    let (phase, ids) = bracket.split_once(':')?;
    let phase: u8 = phase.trim().parse().ok()?;
    let mut covers: BTreeSet<String> = ids
        .split(',')
        .map(|id| id.trim().to_owned())
        .filter(|id| !id.is_empty())
        .collect();
    covers.extend(ALWAYS_COVERED.iter().map(|id| (*id).to_owned()));
    Some(Entry {
        name: name.to_owned(),
        phase,
        covers,
        rows: rows_of(text),
    })
}

/// Every entry of the catalog section of `doc`.
pub fn entries(doc: &str) -> Vec<Entry> {
    let mut inside = false;
    let mut found = Vec::new();
    for line in doc.lines() {
        if line.starts_with("## ") {
            inside = line.trim() == HEADING;
            continue;
        }
        if inside && let Some(entry) = parse_entry(line) {
            found.push(entry);
        }
    }
    found
}

/// A scenario file as the check reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts {
    pub name: String,
    pub phase: u8,
    pub covers: BTreeSet<String>,
    /// Each worktree name and activity of every expected document.
    pub rows: BTreeSet<(String, String)>,
}

pub fn file_facts(name: &str, json: &serde_json::Value) -> FileFacts {
    let phase = json
        .get("phase")
        .and_then(serde_json::Value::as_u64)
        .and_then(|p| u8::try_from(p).ok())
        .unwrap_or(u8::MAX);
    let covers = json
        .get("covers")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|id| id.as_str().map(str::to_owned))
        .collect();
    let mut documents: Vec<&serde_json::Value> = json
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|step| step.get("expect"))
        .collect();
    documents.extend(json.get("expect"));
    let mut rows = BTreeSet::new();
    for document in documents {
        let worktrees = document
            .get("pools")
            .and_then(|pools| pools.get(0))
            .and_then(|pool| pool.get("worktrees"))
            .and_then(serde_json::Value::as_array);
        for worktree in worktrees.into_iter().flatten() {
            let name = worktree.get("name").and_then(serde_json::Value::as_str);
            let activity = worktree.get("activity").and_then(serde_json::Value::as_str);
            if let (Some(name), Some(activity)) = (name, activity) {
                rows.insert((name.to_owned(), activity.to_owned()));
            }
        }
    }
    FileFacts {
        name: name.to_owned(),
        phase,
        covers,
        rows,
    }
}

/// The faults of the files against the catalog. Each entry of phase `phase` or earlier needs a
/// file.
pub fn problems(catalog: &[Entry], files: &[FileFacts], phase: u8) -> Vec<String> {
    let mut found = Vec::new();
    for entry in catalog.iter().filter(|e| e.phase <= phase) {
        if !files.iter().any(|f| f.name == entry.name) {
            found.push(format!("missing: verify/scenarios/{}.json", entry.name));
        }
    }
    for file in files {
        let Some(entry) = catalog.iter().find(|e| e.name == file.name) else {
            found.push(format!(
                "extra: verify/scenarios/{}.json is not in the catalog",
                file.name
            ));
            continue;
        };
        if file.phase != entry.phase {
            found.push(format!(
                "{}: phase {} where the catalog says {}",
                file.name, file.phase, entry.phase
            ));
        }
        if file.covers != entry.covers {
            found.push(format!(
                "{}: covers {:?} where the catalog says {:?}",
                file.name, file.covers, entry.covers
            ));
        }
        for (worktree, activity) in entry.rows.difference(&file.rows) {
            found.push(format!(
                "{}: no expected document has {worktree} `{activity}`",
                file.name
            ));
        }
    }
    found
}

pub fn check(root: &Path, phase: u8) -> CheckResult {
    let doc = match fs::read_to_string(root.join(DOC)) {
        Ok(doc) => doc,
        Err(error) => return CheckResult::error(NAME, "static", format!("{DOC}: {error}")),
    };
    let catalog = entries(&doc);
    if catalog.is_empty() {
        return CheckResult::error(NAME, "static", format!("{DOC} has no catalog entries"));
    }
    let mut files = Vec::new();
    for name in crate::scenario::names(root) {
        let path = crate::scenario::path(root, &name);
        let parsed = fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                serde_json::from_str::<serde_json::Value>(&text).map_err(|e| e.to_string())
            });
        match parsed {
            Ok(json) => files.push(file_facts(&name, &json)),
            Err(error) => {
                return CheckResult::assertion(
                    NAME,
                    "static",
                    format!("{}: {error}", path.display()),
                );
            }
        }
    }
    let found = problems(&catalog, &files, phase);
    if found.is_empty() {
        CheckResult::pass(
            NAME,
            "static",
            format!("{} scenario files match the catalog", files.len()),
        )
    } else {
        CheckResult::assertion(NAME, "static", found.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn rows(items: &[(&str, &str)]) -> BTreeSet<(String, String)> {
        items
            .iter()
            .map(|(w, a)| ((*w).to_owned(), (*a).to_owned()))
            .collect()
    }

    #[rstest]
    #[case::four_rows(
        "1. `idle_pool` (1: R1, A5): fg `foreground`, wt1 `free`, wt2 `unused`, wt3 `not reusable`.",
        "idle_pool", 1, &["R1", "A5", "R3", "U1"][..],
        &[("fg", "foreground"), ("wt1", "free"), ("wt2", "unused"), ("wt3", "not_reusable")][..]
    )]
    #[case::then(
        "2. `building_without_agent` (1: S4): wt1 `building`, then wt1 `free`.",
        "building_without_agent", 1, &["S4", "R3", "U1"][..],
        &[("wt1", "building"), ("wt1", "free")][..]
    )]
    #[case::no_rows(
        "8. `agents_off` (1: S8): note \"agent activity is off\".",
        "agents_off", 1, &["S8", "R3", "U1"][..], &[][..]
    )]
    #[case::other_quotes(
        "17. `auto_mode_denied` (2: H12): PermissionDenied, a build, clock +180 s → nothing in flight, actor `quiet`, wt1 `building`.",
        "auto_mode_denied", 2, &["H12", "R3", "U1"][..], &[("wt1", "building")][..]
    )]
    fn parse_entry__should_read_name_phase_covers_and_rows(
        #[case] line: &str,
        #[case] name: &str,
        #[case] phase: u8,
        #[case] covers: &[&str],
        #[case] expected_rows: &[(&str, &str)],
    ) {
        // Given
        let text = line;

        // When
        let entry = parse_entry(text);

        // Then
        assert_eq!(
            entry,
            Some(Entry {
                name: name.to_owned(),
                phase,
                covers: set(covers),
                rows: rows(expected_rows),
            })
        );
    }

    #[test]
    fn entries__should_read_only_the_catalog_section() {
        // Given
        let doc = "## Layers\n\n1. `not_this` (1: R1): fg `free`.\n\n## Scenario catalog\n\nText.\n\n1. `idle_pool` (1: R1): fg `foreground`.\n2. `b` (2: H10): wt1 `working`.\n\n## Trace list\n\n3. `nor_this` (1: R2): x `unused`.\n";

        // When
        let names: Vec<String> = entries(doc).into_iter().map(|e| e.name).collect();

        // Then
        assert_eq!(names, vec!["idle_pool".to_owned(), "b".to_owned()]);
    }

    fn entry(name: &str, phase: u8, covers: &[&str], entry_rows: &[(&str, &str)]) -> Entry {
        Entry {
            name: name.to_owned(),
            phase,
            covers: set(covers),
            rows: rows(entry_rows),
        }
    }

    fn file(name: &str, phase: u8, covers: &[&str], file_rows: &[(&str, &str)]) -> FileFacts {
        FileFacts {
            name: name.to_owned(),
            phase,
            covers: set(covers),
            rows: rows(file_rows),
        }
    }

    #[rstest]
    #[case::matching(vec![file("a", 1, &["R1", "R3", "U1"], &[("wt1", "free"), ("fg", "foreground")])], 0)]
    #[case::missing_file(vec![], 1)]
    #[case::extra_file(vec![file("a", 1, &["R1", "R3", "U1"], &[("wt1", "free")]), file("z", 1, &[], &[])], 1)]
    #[case::wrong_phase(vec![file("a", 2, &["R1", "R3", "U1"], &[("wt1", "free")])], 1)]
    #[case::wrong_covers(vec![file("a", 1, &["R1"], &[("wt1", "free")])], 1)]
    #[case::row_without_document(vec![file("a", 1, &["R1", "R3", "U1"], &[("wt1", "unused")])], 1)]
    fn problems__should_name_each_difference(#[case] files: Vec<FileFacts>, #[case] count: usize) {
        // Given
        let catalog = vec![
            entry("a", 1, &["R1", "R3", "U1"], &[("wt1", "free")]),
            entry("later", 2, &["H10", "R3", "U1"], &[("wt1", "working")]),
        ];

        // When
        let found = problems(&catalog, &files, 1);

        // Then
        assert_eq!(found.len(), count, "{found:?}");
    }
}
