//! `cargo xtask freeze`: commits exactly the oracle author's files with `verify/oracle.lock`,
//! after each new check failed on an assertion in a clone, with its canary when it has one.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::clone::{
    ScratchClone, absent_checks, caught, first_line, implemented_checks, oracle_canaries,
};
use crate::cmd::{self, git};
use crate::oracle::{self, FREEZE_SUBJECT, LOCK, Lock, ORACLE_CHANGES};

pub fn freeze(root: &Path, phase: u8, dry_run: bool) -> Result<(), String> {
    let status = status_entries(&git(root, &["status", "--porcelain", "-uall", "-z"])?);
    let foreign: Vec<&String> = status
        .iter()
        .flat_map(|entry| std::iter::once(&entry.path).chain(entry.from.as_ref()))
        .filter(|rel| !oracle::is_oracle_path(rel, phase) && !is_oracle_key_file(root, rel))
        .collect();
    if !foreign.is_empty() {
        return Err(format!(
            "not oracle files, so freeze will not commit them: {foreign:?}"
        ));
    }
    let changed: Vec<String> = status.into_iter().map(|entry| entry.path).collect();
    let checks = implemented_checks(root)?;
    let previous_lock: Lock = git(root, &["show", &format!("HEAD:{LOCK}")])
        .ok()
        .and_then(|text| oracle::load_lock_text(&text).ok())
        .unwrap_or_default();
    let previous: BTreeSet<String> = previous_lock.checks.iter().cloned().collect();
    let entries = match fs::read_to_string(root.join(ORACLE_CHANGES)) {
        Ok(text) => oracle::parse_entries(&text)?,
        Err(_) => Vec::new(),
    };
    let absent = absent_checks(root)?;
    let canaries = oracle_canaries(root)?;
    let lock = Lock {
        phase,
        checks: checks.iter().cloned().collect(),
        spent: oracle::spent_after(&previous_lock.spent, &entries),
        seal: String::new(),
        files: oracle::oracle_files(root, phase),
        keys: oracle::oracle_keys(root),
        recordings: crate::scrub::recordings_dir()
            .map(|dir| crate::scrub::recording_hashes(&dir))
            .unwrap_or_default(),
    };
    let lock_text = oracle::render_lock(&lock)?;

    let clone = ScratchClone::prepare(root, "freeze")?;
    fs::write(clone.dir.join(LOCK), &lock_text).map_err(|e| e.to_string())?;
    let frozen = clone.commit_freeze(phase)?;
    let mut refused = Vec::new();
    let mut judge = |check: &str, result: &crate::results::CheckResult, with: String| {
        if caught(result) {
            println!(
                "fails on an assertion{with}: {check}: {}",
                first_line(&result.message)
            );
        } else {
            println!(
                "does not fail on an assertion{with}: {check}: {:?} {}",
                result.outcome,
                first_line(&result.message)
            );
            refused.push(check.to_owned());
        }
    };
    // Test once: checks without a canary run in one invocation per part against the frozen
    // tree; only a canary needs a run of its own (verification.md, "The report").
    let mut by_part: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for check in checks.difference(&previous) {
        if absent.contains(check) {
            println!("absent: {check} tests an absence listed in verify/absent.toml");
            continue;
        }
        if let Some(canary) = canaries.iter().find(|c| &c.check == check) {
            clone.reset_to(&frozen)?;
            clone.apply(canary)?;
            let result = clone.run_check(phase, check)?;
            judge(check, &result, format!(" with {}", canary.path.display()));
            continue;
        }
        by_part
            .entry(crate::clone::part_for(&clone.dir, check))
            .or_default()
            .push(check.clone());
    }
    if !by_part.is_empty() {
        clone.reset_to(&frozen)?;
        let parts: Vec<&str> = by_part.keys().copied().collect();
        let found = clone.run_parts(phase, &parts)?;
        for (part, wanted) in &by_part {
            for check in wanted {
                match found.iter().find(|r| &r.check == check) {
                    Some(result) => judge(check, result, String::new()),
                    None => {
                        let missing = crate::results::CheckResult::error(
                            check,
                            part,
                            format!("part {part} reported no result"),
                        );
                        judge(check, &missing, String::new());
                    }
                }
            }
        }
    }
    if !refused.is_empty() {
        return Err(format!(
            "new checks that do not fail on an assertion: {refused:?}"
        ));
    }
    if dry_run {
        println!("dry run: every new check fails as it should; wrote and committed nothing");
        return Ok(());
    }
    fs::write(root.join(LOCK), &lock_text).map_err(|e| e.to_string())?;
    let mut add = vec!["add", "--"];
    add.extend(changed.iter().map(String::as_str));
    add.push(LOCK);
    git(root, &add)?;
    let message = format!("{FREEZE_SUBJECT} {phase}");
    let mut commit: Vec<String> = identity(root);
    commit.extend([
        "commit".to_owned(),
        "--quiet".to_owned(),
        "-m".to_owned(),
        message,
    ]);
    let commit: Vec<&str> = commit.iter().map(String::as_str).collect();
    git(root, &commit)?;
    println!(
        "committed {}",
        git(root, &["rev-parse", "--short", "HEAD"])?.trim()
    );
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct StatusEntry {
    path: String,
    /// The source of a staged rename or copy, which `git add` needs no more.
    from: Option<String>,
}

/// The entries of `git status --porcelain -z`. With `-z` the source of a rename or copy follows
/// its entry as a part of its own, with no status in front.
fn status_entries(status: &str) -> Vec<StatusEntry> {
    let mut entries = Vec::new();
    let mut parts = status.split('\0').filter(|part| !part.is_empty());
    while let Some(part) = parts.next() {
        let code = part.get(..2).unwrap_or_default();
        let from = if code.contains(['R', 'C']) {
            parts.next().map(str::to_owned)
        } else {
            None
        };
        entries.push(StatusEntry {
            path: part.get(3..).unwrap_or_default().to_owned(),
            from,
        });
    }
    entries
}

fn is_oracle_key_file(root: &Path, rel: &str) -> bool {
    let files: BTreeSet<&str> = oracle::ORACLE_KEYS.iter().map(|(file, _)| *file).collect();
    if !files.contains(rel) {
        return false;
    }
    let Ok(now) = fs::read_to_string(root.join(rel)) else {
        return false;
    };
    let Ok(head) = git(root, &["show", &format!("HEAD:{rel}")]) else {
        return false;
    };
    without_oracle_keys(&now, rel) == without_oracle_keys(&head, rel)
}

fn without_oracle_keys(text: &str, rel: &str) -> Option<toml::Table> {
    let mut table = text.parse::<toml::Table>().ok()?;
    for (file, path) in oracle::ORACLE_KEYS {
        if file != rel {
            continue;
        }
        let Some((last, parents)) = path.split_last() else {
            continue;
        };
        if let Some(parent) = table_at(&mut table, parents) {
            parent.remove(*last);
        }
    }
    Some(table)
}

fn table_at<'a>(table: &'a mut toml::Table, path: &[&str]) -> Option<&'a mut toml::Table> {
    match path.split_first() {
        None => Some(table),
        Some((key, rest)) => table_at(table.get_mut(*key)?.as_table_mut()?, rest),
    }
}

/// Cargo hides the global git config (`.cargo/config.toml` sets `GIT_CONFIG_GLOBAL`), so commits
/// name their author from it explicitly.
fn identity(root: &Path) -> Vec<String> {
    if git(root, &["config", "user.email"]).is_ok_and(|e| !e.trim().is_empty()) {
        return Vec::new();
    }
    match global_identity(root, &[]) {
        Some((name, email)) => vec![
            "-c".to_owned(),
            format!("user.name={name}"),
            "-c".to_owned(),
            format!("user.email={email}"),
        ],
        None => Vec::new(),
    }
}

/// `user.name` and `user.email` of the user's global git config, as git itself finds them without
/// `GIT_CONFIG_GLOBAL`: `~/.gitconfig`, `$XDG_CONFIG_HOME/git/config` (else `~/.config/git/config`)
/// and the files they include. `env` adds variables to each git call.
fn global_identity(root: &Path, env: &[(&str, &OsStr)]) -> Option<(String, String)> {
    let read = |key: &str| {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(root)
            .args(["config", "--global", "--includes", key])
            .env_remove("GIT_CONFIG_GLOBAL")
            .envs(env.iter().copied());
        cmd::output(&mut command, Duration::from_secs(30))
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    Some((read("user.name")?, read("user.email")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::gitconfig(".gitconfig")]
    #[case::xdg_config("xdg/git/config")]
    fn global_identity__should_read_every_global_file_and_its_includes(#[case] file: &str) {
        // Given
        let home = std::env::temp_dir().join(format!(
            "xtask-identity-{}-{}",
            std::process::id(),
            file.replace('/', "-")
        ));
        let _ = fs::remove_dir_all(&home);
        let config = home.join(file);
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "[include]\n\tpath = identity.inc\n").unwrap();
        fs::write(
            config.parent().unwrap().join("identity.inc"),
            "[user]\n\tname = Pool Owner\n\temail = owner@example.invalid\n",
        )
        .unwrap();
        let xdg = home.join("xdg");

        // When
        let found = global_identity(
            &home,
            &[
                ("HOME", home.as_os_str()),
                ("XDG_CONFIG_HOME", xdg.as_os_str()),
            ],
        );

        // Then
        let _ = fs::remove_dir_all(&home);
        assert_eq!(
            found,
            Some(("Pool Owner".to_owned(), "owner@example.invalid".to_owned()))
        );
    }

    fn status(path: &str, from: Option<&str>) -> StatusEntry {
        StatusEntry {
            path: path.to_owned(),
            from: from.map(str::to_owned),
        }
    }

    #[rstest]
    #[case::modified(" M xtask/src/a.rs\0", vec![status("xtask/src/a.rs", None)])]
    #[case::untracked("?? verify/b\0", vec![status("verify/b", None)])]
    #[case::renamed(
        "R  xtask/src/clone/mod.rs\0xtask/src/clone.rs\0 M clippy.toml\0",
        vec![
            status("xtask/src/clone/mod.rs", Some("xtask/src/clone.rs")),
            status("clippy.toml", None)
        ]
    )]
    #[case::copied_from_a_short_name(
        "C  verify/b\0a\0",
        vec![status("verify/b", Some("a"))]
    )]
    #[case::empty("", vec![])]
    fn status_entries__should_keep_every_path_whole(
        #[case] text: &str,
        #[case] expected: Vec<StatusEntry>,
    ) {
        // Given
        let porcelain = text;

        // When
        let entries = status_entries(porcelain);

        // Then
        assert_eq!(entries, expected);
    }

    #[rstest]
    #[case::only_lints_changed(
        "[workspace]\nmembers = [\"a\"]\n[workspace.lints.clippy]\npanic = \"deny\"\n",
        "[workspace]\nmembers = [\"a\", \"xtask\"]\n[workspace.lints.clippy]\npanic = \"warn\"\n",
        true
    )]
    #[case::dependency_changed(
        "[workspace]\nmembers = [\"a\"]\n[workspace.dependencies]\nx = \"1\"\n",
        "[workspace]\nmembers = [\"a\"]\n[workspace.dependencies]\nx = \"2\"\n",
        false
    )]
    fn without_oracle_keys__should_keep_only_builder_owned_keys(
        #[case] head: &str,
        #[case] now: &str,
        #[case] equal: bool,
    ) {
        // Given
        let rel = "Cargo.toml";

        // When
        let same = without_oracle_keys(head, rel) == without_oracle_keys(now, rel);

        // Then
        assert_eq!(same, equal);
    }
}
