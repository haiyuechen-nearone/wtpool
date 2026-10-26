//! The settings of heavy jobs as the checks read and write them: `machine.toml`, the repository's
//! `wtpool.toml`, `wtpool job <setting>` runs and the files and modes they leave.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use xtask::pool::{Pool, PoolSpec, WorktreeSpec};

use super::{describe, wtpool};
use crate::support::{FileState, Sandbox, file_state};

/// A pool of the main worktree `fg` alone, on `main`: no slot is needed to read settings.
pub fn must_lone_pool(sandbox: &Sandbox) -> Pool {
    let spec = PoolSpec {
        agents: true,
        worktrees: BTreeMap::from([(
            "fg".to_owned(),
            WorktreeSpec {
                branch: Some("main".into()),
                ..WorktreeSpec::default()
            },
        )]),
    };
    sandbox.must_pool(&spec)
}

/// The words of `text`: runs of letters, digits and `_ . - : * +`.
pub fn tokens(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '.' | '-' | ':' | '*' | '+')))
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// True when `shown` names `expected` as written or with the symlinks of its parent resolved.
pub fn same_path(shown: &str, expected: &Path) -> bool {
    let shown = Path::new(shown);
    if shown == expected {
        return true;
    }
    let canonical = |path: &Path| -> Option<PathBuf> {
        let parent = fs::canonicalize(path.parent()?).ok()?;
        Some(parent.join(path.file_name()?))
    };
    canonical(shown).is_some_and(|found| Some(found) == canonical(expected))
}

/// True when `text` holds `path` as written or with the symlinks of its parent resolved.
pub fn names_path(text: &str, path: &Path) -> bool {
    let written = path.to_string_lossy();
    let resolved = path
        .parent()
        .and_then(|parent| fs::canonicalize(parent).ok())
        .zip(path.file_name())
        .map(|(parent, name)| parent.join(name).to_string_lossy().into_owned());
    text.contains(&*written) || resolved.is_some_and(|resolved| text.contains(&resolved))
}

pub fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Sets the Unix mode of `path`. Off Unix the setup fails, as Windows has no such mode.
pub fn set_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("setup: chmod");
    }
    #[cfg(not(unix))]
    panic!(
        "setup: mode {mode:o} of {} needs Unix permissions",
        path.display()
    );
}

/// The permission bits of `path`, without following a symlink. `None` off Unix.
pub fn mode_of(path: &Path) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        fs::symlink_metadata(path)
            .ok()
            .map(|meta| meta.mode() & 0o777)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

/// The bytes and state of every entry below `dir`, symlinks not followed, `skip` left out.
pub fn snapshot(dir: &Path, skip: &[&Path]) -> BTreeMap<PathBuf, (FileState, Vec<u8>)> {
    let mut found = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if skip.iter().any(|left_out| path.starts_with(left_out)) {
                continue;
            }
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            let bytes = if meta.is_file() {
                fs::read(&path).unwrap_or_default()
            } else {
                Vec::new()
            };
            if meta.is_dir() {
                stack.push(path.clone());
            }
            found.insert(path, (file_state(&meta), bytes));
        }
    }
    found
}

/// The `[jobs]` table of the TOML file at `path`, `None` when it is missing or not TOML.
pub fn jobs_table(path: &Path) -> Option<toml::Table> {
    let text = fs::read_to_string(path).ok()?;
    let table: toml::Table = toml::from_str(&text).ok()?;
    table.get("jobs")?.as_table().cloned()
}

const CORES: (&str, &str) = ("WTPOOL_VERIFY_CORES", "16");

/// `wtpool job <args>` from `cwd` on 16 cores, so the default limit is 4.
pub fn job(sandbox: &Sandbox, cwd: &Path, args: &[&str]) -> Output {
    let mut all = vec!["job"];
    all.extend_from_slice(args);
    wtpool(sandbox, cwd, &all, &[(CORES.0, OsStr::new(CORES.1))])
}

/// The bytes and state of `path`, `None` when it is missing.
pub fn file(path: &Path) -> Option<(Vec<u8>, FileState)> {
    let meta = fs::symlink_metadata(path).ok()?;
    Some((fs::read(path).unwrap_or_default(), file_state(&meta)))
}

/// The top level table of the TOML file at `path`.
pub fn table(path: &Path) -> Option<toml::Table> {
    toml::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// The strings of the array `key` of `table`, as a set.
pub fn strings(table: Option<&toml::Table>, key: &str) -> BTreeSet<String> {
    table
        .and_then(|t| t.get(key))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// Notes a run that must exit 125 and leave `path` as it was.
pub fn refused(
    wrong: &mut Vec<String>,
    what: &str,
    output: &Output,
    path: &Path,
    before: &Option<(Vec<u8>, FileState)>,
) {
    if output.status.code() != Some(125) {
        wrong.push(format!("{what}: not exit 125: {}", describe(output)));
    }
    if file(path) != *before {
        wrong.push(format!("{what}: changed {}", path.display()));
    }
}

pub fn succeeded(wrong: &mut Vec<String>, what: &str, output: &Output) -> bool {
    let ok = output.status.success();
    if !ok {
        wrong.push(format!("{what}: {}", describe(output)));
    }
    ok
}

/// Appends `text` to the repository's `<common>/wtpool.toml`.
pub fn add_to_repository(common: &Path, text: &str) {
    let path = common.join("wtpool.toml");
    let mut config = fs::read_to_string(&path).expect("setup: read wtpool.toml");
    config.push_str(text);
    fs::write(&path, config).expect("setup: write wtpool.toml");
}
