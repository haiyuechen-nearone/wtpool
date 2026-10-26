//! Sandbox pools of the scenario catalog: a foreground `fg`, slots `wt1` to `wt3` and unmanaged
//! worktrees, built from the `pool` object of a scenario file. Every repository lives in the pool
//! folder, and `origin` is a bare repository there too.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

use crate::cmd;
use crate::isolation::VERIFY_EMAIL;

/// The `pool` object of a scenario file.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PoolSpec {
    /// True when `<common>/wtpool-activity/` exists, holding the sandbox marker.
    pub agents: bool,
    pub worktrees: BTreeMap<String, WorktreeSpec>,
}

/// The state one worktree starts in. `fg` is the main worktree, `wt<N>` a slot, any other name an
/// unmanaged worktree.
#[derive(Deserialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields, default)]
pub struct WorktreeSpec {
    pub branch: Option<String>,
    pub detached: bool,
    /// Pushes the branch to `origin` and tracks it.
    pub upstream: bool,
    pub ahead: u32,
    pub behind: u32,
    /// The number of commits on a detached `HEAD` that no ref holds.
    pub unreferenced: u32,
    /// The number of untracked files.
    pub dirty: u32,
    /// `bisect`, `rebase` or `merge`, the last two stopped at a conflict.
    pub operation: Option<String>,
    pub unreadable_index: bool,
    /// The folder is deleted after the pool is built, so git still lists the worktree.
    pub missing: bool,
}

pub struct Pool {
    /// The folder that holds every repository of the pool, canonical.
    pub dir: PathBuf,
    pub fg: PathBuf,
    pub common: PathBuf,
    pub origin: PathBuf,
}

impl Pool {
    /// The pool [`build`] made in `dir` earlier.
    pub fn at(dir: &Path) -> Result<Self, String> {
        let dir = fs::canonicalize(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let fg = dir.join("fg");
        Ok(Self {
            common: fg.join(".git"),
            origin: dir.join("origin.git"),
            fg,
            dir,
        })
    }

    pub fn worktree(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// The git folder of a worktree: the common folder for `fg`, else its admin folder.
    pub fn git_dir(&self, name: &str) -> PathBuf {
        if name == "fg" {
            self.common.clone()
        } else {
            self.common.join("worktrees").join(name)
        }
    }

    pub fn activity(&self) -> PathBuf {
        self.common.join("wtpool-activity")
    }

    /// Runs git in a worktree of the pool and returns its stdout.
    pub fn git(&self, name: &str, args: &[&str]) -> Result<String, String> {
        git_in(&self.worktree(name), args)
    }
}

/// A git command in `dir` with the user's configuration hidden, as the sandbox does.
pub fn git_command(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "wtpool-verify")
        .env("GIT_AUTHOR_EMAIL", VERIFY_EMAIL)
        .env("GIT_COMMITTER_NAME", "wtpool-verify")
        .env("GIT_COMMITTER_EMAIL", VERIFY_EMAIL);
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        command.env_remove(name);
    }
    command
}

pub fn git_in(dir: &Path, args: &[&str]) -> Result<String, String> {
    cmd::output(git_command(dir).args(args), Duration::from_secs(120))
}

/// Runs git and returns true when it exited 0, for commands that stop at a conflict on purpose.
fn git_status(dir: &Path, args: &[&str]) -> Result<bool, String> {
    let ran = cmd::run(git_command(dir).args(args), Duration::from_secs(120))?;
    Ok(ran.ok())
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The commit that `origin`'s copy of a branch moves to, made without touching any worktree.
fn commit_on(dir: &Path, parent: &str, message: &str) -> Result<String, String> {
    let tree = git_in(dir, &["rev-parse", &format!("{parent}^{{tree}}")])?;
    let commit = git_in(
        dir,
        &["commit-tree", tree.trim(), "-p", parent, "-m", message],
    )?;
    Ok(commit.trim().to_owned())
}

/// A commit on `main` that changes `f.txt`, so a worktree that changed it too conflicts.
fn conflicting_commit(fg: &Path) -> Result<String, String> {
    let file = fg.join(".git/wtpool-verify-theirs.txt");
    write(&file, "theirs\n")?;
    let blob = git_in(fg, &["hash-object", "-w", &path_text(&file)])?;
    let _ = fs::remove_file(&file);
    let index = fg.join(".git/wtpool-verify-index");
    let with_index = |args: &[&str]| {
        cmd::output(
            git_command(fg).args(args).env("GIT_INDEX_FILE", &index),
            Duration::from_secs(30),
        )
    };
    with_index(&["read-tree", "main"])?;
    with_index(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("100644,{},f.txt", blob.trim()),
    ])?;
    let tree = with_index(&["write-tree"])?;
    let _ = fs::remove_file(&index);
    let commit = git_in(
        fg,
        &["commit-tree", tree.trim(), "-p", "main", "-m", "theirs"],
    )?;
    Ok(commit.trim().to_owned())
}

/// Builds the pool `spec` describes in `dir`. The folder must be new or empty.
pub fn build(dir: &Path, spec: &PoolSpec) -> Result<Pool, String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let dir = fs::canonicalize(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let origin = dir.join("origin.git");
    let fg = dir.join("fg");
    git_in(
        &dir,
        &[
            "init",
            "--quiet",
            "--bare",
            "-b",
            "main",
            &path_text(&origin),
        ],
    )?;
    git_in(&dir, &["init", "--quiet", "-b", "main", &path_text(&fg)])?;
    for (key, value) in [
        ("user.name", "wtpool-verify"),
        ("user.email", VERIFY_EMAIL),
        ("commit.gpgsign", "false"),
        ("tag.gpgsign", "false"),
        ("advice.detachedHead", "false"),
        ("init.defaultBranch", "main"),
        ("merge.conflictStyle", "merge"),
        ("pull.rebase", "false"),
    ] {
        git_in(&fg, &["config", key, value])?;
    }
    let common = fg.join(".git");
    write(&common.join("info/exclude"), ".envrc\n")?;
    write(
        &common.join("wtpool.toml"),
        "slot_dir = \".\"\nslot_prefix = \"wt\"\nslot_count = 3\n",
    )?;
    write(&fg.join("f.txt"), "base\n")?;
    git_in(&fg, &["add", "f.txt"])?;
    git_in(&fg, &["commit", "--quiet", "-m", "base"])?;
    git_in(&fg, &["remote", "add", "origin", &path_text(&origin)])?;
    git_in(&fg, &["push", "--quiet", "-u", "origin", "main"])?;
    let pool = Pool {
        dir: dir.clone(),
        fg: fg.clone(),
        common,
        origin,
    };

    for (name, worktree) in &spec.worktrees {
        if name == "fg" {
            continue;
        }
        let path = dir.join(name);
        git_in(
            &fg,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                &path_text(&path),
                "main",
            ],
        )?;
        if let Some(branch) = &worktree.branch {
            git_in(&path, &["checkout", "--quiet", "-b", branch])?;
        }
    }
    for (name, worktree) in &spec.worktrees {
        shape(&pool, name, worktree)?;
    }
    for (name, worktree) in &spec.worktrees {
        if worktree.unreadable_index {
            let index = pool.git_dir(name).join("index");
            make_unreadable(&index)?;
        }
    }
    for (name, worktree) in &spec.worktrees {
        if worktree.missing && name != "fg" {
            let path = pool.worktree(name);
            fs::remove_dir_all(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    if spec.agents {
        let activity = pool.activity();
        fs::create_dir_all(&activity).map_err(|e| format!("{}: {e}", activity.display()))?;
        write(&activity.join(".verify-sandbox"), "")?;
    }
    Ok(pool)
}

/// Takes every permission from `path`, so git cannot read it.
#[cfg(unix)]
fn make_unreadable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o000))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Windows has no mode that keeps an owner from reading a file.
#[cfg(not(unix))]
fn make_unreadable(path: &Path) -> Result<(), String> {
    Err(format!(
        "{}: an unreadable file needs Unix permissions",
        path.display()
    ))
}

fn shape(pool: &Pool, name: &str, spec: &WorktreeSpec) -> Result<(), String> {
    let path = pool.worktree(name);
    if let Some(branch) = &spec.branch
        && spec.upstream
    {
        git_in(&path, &["push", "--quiet", "-u", "origin", branch])?;
        let mut tip = git_in(&path, &["rev-parse", "HEAD"])?.trim().to_owned();
        for number in 0..spec.behind {
            tip = commit_on(&path, &tip, &format!("upstream {number}"))?;
        }
        if spec.behind > 0 {
            git_in(
                &path,
                &[
                    "push",
                    "--quiet",
                    "origin",
                    &format!("{tip}:refs/heads/{branch}"),
                ],
            )?;
            git_in(&path, &["fetch", "--quiet", "origin"])?;
        }
    }
    for number in 0..spec.ahead.max(spec.unreferenced) {
        git_in(
            &path,
            &[
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                &format!("local {number}"),
            ],
        )?;
    }
    match spec.operation.as_deref() {
        None => {}
        Some("bisect") => {
            git_in(&path, &["bisect", "start"])?;
        }
        Some(operation @ ("rebase" | "merge")) => {
            let theirs = conflicting_commit(&pool.fg)?;
            let branch = format!("ops-{name}");
            git_in(&pool.fg, &["branch", &branch, &theirs])?;
            write(&path.join("f.txt"), "ours\n")?;
            git_in(&path, &["commit", "--quiet", "-am", "ours"])?;
            if git_status(&path, &[operation, &branch])? {
                return Err(format!("{operation} of {branch} in {name} did not stop"));
            }
        }
        Some(other) => return Err(format!("unknown operation {other}")),
    }
    for number in 0..spec.dirty {
        write(&path.join(format!("dirty-{number}.txt")), "uncommitted\n")?;
    }
    Ok(())
}

/// Replaces `{pool}` and `{common}` in every string of `value` with the pool's paths.
pub fn resolve_placeholders(value: &mut serde_json::Value, pool: &Pool) {
    let pool_text = path_text(&pool.dir);
    let common_text = path_text(&pool.common);
    match value {
        serde_json::Value::String(text) => {
            *text = text
                .replace("{common}", &common_text)
                .replace("{pool}", &pool_text);
        }
        serde_json::Value::Array(items) => {
            for item in items {
                resolve_placeholders(item, pool);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values_mut() {
                resolve_placeholders(item, pool);
            }
        }
        _ => {}
    }
}
