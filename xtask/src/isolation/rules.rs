//! The traces each part left, judged from the snapshots taken around it and at `--report`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::present::{outside_remotes, verify_refs};
use super::snapshot::{ActivityEntry, Invocation, Places, Snapshot, reflog_email};
use super::{NAME, VERIFY_EMAIL};
use crate::results::CheckResult;

/// The traces one part left behind, judged from the snapshots around it.
pub fn traces(invocation: &Invocation, sandbox_roots: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    let (before, after) = (&invocation.before, &invocation.after);
    let part = &invocation.part;
    let pool = &after.pool;
    if before.pool != after.pool {
        found.push(format!(
            "{part}: the snapshots read two pools, {} and {pool}, so they compare nothing",
            before.pool
        ));
    }
    match (&before.wtpool_activity, &after.wtpool_activity) {
        (None, Some(_)) => found.push(format!("{part}: created {pool}/.git/wtpool-activity/")),
        (_, Some(entries)) => {
            let earlier: BTreeSet<&ActivityEntry> =
                before.wtpool_activity.iter().flatten().collect();
            for entry in entries
                .iter()
                .filter(|e| e.mentions_run && !earlier.contains(e))
            {
                found.push(format!("{part}: wrote wtpool-activity/{}", entry.name));
            }
        }
        _ => {}
    }
    for worktree in &after.worktrees {
        let inside_sandbox = sandbox_roots
            .iter()
            .any(|root| worktree == root || worktree.starts_with(&format!("{root}/")));
        if inside_sandbox {
            found.push(format!(
                "{part}: {pool} has a worktree in the run folder: {worktree}"
            ));
        } else if after.verify_worktrees.contains(worktree) {
            found.push(format!(
                "{part}: {pool} has a worktree at a commit by wtpool-verify: {worktree}"
            ));
        }
    }
    if after.listing_mentions_run {
        found.push(format!(
            "{part}: git -C {pool} worktree list names the run folder"
        ));
    }
    for worktree in before
        .worktrees
        .iter()
        .filter(|w| !after.worktrees.contains(w))
    {
        found.push(format!(
            "{part}: {pool} lost or moved its worktree {worktree}"
        ));
    }
    for worktree in after
        .missing_folders
        .iter()
        .filter(|w| before.worktrees.contains(w) && !before.missing_folders.contains(w))
    {
        found.push(format!(
            "{part}: the folder of {pool} worktree {worktree} is gone"
        ));
    }
    if after.fallback_email.is_none() {
        found.push(format!(
            "{part}: git's fallback identity is unknown, so a HEAD move under it goes unseen"
        ));
    }
    // The owner's own work moves HEADs under their address or a throwaway one, never under these.
    let run_emails: BTreeSet<&str> = [Some(VERIFY_EMAIL)]
        .into_iter()
        .chain([
            before.fallback_email.as_deref(),
            after.fallback_email.as_deref(),
        ])
        .flatten()
        .collect();
    for (worktree, entries) in &after.head_entries {
        let earlier: BTreeSet<&String> = before
            .head_entries
            .get(worktree)
            .into_iter()
            .flatten()
            .collect();
        let added: BTreeSet<&str> = entries
            .iter()
            .filter(|entry| !earlier.contains(entry))
            .filter_map(|entry| reflog_email(entry))
            .filter(|email| run_emails.contains(email))
            .collect();
        if !added.is_empty() {
            let names: Vec<&str> = added.into_iter().collect();
            found.push(format!(
                "{part}: the HEAD of {pool} worktree {worktree} moved under {}, an \
                 identity of the run",
                names.join(", ")
            ));
        }
    }
    if before.wtpool_toml != after.wtpool_toml {
        found.push(format!("{part}: changed {pool}/.git/wtpool.toml"));
    }
    if after.settings_mention_wtpool_verify {
        found.push(format!("{part}: {} mentions wtpool-verify", after.settings));
    }
    for extension in after
        .vscode_extensions
        .iter()
        .filter(|e| !before.vscode_extensions.contains(e))
    {
        found.push(format!("{part}: created ~/.vscode/extensions/{extension}"));
    }
    found
}

/// The milliseconds in a part record's name `<part>-<millis>.json`.
fn recorded_at(file: &Path) -> u128 {
    file.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.rsplit_once('-'))
        .and_then(|(_, millis)| millis.parse().ok())
        .unwrap_or_default()
}

/// Judges the part records of `run_dir`, then `last`, a snapshot taken after them, against the
/// newest record's after.
pub fn check(
    run_dir: &Path,
    places: &Places,
    sandbox_roots: &[String],
    last: &Snapshot,
) -> CheckResult {
    let mut found = Vec::new();
    let dir = run_dir.join("isolation");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .map(|entries| entries.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    files.sort_by_key(|file| (recorded_at(file), file.clone()));
    let mut newest = None;
    for file in &files {
        let parsed = fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str::<Invocation>(&text).ok());
        match parsed {
            Some(invocation) => {
                found.extend(traces(&invocation, sandbox_roots));
                newest = Some(invocation.after);
            }
            None => found.push(format!("{} does not parse", file.display())),
        }
    }
    if let Some(before) = newest {
        found.extend(traces(
            &Invocation {
                part: "report".to_owned(),
                before,
                after: last.clone(),
            },
            sandbox_roots,
        ));
    }
    if files.is_empty() {
        return CheckResult::assertion(
            NAME,
            "report",
            "no part of this run recorded its snapshots",
        );
    }
    found.extend(outside_remotes(run_dir));
    let pool_note = match places.pool.as_deref() {
        Some(pool) if last.pool_found => {
            found.extend(verify_refs(pool));
            format!("the real pool {} ({})", pool.display(), places.pool_source)
        }
        Some(pool) => format!(
            "no worktree at {} ({}), so the pool rules were skipped",
            pool.display(),
            places.pool_source
        ),
        None => format!(
            "no real pool: {}, so the pool rules were skipped",
            places.pool_source
        ),
    };
    if found.is_empty() {
        CheckResult::pass(
            NAME,
            "report",
            format!("{} part runs left no trace; {pool_note}", files.len()),
        )
    } else {
        CheckResult::assertion(NAME, "report", found.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use rstest::rstest;

    fn entry(name: &str, mentions_run: bool) -> ActivityEntry {
        ActivityEntry {
            name: name.into(),
            hash: "h".into(),
            mentions_run,
        }
    }

    fn moves(worktree: &str, entries: &[String]) -> BTreeMap<String, Vec<String>> {
        BTreeMap::from([(worktree.to_owned(), entries.to_vec())])
    }

    fn line(email: &str, number: u8) -> String {
        format!("{number:040} {number:040} n <{email}> 1792790000 +0200\tcheckout: x")
    }

    fn known() -> Snapshot {
        Snapshot {
            fallback_email: Some(FALLBACK.into()),
            ..Snapshot::default()
        }
    }

    const FALLBACK: &str = "u@host.invalid";

    fn invocation(before: Snapshot, after: Snapshot) -> Invocation {
        Invocation {
            part: "int".into(),
            before,
            after,
        }
    }

    #[rstest]
    #[case::untouched(known(), known(), 0)]
    #[case::two_pools(
        Snapshot { pool: "/a".into(), ..known() },
        Snapshot { pool: "/b".into(), ..known() },
        1
    )]
    #[case::activity_created(known(), Snapshot { wtpool_activity: Some(vec![]), ..known() }, 1)]
    #[case::record_about_the_run(
        Snapshot { wtpool_activity: Some(vec![]), ..known() },
        Snapshot { wtpool_activity: Some(vec![entry("s.json", true)]), ..known() },
        1
    )]
    #[case::record_of_owner(
        Snapshot { wtpool_activity: Some(vec![]), ..known() },
        Snapshot { wtpool_activity: Some(vec![entry("s.json", false)]), ..known() },
        0
    )]
    #[case::sandbox_worktree(
        Snapshot { worktrees: vec!["/run/sandbox/x".into()], ..known() },
        Snapshot { worktrees: vec!["/run/sandbox/x".into()], ..known() },
        1
    )]
    #[case::new_worktree_of_owner(
        known(),
        Snapshot { worktrees: vec!["/elsewhere".into()], ..known() },
        0
    )]
    #[case::worktree_at_a_verify_commit(
        known(),
        Snapshot {
            worktrees: vec!["/elsewhere".into()],
            verify_worktrees: vec!["/elsewhere".into()],
            ..known()
        },
        1
    )]
    #[case::listing_names_the_run(known(), Snapshot { listing_mentions_run: true, ..known() }, 1)]
    #[case::listing_named_the_run_before_too(
        Snapshot { listing_mentions_run: true, ..known() },
        Snapshot { listing_mentions_run: true, ..known() },
        1
    )]
    #[case::listing_cleared_during_the_part(
        Snapshot { listing_mentions_run: true, ..known() },
        known(),
        0
    )]
    #[case::worktree_removed(
        Snapshot { worktrees: vec!["/pool".into(), "/pool-wt1".into()], ..known() },
        Snapshot { worktrees: vec!["/pool".into()], ..known() },
        1
    )]
    #[case::worktree_moved(
        Snapshot { worktrees: vec!["/pool".into(), "/pool-wt1".into()], ..known() },
        Snapshot { worktrees: vec!["/pool".into(), "/pool-wt9".into()], ..known() },
        1
    )]
    #[case::worktree_folder_deleted(
        Snapshot { worktrees: vec!["/pool".into(), "/pool-wt1".into()], ..known() },
        Snapshot {
            worktrees: vec!["/pool".into(), "/pool-wt1".into()],
            missing_folders: vec!["/pool-wt1".into()],
            ..known()
        },
        1
    )]
    #[case::worktree_folder_gone_before_the_run(
        Snapshot {
            worktrees: vec!["/pool".into(), "/pool-wt1".into()],
            missing_folders: vec!["/pool-wt1".into()],
            ..known()
        },
        Snapshot {
            worktrees: vec!["/pool".into(), "/pool-wt1".into()],
            missing_folders: vec!["/pool-wt1".into()],
            ..known()
        },
        0
    )]
    #[case::new_worktree_of_owner_without_folder(
        known(),
        Snapshot {
            worktrees: vec!["/elsewhere".into()],
            missing_folders: vec!["/elsewhere".into()],
            ..known()
        },
        0
    )]
    #[case::fallback_identity_unknown(known(), Snapshot { fallback_email: None, ..known() }, 1)]
    #[case::head_moved_by_verify(
        known(),
        Snapshot { head_entries: moves("/elsewhere", &[line(VERIFY_EMAIL, 1)]), ..known() },
        1
    )]
    #[case::head_moved_by_fallback(
        known(),
        Snapshot { head_entries: moves("/elsewhere", &[line(FALLBACK, 1)]), ..known() },
        1
    )]
    #[case::head_moved_by_owner(
        known(),
        Snapshot {
            head_entries: moves("/elsewhere", &[line("owner@example.com", 1)]),
            ..known()
        },
        0
    )]
    #[case::head_moved_by_throwaway(
        known(),
        Snapshot { head_entries: moves("/elsewhere", &[line("t@t", 1)]), ..known() },
        0
    )]
    #[case::head_moved_before_the_run(
        Snapshot { head_entries: moves("/elsewhere", &[line(VERIFY_EMAIL, 1)]), ..known() },
        Snapshot { head_entries: moves("/elsewhere", &[line(VERIFY_EMAIL, 1)]), ..known() },
        0
    )]
    #[case::head_moved_again(
        Snapshot { head_entries: moves("/elsewhere", &[line(VERIFY_EMAIL, 1)]), ..known() },
        Snapshot {
            head_entries: moves("/elsewhere", &[line(VERIFY_EMAIL, 1), line(VERIFY_EMAIL, 2)]),
            ..known()
        },
        1
    )]
    #[case::earlier_entry_expired(
        Snapshot { head_entries: moves("/elsewhere", &[line(VERIFY_EMAIL, 1)]), ..known() },
        known(),
        0
    )]
    #[case::expired_and_moved(
        Snapshot { head_entries: moves("/elsewhere", &[line(VERIFY_EMAIL, 1)]), ..known() },
        Snapshot { head_entries: moves("/elsewhere", &[line(VERIFY_EMAIL, 2)]), ..known() },
        1
    )]
    #[case::wtpool_toml_changed(
        Snapshot { wtpool_toml: Some("a".into()), ..known() },
        Snapshot { wtpool_toml: Some("b".into()), ..known() },
        1
    )]
    #[case::settings_mention(known(), Snapshot { settings_mention_wtpool_verify: true, ..known() }, 1)]
    #[case::extension_added(
        known(),
        Snapshot { vscode_extensions: vec!["haiyuechen-nearone.wtpool-0.1.0".into()], ..known() },
        1
    )]
    fn traces__should_report_each_left_trace(
        #[case] before: Snapshot,
        #[case] after: Snapshot,
        #[case] expected: usize,
    ) {
        // Given
        let invocation = invocation(before, after);

        // When
        let found = traces(&invocation, &["/run".to_owned()]);

        // Then
        assert_eq!(found.len(), expected, "{found:?}");
    }

    #[test]
    fn traces__should_name_the_settings_file_it_read() {
        // Given
        let after = Snapshot {
            settings: "/c/claude/settings.json".into(),
            settings_mention_wtpool_verify: true,
            ..known()
        };

        // When
        let found = traces(&invocation(known(), after), &[]);

        // Then
        assert_eq!(
            found,
            ["int: /c/claude/settings.json mentions wtpool-verify"]
        );
    }

    fn must_record(run: &Path, name: &str, after: Snapshot) {
        let dir = run.join("isolation");
        fs::create_dir_all(&dir).unwrap();
        let invocation = Invocation {
            part: name.split('-').next().unwrap().into(),
            before: after.clone(),
            after,
        };
        fs::write(
            dir.join(format!("{name}.json")),
            serde_json::to_string(&invocation).unwrap(),
        )
        .unwrap();
    }

    fn pool(worktrees: &[&str]) -> Snapshot {
        Snapshot {
            worktrees: worktrees.iter().map(|w| (*w).to_owned()).collect(),
            ..known()
        }
    }

    #[rstest]
    #[case::unchanged_since_the_last_part(pool(&["/pool", "/pool-wt1"]), crate::results::Outcome::Pass)]
    #[case::changed_after_the_last_part(pool(&["/pool"]), crate::results::Outcome::Fail)]
    fn check__should_compare_the_last_snapshot_with_the_newest_part(
        #[case] last: Snapshot,
        #[case] expected: crate::results::Outcome,
    ) {
        // Given
        let run = std::env::temp_dir().join(format!(
            "xtask-final-{}-{}",
            std::process::id(),
            last.worktrees.len()
        ));
        let _ = fs::remove_dir_all(&run);
        must_record(&run, "quick-300", pool(&["/pool"]));
        must_record(&run, "int-1000", pool(&["/pool", "/pool-wt1"]));
        let places = Places {
            home: run.join("home"),
            pool: Some(run.join("no-pool")),
            pool_source: "test".to_owned(),
            extension_ids: Vec::new(),
        };

        // When
        let result = check(&run, &places, &[run.to_string_lossy().into_owned()], &last);

        // Then
        let _ = fs::remove_dir_all(&run);
        assert_eq!(result.outcome, expected, "{}", result.message);
    }

    #[rstest]
    #[case::unset(None, "no real pool: WTPOOL_FOREGROUND is not set")]
    #[case::no_worktree(Some("no-pool"), "no worktree at")]
    fn check__should_say_why_it_skipped_the_pool_rules(
        #[case] pool: Option<&str>,
        #[case] note: &str,
    ) {
        // Given
        let run = std::env::temp_dir().join(format!(
            "xtask-no-pool-{}-{}",
            std::process::id(),
            pool.is_some()
        ));
        let _ = fs::remove_dir_all(&run);
        must_record(&run, "quick-300", known());
        let places = Places {
            home: run.join("home"),
            pool: pool.map(|pool| run.join(pool)),
            pool_source: if pool.is_some() {
                "WTPOOL_FOREGROUND".to_owned()
            } else {
                "WTPOOL_FOREGROUND is not set".to_owned()
            },
            extension_ids: Vec::new(),
        };

        // When
        let result = check(
            &run,
            &places,
            &[run.to_string_lossy().into_owned()],
            &known(),
        );

        // Then
        let _ = fs::remove_dir_all(&run);
        assert_eq!(
            result.outcome,
            crate::results::Outcome::Pass,
            "{}",
            result.message
        );
        assert!(result.message.contains(note), "{}", result.message);
        assert!(
            result.message.contains("pool rules were skipped"),
            "{}",
            result.message
        );
    }
}
