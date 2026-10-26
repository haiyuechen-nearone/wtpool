//! Reads the real places a run must not touch: the real pool, its git state and activity, the
//! Claude Code settings and the installed extensions.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::VERIFY_EMAIL;
use crate::cmd::{self, git};
use crate::repo::entry_hash;

/// The real places a run must not touch, read before and after each part.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// The foreground of the real pool this snapshot read ([`Places::real`]).
    #[serde(default)]
    pub pool: String,
    /// True when that foreground is a git worktree. Without one, the pool rules have nothing to
    /// read.
    #[serde(default)]
    pub pool_found: bool,
    pub wtpool_activity: Option<Vec<ActivityEntry>>,
    pub worktrees: Vec<String>,
    /// True when any line of `git worktree list --porcelain`, a lock reason included, names the
    /// run folder.
    #[serde(default)]
    pub listing_mentions_run: bool,
    /// Listed worktrees whose folder, or the `.git` link in it, is gone. git keeps listing such a
    /// worktree, marked prunable.
    #[serde(default)]
    pub missing_folders: Vec<String>,
    /// Worktrees of the real pool whose `HEAD` is a commit by wtpool-verify.
    #[serde(default)]
    pub verify_worktrees: Vec<String>,
    /// The address git names when neither configuration nor environment does, read with this
    /// snapshot.
    #[serde(default)]
    pub fallback_email: Option<String>,
    /// Per worktree of the real pool, the main one included, its `HEAD` reflog entries.
    #[serde(default)]
    pub head_entries: BTreeMap<String, Vec<String>>,
    pub wtpool_toml: Option<String>,
    /// The Claude Code settings file this snapshot read, as `claude_settings` names it.
    #[serde(default)]
    pub settings: String,
    pub settings_mention_wtpool_verify: bool,
    /// Folders of the product's extension in `~/.vscode/extensions/`.
    pub vscode_extensions: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ActivityEntry {
    pub name: String,
    pub hash: String,
    pub mentions_run: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Invocation {
    pub part: String,
    pub before: Snapshot,
    pub after: Snapshot,
}

pub struct Places {
    pub home: PathBuf,
    /// The foreground of the real pool, when `WTPOOL_FOREGROUND` names one.
    pub pool: Option<PathBuf>,
    /// Where [`pool`](Self::pool) came from, or the reason there is none, for the report.
    pub pool_source: String,
    /// `<publisher>.<name>` of the product's extension, lower case, when `vscode/package.json`
    /// names one.
    pub extension_ids: Vec<String>,
}

/// The variable that names the foreground of the pool wtpool works on.
const FOREGROUND_VARIABLE: &str = "WTPOOL_FOREGROUND";

impl Places {
    /// The user's real places. The pool is the foreground `WTPOOL_FOREGROUND` names, as for
    /// wtpool, and none without it, as wtpool has no default pool. Inside a run's sandbox
    /// (`WTPOOL_VERIFY_SANDBOX`), `WTPOOL_FOREGROUND` names the sandbox pool, never a real one.
    pub fn real() -> Self {
        let home = crate::dirs::home().unwrap_or_default();
        let in_sandbox = std::env::var_os("WTPOOL_VERIFY_SANDBOX").is_some();
        let named = std::env::var_os(FOREGROUND_VARIABLE).filter(|v| !v.is_empty());
        let (pool, pool_source) = match named {
            Some(_) if in_sandbox => (
                None,
                format!("{FOREGROUND_VARIABLE} names the sandbox pool inside a run"),
            ),
            Some(named) => (Some(PathBuf::from(named)), FOREGROUND_VARIABLE.to_owned()),
            None => (None, format!("{FOREGROUND_VARIABLE} is not set")),
        };
        Self {
            home,
            pool,
            pool_source,
            extension_ids: Vec::new(),
        }
    }

    pub fn with_extension_of(mut self, root: &Path) -> Self {
        self.extension_ids = extension_id(root).into_iter().collect();
        self
    }
}

/// The id VS Code gives the extension that `vscode/package.json` in `root` describes.
pub fn extension_id(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("vscode/package.json")).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&text).ok()?;
    let publisher = manifest.get("publisher")?.as_str()?;
    let name = manifest.get("name")?.as_str()?;
    Some(format!("{publisher}.{name}").to_lowercase())
}

/// True when an `~/.vscode/extensions/` folder holds a version of one of `ids`.
pub fn is_extension_folder(folder: &str, ids: &[String]) -> bool {
    let folder = folder.to_lowercase();
    ids.iter().any(|id| {
        folder == *id
            || folder
                .strip_prefix(id.as_str())
                .is_some_and(|rest| rest.starts_with('-'))
    })
}

pub fn snapshot(places: &Places, run_dir: &Path) -> Snapshot {
    let run_text = run_dir.to_string_lossy().into_owned();
    let pool = places.pool.as_deref();
    let wtpool_activity =
        pool.and_then(|pool| fs::read_dir(pool.join(".git/wtpool-activity")).ok());
    let wtpool_activity = wtpool_activity.map(|entries| {
        let mut found: Vec<ActivityEntry> = entries
            .flatten()
            .map(|entry| {
                let path = entry.path();
                let text = fs::read(&path).unwrap_or_default();
                ActivityEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    hash: entry_hash(&path),
                    mentions_run: mentions(&text, &run_text),
                }
            })
            .collect();
        found.sort();
        found
    });
    // A folder that is no worktree may still lie inside another repository, and git would read
    // that one.
    let pool = pool.filter(|pool| pool.join(".git").exists());
    let pool_found = pool.is_some();
    let listing = pool
        .and_then(|pool| git(pool, &["worktree", "list", "--porcelain"]).ok())
        .unwrap_or_default();
    let listing_mentions_run = mentions(listing.as_bytes(), &run_text);
    let mut worktrees = Vec::new();
    let mut missing_folders = Vec::new();
    let mut verify_worktrees = Vec::new();
    for block in listing.split("\n\n") {
        let field = |name: &str| {
            block
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .map(str::to_owned)
        };
        let Some(path) = field("worktree ") else {
            continue;
        };
        if field("HEAD ").is_some_and(|head| pool.is_some_and(|pool| is_verify_commit(pool, &head)))
        {
            verify_worktrees.push(path.clone());
        }
        let folder_gone = field("prunable").is_some()
            || fs::symlink_metadata(Path::new(&path).join(".git")).is_err();
        if folder_gone {
            missing_folders.push(path.clone());
        }
        worktrees.push(path);
    }
    let wtpool_toml = pool
        .map(|pool| pool.join(".git/wtpool.toml"))
        .and_then(|path| fs::symlink_metadata(&path).ok().map(|_| entry_hash(&path)));
    let settings = claude_settings(std::env::var_os("CLAUDE_CONFIG_DIR"), &places.home);
    let settings_mention_wtpool_verify = fs::read(&settings)
        .map(|bytes| mentions(&bytes, "wtpool-verify"))
        .unwrap_or(false);
    let mut vscode_extensions: Vec<String> = fs::read_dir(places.home.join(".vscode/extensions"))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|folder| is_extension_folder(folder, &places.extension_ids))
                .collect()
        })
        .unwrap_or_default();
    vscode_extensions.sort();
    Snapshot {
        pool: places
            .pool
            .as_deref()
            .map(|pool| pool.to_string_lossy().into_owned())
            .unwrap_or_default(),
        pool_found,
        wtpool_activity,
        worktrees,
        listing_mentions_run,
        missing_folders,
        verify_worktrees,
        fallback_email: fallback_email(),
        head_entries: pool.map(head_entries).unwrap_or_default(),
        wtpool_toml,
        settings: settings.to_string_lossy().into_owned(),
        settings_mention_wtpool_verify,
        vscode_extensions,
    }
}

/// The address git falls back to from the account and host name, as a process of the run with a
/// cleared environment writes it into the real pool, whose config names no user.
pub fn fallback_email() -> Option<String> {
    let ident = cmd::output(
        Command::new("git")
            .args(["var", "GIT_COMMITTER_IDENT"])
            .current_dir("/")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null"),
        Duration::from_secs(120),
    )
    .ok()?;
    reflog_email(ident.trim())
        .filter(|email| !email.is_empty())
        .map(str::to_owned)
}

/// The `HEAD` reflog entries of each worktree of `pool`, the main one included. Worktrees without
/// a reflog are left out.
pub fn head_entries(pool: &Path) -> BTreeMap<String, Vec<String>> {
    let git_dir = pool.join(".git");
    let mut logs = vec![(
        pool.to_string_lossy().into_owned(),
        git_dir.join("logs/HEAD"),
    )];
    if let Ok(entries) = fs::read_dir(git_dir.join("worktrees")) {
        for entry in entries.flatten() {
            let admin = entry.path();
            let worktree = fs::read_to_string(admin.join("gitdir"))
                .map(|text| text.trim().trim_end_matches("/.git").to_owned())
                .unwrap_or_else(|_| admin.to_string_lossy().into_owned());
            logs.push((worktree, admin.join("logs/HEAD")));
        }
    }
    logs.into_iter()
        .filter_map(|(worktree, log)| {
            let text = fs::read(&log).ok()?;
            let entries: Vec<String> = String::from_utf8_lossy(&text)
                .lines()
                .map(str::to_owned)
                .collect();
            Some((worktree, entries))
        })
        .collect()
}

/// The address in a reflog line `<old> <new> <name> <<email>> <time> <zone>\t<message>`, or in
/// an identity `<name> <<email>> <time> <zone>`.
pub fn reflog_email(line: &str) -> Option<&str> {
    let identity = line.split('\t').next()?;
    let end = identity.rfind('>')?;
    let start = identity.get(..end)?.rfind('<')?;
    identity.get(start.checked_add(1)?..end)
}

/// Claude Code's user settings file, as `wtpool hook install` names it: `settings.json` in
/// `config_dir`, the value of `CLAUDE_CONFIG_DIR`, else `~/.claude/settings.json`. An empty
/// `CLAUDE_CONFIG_DIR` counts as unset.
fn claude_settings(config_dir: Option<OsString>, home: &Path) -> PathBuf {
    match config_dir.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir).join("settings.json"),
        None => home.join(".claude/settings.json"),
    }
}

fn is_verify_commit(pool: &Path, commit: &str) -> bool {
    git(pool, &["log", "-1", "--format=%ae %ce", commit, "--"])
        .is_ok_and(|emails| emails.contains(VERIFY_EMAIL))
}

fn mentions(haystack: &[u8], needle: &str) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::isolated_git;
    use rstest::rstest;

    #[rstest]
    #[case::config_dir(Some("/c/claude"), "/c/claude/settings.json")]
    #[case::unset(None, "/h/.claude/settings.json")]
    #[case::empty(Some(""), "/h/.claude/settings.json")]
    fn claude_settings__should_follow_claude_config_dir_else_the_home_folder(
        #[case] config_dir: Option<&str>,
        #[case] expected: &str,
    ) {
        // When
        let settings = claude_settings(config_dir.map(OsString::from), Path::new("/h"));

        // Then
        assert_eq!(settings, PathBuf::from(expected));
    }

    #[rstest]
    #[case::versioned_folder("haiyuechen-nearone.wtpool-0.1.0", true)]
    #[case::other_case("HAIYUECHEN-NEARONE.wtpool-0.2.0-darwin-arm64", true)]
    #[case::bare_id("haiyuechen-nearone.wtpool", true)]
    #[case::longer_name("haiyuechen-nearone.wtpool-extra", true)]
    #[case::name_prefix("haiyuechen-nearone.wtpoolbar-0.1.0", false)]
    #[case::other_extension("rust-lang.rust-analyzer-0.3.2", false)]
    fn is_extension_folder__should_match_only_the_product(
        #[case] folder: &str,
        #[case] matched: bool,
    ) {
        // Given
        let ids = vec!["haiyuechen-nearone.wtpool".to_owned()];

        // When
        let result = is_extension_folder(folder, &ids);

        // Then
        assert_eq!(result, matched);
    }

    /// A pool like the real one: commits by `h`, no `user.*` in its config.
    fn must_pool(name: &str) -> PathBuf {
        let pool = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("xtask-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&pool);
        fs::create_dir_all(&pool).unwrap();
        for args in [
            &["init", "--quiet"][..],
            &[
                "-c",
                "user.name=h",
                "-c",
                "user.email=h@example.invalid",
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "a",
            ],
        ] {
            let status = isolated_git(&pool).args(args).status().unwrap();
            assert!(status.success(), "git {args:?}");
        }
        pool
    }

    #[test]
    fn fallback_email__should_name_the_identity_of_a_cleared_environment() {
        // Given
        let pool = must_pool("fallback");
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&pool)
            .args(["checkout", "--quiet", "--detach"])
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .status()
            .unwrap();
        assert!(status.success());
        let log = fs::read_to_string(pool.join(".git/logs/HEAD")).unwrap();

        // When
        let email = fallback_email();

        // Then
        let _ = fs::remove_dir_all(&pool);
        let written = log.lines().last().and_then(reflog_email);
        assert!(email.is_some());
        assert_eq!(email.as_deref(), written);
    }

    #[test]
    fn head_entries__should_read_the_reflog_of_each_worktree() {
        // Given
        let pool = must_pool("heads");
        let slot = pool.with_extension("wt1");
        let slot_text = slot.to_string_lossy().into_owned();
        let status = isolated_git(&pool)
            .args(["worktree", "add", "--quiet", "--detach", &slot_text])
            .env("EMAIL", VERIFY_EMAIL)
            .status()
            .unwrap();
        assert!(status.success());

        // When
        let found = head_entries(&pool);

        // Then
        let _ = fs::remove_dir_all(&pool);
        let _ = fs::remove_dir_all(&slot);
        let emails: Vec<(&str, Vec<&str>)> = found
            .iter()
            .map(|(worktree, entries)| {
                (
                    worktree.as_str(),
                    entries.iter().filter_map(|e| reflog_email(e)).collect(),
                )
            })
            .collect();
        assert_eq!(
            emails,
            vec![
                (pool.to_string_lossy().as_ref(), vec!["h@example.invalid"]),
                (slot_text.as_str(), vec![VERIFY_EMAIL]),
            ]
        );
    }

    #[rstest]
    #[case::folder_deleted(true, false)]
    #[case::git_link_deleted(false, true)]
    #[case::untouched(false, false)]
    fn snapshot__should_record_each_worktree_whose_folder_is_gone(
        #[case] delete_folder: bool,
        #[case] delete_git_link: bool,
    ) {
        // Given
        let pool = must_pool(&format!("folders-{delete_folder}-{delete_git_link}"));
        let slot = pool.with_extension("wt1");
        let slot_text = slot.to_string_lossy().into_owned();
        let status = isolated_git(&pool)
            .args(["worktree", "add", "--quiet", "--detach", &slot_text])
            .status()
            .unwrap();
        assert!(status.success());
        if delete_folder {
            fs::remove_dir_all(&slot).unwrap();
        }
        if delete_git_link {
            fs::remove_file(slot.join(".git")).unwrap();
        }
        let places = Places {
            home: pool.join("home"),
            pool: Some(pool.clone()),
            pool_source: "test".to_owned(),
            extension_ids: Vec::new(),
        };

        // When
        let found = snapshot(&places, &pool.join("run"));

        // Then
        let _ = fs::remove_dir_all(&pool);
        let _ = fs::remove_dir_all(&slot);
        let pool_text = pool.to_string_lossy().into_owned();
        assert_eq!(found.worktrees, vec![pool_text, slot_text.clone()]);
        let expected = if delete_folder || delete_git_link {
            vec![slot_text]
        } else {
            Vec::new()
        };
        assert_eq!(found.missing_folders, expected);
    }

    #[derive(Debug, Clone, Copy)]
    enum SlotLock {
        None,
        NamingTheRun,
        OfOwner,
    }

    #[rstest]
    #[case::untouched(false, SlotLock::None, false)]
    #[case::lock_naming_the_run(false, SlotLock::NamingTheRun, true)]
    #[case::lock_of_owner(false, SlotLock::OfOwner, false)]
    #[case::worktree_in_the_run(true, SlotLock::None, true)]
    fn snapshot__should_record_whether_the_listing_names_the_run_folder(
        #[case] slot_in_run: bool,
        #[case] lock: SlotLock,
        #[case] expected: bool,
    ) {
        // Given
        let pool = must_pool(&format!("listing-{slot_in_run}-{lock:?}"));
        let run = pool.with_extension("run");
        let slot = if slot_in_run {
            run.join("sandbox/pool/wt1")
        } else {
            pool.with_extension("wt1")
        };
        let slot_text = slot.to_string_lossy().into_owned();
        let run_foreground = run.join("sandbox/pool/fg").to_string_lossy().into_owned();
        let reason = match lock {
            SlotLock::None => None,
            SlotLock::NamingTheRun => Some(run_foreground.as_str()),
            SlotLock::OfOwner => Some("the owner keeps it"),
        };
        let mut commands = vec![vec!["worktree", "add", "--quiet", "--detach", &slot_text]];
        if let Some(reason) = reason {
            commands.push(vec!["worktree", "lock", "--reason", reason, &slot_text]);
        }
        for args in commands {
            let status = isolated_git(&pool).args(&args).status().unwrap();
            assert!(status.success(), "git {args:?}");
        }
        let places = Places {
            home: pool.join("home"),
            pool: Some(pool.clone()),
            pool_source: "test".to_owned(),
            extension_ids: Vec::new(),
        };

        // When
        let found = snapshot(&places, &run);

        // Then
        let _ = fs::remove_dir_all(&pool);
        let _ = fs::remove_dir_all(&slot);
        let _ = fs::remove_dir_all(&run);
        assert_eq!(found.listing_mentions_run, expected);
    }

    #[rstest]
    #[case::configured(
        "0000000000000000000000000000000000000000 7a92ff4 Pool Owner <h@example.invalid> 1792788089 +0200\tcheckout: x",
        Some("h@example.invalid")
    )]
    #[case::passwd_fallback(
        "56aebac 7a92ff4 Pool Owner <you@mac.example> 1792787786 +0200\tcheckout: moving to <b@c>",
        Some("you@mac.example")
    )]
    #[case::identity(
        "Pool Owner <you@mac.example> 1792790056 +0200",
        Some("you@mac.example")
    )]
    #[case::empty_address("a b n <> 1 +0000\tm", Some(""))]
    #[case::no_identity("a b garbage", None)]
    fn reflog_email__should_read_the_identity_before_the_message(
        #[case] line: &str,
        #[case] expected: Option<&str>,
    ) {
        // Given
        let entry = line;

        // When
        let email = reflog_email(entry);

        // Then
        assert_eq!(email, expected);
    }
}
