//! The ledger of oracle changes: the entries of `verify/oracle-changes.toml` and the accept lines
//! of `verify/accepted.txt`.

use std::collections::{BTreeMap, BTreeSet};

use crate::repo::sha256;

use super::{ACCEPTED, ORACLE_CHANGES};

/// The hash a `files` table gives a path or key that the change deletes.
pub const MISSING: &str = "missing";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    /// Each path, or `file:key` for an oracle key, with its hash after the change.
    pub files: BTreeMap<String, String>,
    /// The hash of the entry's whole content, which its accept line names.
    pub hash: String,
}

/// The entries of `verify/oracle-changes.toml`. Keys other than `id` and `files`, such as
/// `reason`, count only through the entry's hash.
pub fn parse_entries(text: &str) -> Result<Vec<Entry>, String> {
    let table: toml::Table = text
        .parse()
        .map_err(|error| format!("{ORACLE_CHANGES} does not parse: {error}"))?;
    let Some(changes) = table.get("change") else {
        return Ok(Vec::new());
    };
    let changes = changes
        .as_array()
        .ok_or_else(|| format!("{ORACLE_CHANGES}: change is not an array of tables"))?;
    changes
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let entry = value
                .as_table()
                .ok_or_else(|| format!("{ORACLE_CHANGES}: change {} is not a table", index + 1))?;
            let id = entry
                .get("id")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| format!("{ORACLE_CHANGES}: change {} has no id", index + 1))?
                .to_owned();
            let files = match entry.get("files") {
                None => BTreeMap::new(),
                Some(files) => files
                    .as_table()
                    .ok_or_else(|| format!("{ORACLE_CHANGES}: {id}: files is not a table"))?
                    .iter()
                    .map(|(name, hash)| match hash.as_str() {
                        Some(hash) => Ok((name.clone(), hash.to_owned())),
                        None => Err(format!(
                            "{ORACLE_CHANGES}: {id}: the hash of {name} is not a string"
                        )),
                    })
                    .collect::<Result<_, String>>()?,
            };
            Ok(Entry {
                id,
                files,
                hash: content_hash(entry),
            })
        })
        .collect()
}

/// SHA 256 of an entry's content as JSON with sorted keys, so layout, comments and key order
/// leave it unchanged.
pub fn content_hash(entry: &toml::Table) -> String {
    match serde_json::to_value(entry).and_then(|value| serde_json::to_string(&value)) {
        Ok(json) => sha256(json.as_bytes()),
        Err(_) => "unserializable".to_owned(),
    }
}

/// The ids a freeze spends: every entry it writes into the lock, and every id spent before, so
/// an entry removed from the ledger cannot come back under its old id.
pub fn spent_after(previous: &BTreeSet<String>, entries: &[Entry]) -> BTreeSet<String> {
    previous
        .iter()
        .cloned()
        .chain(entries.iter().map(|entry| entry.id.clone()))
        .collect()
}

/// The entries a change may still use: those whose id no freeze spent.
pub fn live_entries(entries: Vec<Entry>, spent: &BTreeSet<String>) -> Vec<Entry> {
    entries
        .into_iter()
        .filter(|entry| !spent.contains(&entry.id))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accept {
    pub id: String,
    /// `None` on a line written before accept lines named an entry hash; it accepts no entry.
    pub hash: Option<String>,
}

/// The lines of `verify/accepted.txt`: `accept <id> <entry hash>`, a `#` comment or blank.
pub fn parse_accepted(text: &str) -> Result<Vec<Accept>, String> {
    let mut accepts = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        let accept = match words.as_slice() {
            ["accept", id] => Accept {
                id: (*id).to_owned(),
                hash: None,
            },
            ["accept", id, hash] if is_hash(hash) => Accept {
                id: (*id).to_owned(),
                hash: Some((*hash).to_owned()),
            },
            _ => {
                return Err(format!(
                    "{ACCEPTED} line {}: `{line}` is not `accept <id> <entry hash>`, a # comment or blank",
                    index + 1
                ));
            }
        };
        accepts.push(accept);
    }
    Ok(accepts)
}

fn is_hash(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    const H1: &str = "1111111111111111111111111111111111111111111111111111111111111111";

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

    #[rstest]
    #[case::comments_and_blanks("# accepted\n\n  # indented\n", Ok(vec![]))]
    #[case::with_hash(
        &format!("accept c1 {H1}\n"),
        Ok(vec![accept("c1", Some(H1))])
    )]
    #[case::from_before_entry_hashes("accept c1\n", Ok(vec![accept("c1", None)]))]
    #[case::other_verb(&format!("reject c1 {H1}\n"), Err(1))]
    #[case::other_verb_without_hash("# ok\nreject c1\n", Err(2))]
    #[case::bare_id("c1\n", Err(1))]
    #[case::not_a_hash("accept c1 deadbeef\n", Err(1))]
    #[case::upper_case_hash(&format!("accept c1 {}\n", H1.replace('1', "A")), Err(1))]
    #[case::trailing_words(&format!("accept c1 {H1} too\n"), Err(1))]
    fn parse_accepted__should_take_only_accept_lines(
        #[case] text: &str,
        #[case] expected: Result<Vec<Accept>, usize>,
    ) {
        // Given
        let file = text;

        // When
        let parsed = parse_accepted(file);

        // Then
        match expected {
            Ok(lines) => assert_eq!(parsed, Ok(lines)),
            Err(line) => {
                let error = parsed.expect_err("a malformed line");
                assert!(error.contains(&format!("line {line}:")), "{error}");
            }
        }
    }

    fn must_entries(text: &str) -> Vec<Entry> {
        parse_entries(text).unwrap()
    }

    #[test]
    fn parse_entries__should_read_the_files_table() {
        // Given
        let text = "[[change]]\nid = \"c1\"\nreason = \"r\"\n[change.files]\n\"xtask/a.rs\" = \"h1\"\n\"Cargo.toml:workspace.members\" = \"missing\"\n\n[[change]]\nid = \"c2\"\npaths = [\"xtask/\"]\n";

        // When
        let entries = must_entries(text);

        // Then
        let files: Vec<Vec<(String, String)>> = entries
            .iter()
            .map(|e| e.files.clone().into_iter().collect())
            .collect();
        assert_eq!(
            files,
            vec![
                owned(&[
                    ("Cargo.toml:workspace.members", MISSING),
                    ("xtask/a.rs", "h1")
                ]),
                vec![]
            ]
        );
    }

    #[rstest]
    #[case::unparseable("[[change]\n")]
    #[case::no_id("[[change]]\nreason = \"r\"\n")]
    #[case::hash_not_a_string("[[change]]\nid = \"c1\"\nfiles = { \"a\" = 1 }\n")]
    #[case::files_not_a_table("[[change]]\nid = \"c1\"\nfiles = [\"a\"]\n")]
    fn parse_entries__should_refuse_a_malformed_ledger(#[case] text: &str) {
        // Given
        let ledger = text;

        // When
        let parsed = parse_entries(ledger);

        // Then
        assert!(parsed.is_err(), "{parsed:?}");
    }

    #[rstest]
    #[case::reordered_and_reformatted(
        "[[change]]\nreason = \"r\"\nid = \"c1\"\n# a note\nfiles = { \"a\" = \"h1\" }\n",
        true
    )]
    #[case::widened(
        "[[change]]\nid = \"c1\"\nreason = \"r\"\n[change.files]\n\"a\" = \"h1\"\n\"b\" = \"h2\"\n",
        false
    )]
    #[case::other_hash(
        "[[change]]\nid = \"c1\"\nreason = \"r\"\n[change.files]\n\"a\" = \"h2\"\n",
        false
    )]
    #[case::other_reason(
        "[[change]]\nid = \"c1\"\nreason = \"s\"\n[change.files]\n\"a\" = \"h1\"\n",
        false
    )]
    fn content_hash__should_change_with_the_content_alone(#[case] other: &str, #[case] same: bool) {
        // Given
        let accepted = "[[change]]\nid = \"c1\"\nreason = \"r\"\n[change.files]\n\"a\" = \"h1\"\n";

        // When
        let hashes = (
            must_entries(accepted)[0].hash.clone(),
            must_entries(other)[0].hash.clone(),
        );

        // Then
        assert_eq!(hashes.0 == hashes.1, same);
    }

    #[test]
    fn spent_after__should_keep_earlier_ids_and_add_every_entry() {
        // Given
        let previous = BTreeSet::from(["gone".to_owned(), "c1".to_owned()]);
        let entries = [entry("c1", &[]), entry("c2", &[])];

        // When
        let spent = spent_after(&previous, &entries);

        // Then
        assert_eq!(
            spent,
            BTreeSet::from(["c1".to_owned(), "c2".to_owned(), "gone".to_owned()])
        );
    }

    #[rstest]
    #[case::none_spent(&["c1", "c2"], &[], &["c1", "c2"])]
    #[case::one_spent(&["c1", "c2"], &["c1"], &["c2"])]
    #[case::reused_spent_id(&["c1", "c1"], &["c1"], &[])]
    fn live_entries__should_drop_entries_a_freeze_spent(
        #[case] ids: &[&str],
        #[case] spent: &[&str],
        #[case] expected: &[&str],
    ) {
        // Given
        let entries: Vec<Entry> = ids.iter().map(|id| entry(id, &[("a", "h1")])).collect();
        let spent: BTreeSet<String> = spent.iter().map(|s| (*s).to_owned()).collect();

        // When
        let live = live_entries(entries, &spent);

        // Then
        let ids: Vec<&str> = live.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, expected);
    }
}
