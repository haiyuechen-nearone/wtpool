use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cmd::git;
use crate::repo::{entry_hash, sha256, walk};
use crate::results::CheckResult;

mod changes;
mod judge;

pub use changes::{
    Accept, Entry, MISSING, content_hash, live_entries, parse_accepted, parse_entries, spent_after,
};
pub use judge::{Judgement, Pending, Unpinned, judge, unaccepted_message, unlisted_message};

pub const LOCK: &str = "verify/oracle.lock";
pub const FREEZE_SUBJECT: &str = "verify: freeze phase";
pub const ORACLE_CHANGES: &str = "verify/oracle-changes.toml";
pub const ACCEPTED: &str = "verify/accepted.txt";

const ORACLE_DIRS: [&str; 4] = ["xtask", "vscode/test", "verify", "docs/design"];
const ORACLE_FILES: [&str; 4] = [
    "clippy.toml",
    "rustfmt.toml",
    "vscode/eslint.config.mjs",
    "vscode/.stylelintrc.json",
];
const ORACLE_FILES_FROM_PHASE_1: [&str; 3] = [
    "vscode/package.json",
    "vscode/package-lock.json",
    "vscode/.gitignore",
];
/// Folders and files under the oracle folders that are not oracle files.
const NOT_ORACLE: [&str; 5] = [
    "verify/out",
    "verify/reviews",
    ACCEPTED,
    ORACLE_CHANGES,
    "xtask/target",
];

/// Oracle parts of files the builder otherwise owns: (file, TOML key path).
pub const ORACLE_KEYS: [(&str, &[&str]); 3] = [
    ("Cargo.toml", &["workspace", "members"]),
    ("Cargo.toml", &["workspace", "lints"]),
    (".cargo/config.toml", &["alias"]),
];

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Lock {
    pub phase: u8,
    /// Every check the frozen oracle implements, so the next freeze knows which checks are new.
    pub checks: Vec<String>,
    /// The id of every entry of oracle-changes.toml at a freeze. The lock holds their changes,
    /// so none allows a change again, in any copy of the history.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub spent: BTreeSet<String>,
    /// SHA 256 of the rest of the lock, which freeze writes: a lock edited by hand no longer
    /// matches it, so the lock itself counts as changed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub seal: String,
    pub files: BTreeMap<String, String>,
    pub keys: BTreeMap<String, String>,
    /// The raw recordings of the oracle step's live contract run, by their path below
    /// `recordings/` of the harness data folder (crate::scrub), from phase 2.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub recordings: BTreeMap<String, String>,
}

pub fn is_oracle_path(rel: &str, phase: u8) -> bool {
    let under = |dir: &str| rel == dir || rel.starts_with(&format!("{dir}/"));
    if NOT_ORACLE.iter().any(|excluded| under(excluded)) {
        return false;
    }
    if rel.split('/').any(|part| part == "node_modules") || rel.ends_with(".DS_Store") {
        return false;
    }
    ORACLE_DIRS.iter().any(|dir| under(dir))
        || ORACLE_FILES.contains(&rel)
        || (phase >= 1 && ORACLE_FILES_FROM_PHASE_1.contains(&rel))
}

/// The hash of every oracle file on disk, the lock itself excluded.
pub fn oracle_files(root: &Path, phase: u8) -> BTreeMap<String, String> {
    let skip_dir = |rel: &str| !is_oracle_dir_worth_walking(rel);
    let mut candidates: BTreeSet<String> = BTreeSet::new();
    for dir in ORACLE_DIRS {
        candidates.extend(walk(root, &root.join(dir), &skip_dir));
    }
    for file in ORACLE_FILES.iter().chain(ORACLE_FILES_FROM_PHASE_1.iter()) {
        if fs::symlink_metadata(root.join(file)).is_ok() {
            candidates.insert((*file).to_owned());
        }
    }
    candidates
        .into_iter()
        .filter(|rel| rel != LOCK && is_oracle_path(rel, phase))
        .map(|rel| {
            let hash = entry_hash(&root.join(&rel));
            (rel, hash)
        })
        .collect()
}

fn is_oracle_dir_worth_walking(rel: &str) -> bool {
    !NOT_ORACLE
        .iter()
        .any(|excluded| rel == *excluded || rel.starts_with(&format!("{excluded}/")))
        && !rel.ends_with("/node_modules")
}

pub fn key_name(file: &str, path: &[&str]) -> String {
    format!("{file}:{}", path.join("."))
}

pub fn oracle_keys(root: &Path) -> BTreeMap<String, String> {
    ORACLE_KEYS
        .iter()
        .map(|(file, path)| {
            let hash = match fs::read_to_string(root.join(file)) {
                Err(_) => "absent file".to_owned(),
                Ok(text) => key_hash(&text, path),
            };
            (key_name(file, path), hash)
        })
        .collect()
}

pub fn key_hash(text: &str, path: &[&str]) -> String {
    let Ok(table) = text.parse::<toml::Table>() else {
        return "unparseable".to_owned();
    };
    let mut value = toml::Value::Table(table);
    for key in path {
        let Some(next) = value.get(*key).cloned() else {
            return "absent".to_owned();
        };
        value = next;
    }
    match serde_json::to_string(&value) {
        Ok(json) => sha256(json.as_bytes()),
        Err(_) => "unserializable".to_owned(),
    }
}

/// Paths and keys whose current hash differs from the lock, including added and removed ones.
pub fn differences(
    lock: &Lock,
    files: &BTreeMap<String, String>,
    keys: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut changed = BTreeSet::new();
    for (locked, current) in [(&lock.files, files), (&lock.keys, keys)] {
        for (name, hash) in locked {
            if current.get(name) != Some(hash) {
                changed.insert(name.clone());
            }
        }
        for name in current.keys() {
            if !locked.contains_key(name) {
                changed.insert(name.clone());
            }
        }
    }
    changed.into_iter().collect()
}

pub fn load_lock_text(text: &str) -> Result<Lock, String> {
    toml::from_str(text).map_err(|error| format!("{LOCK} does not parse: {error}"))
}

/// SHA 256 of the lock's content without its seal.
pub fn seal_of(lock: &Lock) -> String {
    let unsealed = Lock {
        seal: String::new(),
        ..lock.clone()
    };
    match serde_json::to_string(&unsealed) {
        Ok(json) => sha256(json.as_bytes()),
        Err(_) => "unserializable".to_owned(),
    }
}

/// The lock's text, sealed.
pub fn render_lock(lock: &Lock) -> Result<String, String> {
    let sealed = Lock {
        seal: seal_of(lock),
        ..lock.clone()
    };
    let body = toml::to_string(&sealed).map_err(|error| error.to_string())?;
    Ok(format!(
        "# Written by `cargo xtask freeze`. SHA 256 per oracle file and oracle key.\n{body}"
    ))
}

pub fn freeze_commit(root: &Path) -> Option<String> {
    let grep = format!("--grep=^{FREEZE_SUBJECT}");
    git(root, &["log", "-1", "--format=%H", &grep, "--", LOCK])
        .ok()
        .map(|sha| sha.trim().to_owned())
        .filter(|sha| !sha.is_empty())
}

/// Each oracle path and key that differs from the lock, with its hash now ([`MISSING`] once it
/// is gone), and the lock itself, with the hash of its file, once its seal no longer matches.
pub fn changed_names(root: &Path, lock: &Lock) -> Vec<(String, String)> {
    let files = oracle_files(root, lock.phase);
    let keys = oracle_keys(root);
    let mut changed: Vec<(String, String)> = differences(lock, &files, &keys)
        .into_iter()
        .map(|name| {
            let hash = files.get(&name).or_else(|| keys.get(&name)).cloned();
            (name, hash.unwrap_or_else(|| MISSING.to_owned()))
        })
        .collect();
    if lock.seal != seal_of(lock) {
        changed.push((LOCK.to_owned(), entry_hash(&root.join(LOCK))));
    }
    changed
}

/// A ledger file parsed, or empty while it does not exist.
fn read_ledger<T: Default>(
    root: &Path,
    rel: &str,
    parse: fn(&str) -> Result<T, String>,
) -> Result<T, String> {
    match fs::read_to_string(root.join(rel)) {
        Ok(text) => parse(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(format!("{rel}: {error}")),
    }
}

/// Compares the oracle with the lock alone, so every copy of the history, a squashed one
/// included, judges alike.
pub fn oracle_unchanged(root: &Path, phase: u8) -> CheckResult {
    const NAME: &str = "oracle_unchanged";
    const LAYER: &str = "harness";
    let Ok(text) = fs::read_to_string(root.join(LOCK)) else {
        return CheckResult::assertion(
            NAME,
            LAYER,
            format!("{LOCK} is missing: no oracle was frozen"),
        );
    };
    let lock = match load_lock_text(&text) {
        Ok(lock) => lock,
        Err(error) => return CheckResult::assertion(NAME, LAYER, error),
    };
    if lock.phase < phase {
        return CheckResult::assertion(
            NAME,
            LAYER,
            format!("the oracle is frozen for phase {}, not {phase}", lock.phase),
        );
    }
    let ledger = read_ledger(root, ORACLE_CHANGES, parse_entries)
        .and_then(|entries| Ok((entries, read_ledger(root, ACCEPTED, parse_accepted)?)));
    let (entries, accepted) = match ledger {
        Ok(ledger) => ledger,
        Err(error) => return CheckResult::assertion(NAME, LAYER, error),
    };
    let live = live_entries(entries, &lock.spent);
    let since = format!("the lock of phase {}", lock.phase);
    match judge(&changed_names(root, &lock), &live, &accepted) {
        Judgement::Unchanged => CheckResult::pass(NAME, LAYER, format!("equal to {since}")),
        Judgement::AllAccepted(names) => CheckResult::pass(
            NAME,
            LAYER,
            format!("changed since {since}, all accepted: {}", names.join(", ")),
        ),
        Judgement::Unaccepted(pending) => CheckResult::needs_you(
            NAME,
            LAYER,
            "oracle_change",
            unaccepted_message(&since, &pending, &accepted),
        ),
        Judgement::Unlisted(unpinned) => {
            CheckResult::assertion(NAME, LAYER, unlisted_message(&since, &unpinned, &live))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::{FailKind, Outcome};
    use rstest::rstest;

    #[rstest]
    #[case::xtask_source("xtask/src/main.rs", 0, true)]
    #[case::xtask_build_output("xtask/target/debug/xtask", 0, false)]
    #[case::verify_data("verify/trace.toml", 0, true)]
    #[case::verify_lock("verify/oracle.lock", 0, true)]
    #[case::run_folder("verify/out/abc/report.md", 0, false)]
    #[case::reviews("verify/reviews/phase-0/report.md", 0, false)]
    #[case::accepted("verify/accepted.txt", 0, false)]
    #[case::oracle_changes("verify/oracle-changes.toml", 0, false)]
    #[case::extension_tests("vscode/test/fake-wtpool.mjs", 0, true)]
    #[case::extension_source("vscode/src/extension.ts", 0, false)]
    #[case::design_doc("docs/design/status-panel/status-panel.md", 0, true)]
    #[case::other_doc("docs/guide.md", 0, false)]
    #[case::clippy_config("clippy.toml", 0, true)]
    #[case::rustfmt_config("rustfmt.toml", 0, true)]
    #[case::eslint_config("vscode/eslint.config.mjs", 0, true)]
    #[case::manifest_before_phase_1("vscode/package.json", 0, false)]
    #[case::manifest_from_phase_1("vscode/package.json", 1, true)]
    #[case::lockfile_from_phase_1("vscode/package-lock.json", 1, true)]
    #[case::product_crate("crates/wtpool/src/main.rs", 2, false)]
    #[case::dependencies("vscode/test/node_modules/x/index.js", 1, false)]
    #[case::finder_noise("verify/.DS_Store", 0, false)]
    #[case::prefix_lookalike("verifyx/a", 0, false)]
    fn is_oracle_path__should_follow_the_oracle_file_list(
        #[case] rel: &str,
        #[case] phase: u8,
        #[case] expected: bool,
    ) {
        // Given
        let path = rel;

        // When
        let oracle = is_oracle_path(path, phase);

        // Then
        assert_eq!(oracle, expected);
    }

    #[rstest]
    #[case::members_present("[workspace]\nmembers = [\"a\"]\n", &["workspace", "members"], false)]
    #[case::members_absent("[workspace]\n", &["workspace", "members"], true)]
    fn key_hash__should_mark_absent_keys(
        #[case] text: &str,
        #[case] path: &[&str],
        #[case] absent: bool,
    ) {
        // Given
        let manifest = text;

        // When
        let hash = key_hash(manifest, path);

        // Then
        assert_eq!(hash == "absent", absent);
    }

    #[test]
    fn key_hash__should_ignore_other_keys_and_formatting() {
        // Given
        let before = "[workspace]\nmembers = [\"a\", \"b\"]\n[workspace.dependencies]\nx = \"1\"\n";
        let after =
            "[workspace]\nmembers = [ \"a\",\n \"b\" ]\n[workspace.dependencies]\nx = \"2\"\n";

        // When
        let hashes = (
            key_hash(before, &["workspace", "members"]),
            key_hash(after, &["workspace", "members"]),
        );

        // Then
        assert_eq!(hashes.0, hashes.1);
    }

    fn lock_of(files: &[(&str, &str)]) -> Lock {
        Lock {
            files: files
                .iter()
                .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
                .collect(),
            ..Lock::default()
        }
    }

    #[rstest]
    #[case::equal(&[("a", "1")], &[("a", "1")], &[])]
    #[case::edited(&[("a", "1")], &[("a", "2")], &["a"])]
    #[case::removed(&[("a", "1")], &[], &["a"])]
    #[case::added(&[], &[("b", "1")], &["b"])]
    fn differences__should_list_edited_added_and_removed(
        #[case] locked: &[(&str, &str)],
        #[case] current: &[(&str, &str)],
        #[case] expected: &[&str],
    ) {
        // Given
        let lock = lock_of(locked);
        let files = lock_of(current).files;

        // When
        let changed = differences(&lock, &files, &BTreeMap::new());

        // Then
        assert_eq!(
            changed,
            expected.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()
        );
    }

    #[rstest]
    #[case::phase(|lock: &mut Lock| lock.phase = 2)]
    #[case::checks(|lock: &mut Lock| lock.checks.push("c".into()))]
    #[case::spent(|lock: &mut Lock| { lock.spent.insert("c1".into()); })]
    #[case::file_hash(|lock: &mut Lock| { lock.files.insert("a".into(), "2".into()); })]
    #[case::key_hash(|lock: &mut Lock| { lock.keys.insert("k".into(), "2".into()); })]
    #[case::recording(|lock: &mut Lock| { lock.recordings.insert("r".into(), "1".into()); })]
    fn render_lock__should_seal_every_field(#[case] edit: fn(&mut Lock)) {
        // Given
        let mut lock = lock_of(&[("a", "1")]);
        let rendered = load_lock_text(&render_lock(&lock).unwrap()).unwrap();

        // When
        edit(&mut lock);
        lock.seal = rendered.seal.clone();

        // Then
        assert_eq!(seal_of(&rendered), rendered.seal);
        assert_ne!(seal_of(&lock), lock.seal);
    }

    /// A tree with one oracle file, frozen by a lock that spent the entry `old`. No git: the
    /// check reads the lock alone.
    fn must_frozen_tree(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("xtask-oracle-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("xtask")).unwrap();
        fs::create_dir_all(dir.join("verify")).unwrap();
        fs::write(dir.join("xtask/a.rs"), "a\n").unwrap();
        let lock = Lock {
            phase: 3,
            spent: BTreeSet::from(["old".to_owned()]),
            files: oracle_files(&dir, 3),
            keys: oracle_keys(&dir),
            ..Lock::default()
        };
        fs::write(dir.join(LOCK), render_lock(&lock).unwrap()).unwrap();
        dir
    }

    /// Writes an entry `id` that pins xtask/a.rs as it is now, and its accept line.
    fn pin_and_accept(dir: &Path, id: &str) {
        let hash = entry_hash(&dir.join("xtask/a.rs"));
        let ledger =
            format!("[[change]]\nid = \"{id}\"\n[change.files]\n\"xtask/a.rs\" = \"{hash}\"\n");
        let entry = parse_entries(&ledger).unwrap()[0].hash.clone();
        fs::write(dir.join(ORACLE_CHANGES), ledger).unwrap();
        fs::write(dir.join(ACCEPTED), format!("accept {id} {entry}\n")).unwrap();
    }

    fn after_the_message(result: &CheckResult) -> String {
        result
            .message
            .split_once('\n')
            .map(|(_, rest)| rest.to_owned())
            .unwrap_or_default()
    }

    #[test]
    fn oracle_unchanged__should_take_the_lines_its_messages_print() {
        // Given
        let dir = must_frozen_tree("paste");

        // When
        let equal = oracle_unchanged(&dir, 3);
        fs::write(dir.join("xtask/a.rs"), "b\n").unwrap();
        let unlisted = oracle_unchanged(&dir, 3);
        let table = after_the_message(&unlisted);
        let ledger = format!("[[change]]\nid = \"c1\"\nreason = \"r\"\n{table}");
        fs::write(dir.join(ORACLE_CHANGES), ledger).unwrap();
        let pending = oracle_unchanged(&dir, 3);
        let line = after_the_message(&pending);
        fs::write(dir.join(ACCEPTED), format!("# c1: r\n{line}")).unwrap();
        let accepted = oracle_unchanged(&dir, 3);
        fs::write(dir.join("xtask/a.rs"), "c\n").unwrap();
        let edited_again = oracle_unchanged(&dir, 3);

        // Then
        let _ = fs::remove_dir_all(&dir);
        let outcomes = [&equal, &unlisted, &pending, &accepted, &edited_again]
            .map(|result| (result.outcome, result.kind, result.code.clone()));
        let assertion = (Outcome::Fail, Some(FailKind::Assertion), None);
        assert_eq!(
            outcomes,
            [
                (Outcome::Pass, None, None),
                assertion.clone(),
                (Outcome::NeedsYou, None, Some("oracle_change".to_owned())),
                (Outcome::Pass, None, None),
                assertion,
            ],
            "{}\n{}\n{}",
            unlisted.message,
            pending.message,
            edited_again.message
        );
        assert!(
            edited_again.message.contains("xtask/a.rs (c1)"),
            "{}",
            edited_again.message
        );
    }

    #[rstest]
    #[case::reused_spent_id("spent", |dir: &Path| pin_and_accept(dir, "old"))]
    #[case::other_verb("verb", |dir: &Path| {
        pin_and_accept(dir, "c1");
        let text = fs::read_to_string(dir.join(ACCEPTED)).unwrap();
        fs::write(dir.join(ACCEPTED), text.replace("accept", "reject")).unwrap();
    })]
    #[case::lock_edited_by_hand("lock", |dir: &Path| {
        let text = fs::read_to_string(dir.join(LOCK)).unwrap();
        let (a, b) = (sha256(b"a\n"), sha256(b"b\n"));
        fs::write(dir.join(LOCK), text.replace(&a, &b)).unwrap();
    })]
    #[case::unparseable_ledger("ledger", |dir: &Path| {
        fs::write(dir.join(ORACLE_CHANGES), "[[change]\n").unwrap();
    })]
    fn oracle_unchanged__should_fail_on_each_way_around_the_gate(
        #[case] name: &str,
        #[case] tamper: fn(&Path),
    ) {
        // Given
        let dir = must_frozen_tree(name);
        fs::write(dir.join("xtask/a.rs"), "b\n").unwrap();

        // When
        tamper(&dir);
        let result = oracle_unchanged(&dir, 3);

        // Then
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(result.kind, Some(FailKind::Assertion), "{}", result.message);
    }
}
