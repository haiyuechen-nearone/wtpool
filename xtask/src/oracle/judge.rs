//! The changes the ledger allows: a live entry pins each changed path or key at its current hash,
//! and an accept line names the entry's content.

use std::collections::BTreeMap;

use super::changes::{Accept, Entry};
use super::{ACCEPTED, LOCK, ORACLE_CHANGES};

fn is_accepted(entry: &Entry, accepted: &[Accept]) -> bool {
    accepted
        .iter()
        .any(|line| line.id == entry.id && line.hash.as_deref() == Some(entry.hash.as_str()))
}

/// A change no live entry pins at its current hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unpinned {
    pub name: String,
    pub hash: String,
    /// Live entries that pin it at another hash: an edit came after them.
    pub stale: Vec<String>,
}

/// An entry that pins changes but whose content no accept line names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub id: String,
    pub hash: String,
    pub names: Vec<String>,
}

#[derive(PartialEq, Eq, Debug)]
pub enum Judgement {
    Unchanged,
    AllAccepted(Vec<String>),
    Unaccepted(Vec<Pending>),
    Unlisted(Vec<Unpinned>),
}

/// Judges each changed path or key, with its hash now: a live entry covers it only while its
/// `files` table holds that hash, and allows it only once an accept line names the entry's hash.
pub fn judge(changed: &[(String, String)], live: &[Entry], accepted: &[Accept]) -> Judgement {
    if changed.is_empty() {
        return Judgement::Unchanged;
    }
    let mut unpinned = Vec::new();
    let mut pending: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for (name, hash) in changed {
        let pinning: Vec<&Entry> = live
            .iter()
            .filter(|entry| entry.files.get(name) == Some(hash))
            .collect();
        if pinning.is_empty() {
            unpinned.push(Unpinned {
                name: name.clone(),
                hash: hash.clone(),
                stale: live
                    .iter()
                    .filter(|entry| entry.files.contains_key(name))
                    .map(|entry| entry.id.clone())
                    .collect(),
            });
        } else if !pinning.iter().any(|entry| is_accepted(entry, accepted)) {
            for entry in pinning {
                pending
                    .entry((entry.id.clone(), entry.hash.clone()))
                    .or_default()
                    .push(name.clone());
            }
        }
    }
    if !unpinned.is_empty() {
        return Judgement::Unlisted(unpinned);
    }
    if !pending.is_empty() {
        return Judgement::Unaccepted(
            pending
                .into_iter()
                .map(|((id, hash), names)| Pending { id, hash, names })
                .collect(),
        );
    }
    Judgement::AllAccepted(changed.iter().map(|(name, _)| name.clone()).collect())
}

/// Names the unpinned changes, then the lines that pin each at its current hash, ready to paste.
pub fn unlisted_message(since: &str, unpinned: &[Unpinned], live: &[Entry]) -> String {
    let names: Vec<&str> = unpinned.iter().map(|u| u.name.as_str()).collect();
    let mut text = format!(
        "changed since {since} without a live entry in {ORACLE_CHANGES} whose files table pins \
         its current hash: {}.",
        names.join(", ")
    );
    let stale: Vec<String> = unpinned
        .iter()
        .filter(|u| !u.stale.is_empty())
        .map(|u| format!("{} ({})", u.name, u.stale.join(", ")))
        .collect();
    if !stale.is_empty() {
        text.push_str(&format!(
            " Edited again after the entries that pin another hash of it: {}.",
            stale.join(", ")
        ));
    }
    if names.contains(&LOCK) {
        text.push_str(&format!(
            " The seal of {LOCK} does not match its content: only cargo xtask freeze writes it."
        ));
    }
    let bare: Vec<&str> = live
        .iter()
        .filter(|entry| entry.files.is_empty())
        .map(|entry| entry.id.as_str())
        .collect();
    if !bare.is_empty() {
        text.push_str(&format!(
            " Live entries without a files table pin nothing: {}.",
            bare.join(", ")
        ));
    }
    text.push_str(
        " Put these lines in the files table of the change's entry, a new id or one no freeze \
         spent, after its reason, then have the entry accepted:\n[change.files]\n",
    );
    for change in unpinned {
        text.push_str(&format!(
            "{} = {}\n",
            toml_string(&change.name),
            toml_string(&change.hash)
        ));
    }
    text
}

/// Names each pending entry and the accept line that accepts it at its current content.
pub fn unaccepted_message(since: &str, pending: &[Pending], accepted: &[Accept]) -> String {
    let entries: Vec<String> = pending
        .iter()
        .map(|p| format!("{} ({})", p.id, p.names.join(", ")))
        .collect();
    let mut text = format!(
        "changed since {since}, pinned by entries that no accept line names at their current \
         content: {}.",
        entries.join(", ")
    );
    for entry in pending {
        let earlier = accepted.iter().filter(|line| line.id == entry.id);
        for line in earlier {
            match &line.hash {
                None => text.push_str(&format!(
                    " {ACCEPTED} names {} without an entry hash.",
                    entry.id
                )),
                Some(_) => text.push_str(&format!(
                    " {ACCEPTED} accepts {} at another content: the entry changed after it was \
                     accepted.",
                    entry.id
                )),
            }
        }
    }
    text.push_str(&format!(
        " Review each entry; to accept it, add its line to {ACCEPTED}:\n"
    ));
    for entry in pending {
        text.push_str(&format!("accept {} {}\n", entry.id, entry.hash));
    }
    text
}

fn toml_string(text: &str) -> String {
    toml::Value::String(text.to_owned()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oracle::changes::{MISSING, parse_accepted, parse_entries};
    use rstest::rstest;

    const H1: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const H2: &str = "2222222222222222222222222222222222222222222222222222222222222222";

    fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    /// An entry whose hash is its id with `-h`, so accept lines in the tables stay short.
    fn entry(id: &str, files: &[(&str, &str)]) -> Entry {
        Entry {
            id: id.to_owned(),
            files: owned(files).into_iter().collect(),
            hash: format!("{id}-h"),
        }
    }

    fn accept(id: &str, hash: Option<&str>) -> Accept {
        Accept {
            id: id.to_owned(),
            hash: hash.map(str::to_owned),
        }
    }

    fn must_entries(text: &str) -> Vec<Entry> {
        parse_entries(text).unwrap()
    }

    fn unpinned(name: &str, hash: &str, stale: &[&str]) -> Unpinned {
        Unpinned {
            name: name.to_owned(),
            hash: hash.to_owned(),
            stale: stale.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn pending(id: &str, names: &[&str]) -> Pending {
        Pending {
            id: id.to_owned(),
            hash: format!("{id}-h"),
            names: names.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[rstest]
    #[case::nothing_changed(&[], vec![], vec![], Judgement::Unchanged)]
    #[case::no_entry(
        &[("a", "h1")],
        vec![],
        vec![],
        Judgement::Unlisted(vec![unpinned("a", "h1", &[])])
    )]
    #[case::entry_without_files(
        &[("a", "h1")],
        vec![entry("c1", &[])],
        vec![accept("c1", Some("c1-h"))],
        Judgement::Unlisted(vec![unpinned("a", "h1", &[])])
    )]
    #[case::pinned_not_accepted(
        &[("a", "h1")],
        vec![entry("c1", &[("a", "h1")])],
        vec![],
        Judgement::Unaccepted(vec![pending("c1", &["a"])])
    )]
    #[case::pinned_and_accepted(
        &[("a", "h1")],
        vec![entry("c1", &[("a", "h1")])],
        vec![accept("c1", Some("c1-h"))],
        Judgement::AllAccepted(vec!["a".into()])
    )]
    #[case::edited_after_the_entry(
        &[("a", "h2")],
        vec![entry("c1", &[("a", "h1")])],
        vec![accept("c1", Some("c1-h"))],
        Judgement::Unlisted(vec![unpinned("a", "h2", &["c1"])])
    )]
    #[case::accepted_at_another_content(
        &[("a", "h1"), ("b", "h1")],
        vec![entry("c1", &[("a", "h1"), ("b", "h1")])],
        vec![accept("c1", Some("c1-old"))],
        Judgement::Unaccepted(vec![pending("c1", &["a", "b"])])
    )]
    #[case::accepted_without_hash(
        &[("a", "h1")],
        vec![entry("c1", &[("a", "h1")])],
        vec![accept("c1", None)],
        Judgement::Unaccepted(vec![pending("c1", &["a"])])
    )]
    #[case::accept_line_of_another_id(
        &[("a", "h1")],
        vec![entry("c1", &[("a", "h1")])],
        vec![accept("c2", Some("c1-h"))],
        Judgement::Unaccepted(vec![pending("c1", &["a"])])
    )]
    #[case::one_unpinned_among_accepted(
        &[("a", "h1"), ("clippy.toml", "h2")],
        vec![entry("c1", &[("a", "h1")])],
        vec![accept("c1", Some("c1-h"))],
        Judgement::Unlisted(vec![unpinned("clippy.toml", "h2", &[])])
    )]
    #[case::deletion_pinned(
        &[("a", MISSING)],
        vec![entry("c1", &[("a", MISSING)])],
        vec![accept("c1", Some("c1-h"))],
        Judgement::AllAccepted(vec!["a".into()])
    )]
    #[case::one_of_two_pins_accepted(
        &[("a", "h1")],
        vec![entry("c1", &[("a", "h1")]), entry("c2", &[("a", "h1")])],
        vec![accept("c2", Some("c2-h"))],
        Judgement::AllAccepted(vec!["a".into()])
    )]
    fn judge__should_allow_a_change_only_through_an_accepted_pin(
        #[case] changed: &[(&str, &str)],
        #[case] live: Vec<Entry>,
        #[case] accepted: Vec<Accept>,
        #[case] expected: Judgement,
    ) {
        // Given
        let changed = owned(changed);

        // When
        let judgement = judge(&changed, &live, &accepted);

        // Then
        assert_eq!(judgement, expected);
    }

    #[test]
    fn unlisted_message__should_end_with_a_files_table_that_parses() {
        // Given
        let changes = [
            unpinned("Cargo.toml:workspace.lints", H1, &[]),
            unpinned("xtask/a \"b\".rs", MISSING, &["c1"]),
        ];

        // When
        let message = unlisted_message("the lock", &changes, &[entry("task-end", &[])]);

        // Then
        let (prose, table) = message.split_once('\n').unwrap();
        let ledger = format!("[[change]]\nid = \"new\"\n{table}");
        let parsed = must_entries(&ledger);
        assert_eq!(
            parsed[0].files.clone().into_iter().collect::<Vec<_>>(),
            owned(&[
                ("Cargo.toml:workspace.lints", H1),
                ("xtask/a \"b\".rs", MISSING)
            ])
        );
        assert!(prose.contains("pin nothing: task-end"), "{prose}");
        assert!(prose.contains("(c1)"), "{prose}");
    }

    #[test]
    fn unaccepted_message__should_end_with_the_lines_that_accept_each_entry() {
        // Given
        let entries = must_entries(
            "[[change]]\nid = \"c1\"\n[change.files]\n\"a\" = \"h1\"\n\n[[change]]\nid = \"c2\"\n[change.files]\n\"b\" = \"h1\"\n",
        );
        let pending: Vec<Pending> = entries
            .iter()
            .map(|e| Pending {
                id: e.id.clone(),
                hash: e.hash.clone(),
                names: e.files.keys().cloned().collect(),
            })
            .collect();
        let earlier = [accept("c1", Some(H2))];

        // When
        let message = unaccepted_message("the lock", &pending, &earlier);

        // Then
        let (prose, lines) = message.split_once('\n').unwrap();
        let accepted = parse_accepted(lines).unwrap();
        assert!(entries.iter().all(|e| is_accepted(e, &accepted)), "{lines}");
        assert!(prose.contains("c1 at another content"), "{prose}");
    }
}
