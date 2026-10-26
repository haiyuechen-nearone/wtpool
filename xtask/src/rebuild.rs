//! The rebuild steps of `version__should_list_version_commit_and_features`: `wtpool` built again
//! in a clone of the tree under test after each change of its tree state. Also the workspace
//! package with the `wtpool` binary, named with `-p` by each harness build of `wtpool`, and the
//! copy of the tree's uncommitted changes into a checkout.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::cmd::{self, git};
use crate::isolation::VERIFY_EMAIL;
use crate::parts::{Ctx, PART_LIMIT};

/// A tracked file the rebuild steps edit to make the clone's tree dirty.
const REBUILD_EDITED_FILE: &str = "README.md";

/// The workspace package with the `wtpool` binary.
pub struct WtpoolPackage {
    pub name: String,
    pub features: BTreeMap<String, Vec<String>>,
}

/// The workspace package with the `wtpool` binary, from `cargo metadata`.
pub fn wtpool_package(ctx: &Ctx) -> Result<WtpoolPackage, String> {
    let text = cmd::output(
        ctx.command("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1", "--locked"]),
        Duration::from_secs(120),
    )?;
    let metadata: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    for package in packages {
        let has_wtpool_bin = package
            .get("targets")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|targets| {
                targets.iter().any(|target| {
                    target.get("name").and_then(serde_json::Value::as_str) == Some("wtpool")
                        && target
                            .get("kind")
                            .and_then(serde_json::Value::as_array)
                            .is_some_and(|kinds| kinds.iter().any(|k| k.as_str() == Some("bin")))
                })
            });
        if !has_wtpool_bin {
            continue;
        }
        let name = package
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let features = package
            .get("features")
            .and_then(|f| serde_json::from_value(f.clone()).ok())
            .unwrap_or_default();
        return Ok(WtpoolPackage { name, features });
    }
    Err("no workspace package has a `wtpool` binary".to_owned())
}

struct Rebuild<'a> {
    ctx: &'a Ctx,
    repo: PathBuf,
    bins: PathBuf,
    profiles: Vec<RebuildProfile>,
    steps: Vec<RebuildStep>,
}

/// A cargo profile the rebuild steps build `wtpool` with, and where cargo puts that binary.
struct RebuildProfile {
    name: &'static str,
    cargo_args: Vec<String>,
    binary: &'static str,
}

/// One build of `wtpool` in the rebuild clone, with the tree state it built from.
#[derive(Serialize)]
struct RebuildStep {
    /// The profile, then the tree state, as in `release rebuild after an empty commit`.
    step: String,
    binary: String,
    commit: String,
    dirty: bool,
}

impl Rebuild<'_> {
    /// Builds `wtpool` with each profile at the clone's current tree state. All steps share the
    /// clone's one target folder, so each profile after its first build only rebuilds.
    fn build(&mut self, state: &str) -> Result<(), String> {
        let commit = crate::repo::head(&self.repo)?;
        let dirty = crate::repo::dirty_fingerprint(&self.repo)?.is_some();
        for profile in &self.profiles {
            let step = format!("{} {state}", profile.name);
            let number = self.steps.len().saturating_add(1);
            let mut command = self.ctx.command("cargo");
            command.current_dir(&self.repo).args(&profile.cargo_args);
            let ran =
                self.ctx
                    .run_logged(&format!("rebuild_{number}"), &mut command, PART_LIMIT)?;
            if !ran.ok() {
                return Err(format!("{step}: {}", ran.tail(20)));
            }
            let binary = self.bins.join(format!("{number}-{}-wtpool", profile.name));
            fs::copy(self.repo.join(profile.binary), &binary)
                .map_err(|e| format!("{step}: copy the binary: {e}"))?;
            self.steps.push(RebuildStep {
                step,
                binary: binary.to_string_lossy().into_owned(),
                commit: commit.clone(),
                dirty,
            });
        }
        Ok(())
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        git(&self.repo, args).map(|_| ())
    }
}

/// Copies `root`'s uncommitted changes, untracked files included, into the checkout `dir`.
pub(crate) fn copy_uncommitted(root: &Path, dir: &Path) -> Result<(), String> {
    let changed = git(root, &["diff", "HEAD", "--name-only", "-z"])?;
    let untracked = git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    for rel in changed
        .split('\0')
        .chain(untracked.split('\0'))
        .filter(|r| !r.is_empty())
    {
        let from = root.join(rel);
        let to = dir.join(rel);
        if fs::symlink_metadata(&from).is_ok() {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("{rel}: {e}"))?;
            }
            fs::copy(&from, &to).map_err(|e| format!("{rel}: {e}"))?;
        } else {
            let _ = fs::remove_file(&to);
        }
    }
    Ok(())
}

/// Builds `wtpool` in a clone of the tree under test inside the run folder, then again with the same
/// target folder after an empty commit, an edit of a tracked file and its revert. Each step builds
/// the `verify` debug build and the release build, as `cargo build --release` installs it. A build
/// that misses a change of the tree state names the state of an earlier build. Writes the steps
/// to `rebuild/steps.json` and returns its path. A `warm_target` folder, when it exists, becomes
/// the clone's target folder before the first build.
pub fn rebuild_steps(
    ctx: &Ctx,
    package: &str,
    features: &[&str],
    warm_target: Option<&Path>,
) -> Result<PathBuf, String> {
    let dir = ctx.run_dir.join("rebuild");
    let _ = fs::remove_dir_all(&dir);
    let build_args = |extra: &[&str]| -> Vec<String> {
        ["build", "--locked", "-p", package, "--bin", "wtpool"]
            .iter()
            .chain(extra)
            .map(|arg| (*arg).to_owned())
            .collect()
    };
    let mut rebuild = Rebuild {
        ctx,
        repo: dir.join("repo"),
        bins: dir.join("bins"),
        profiles: vec![
            RebuildProfile {
                name: "debug",
                cargo_args: build_args(features),
                binary: "target/debug/wtpool",
            },
            RebuildProfile {
                name: "release",
                cargo_args: build_args(&["--release"]),
                binary: "target/release/wtpool",
            },
        ],
        steps: Vec::new(),
    };
    for folder in [&rebuild.repo, &rebuild.bins] {
        fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    }
    if let Some(warm) = warm_target.filter(|warm| warm.is_dir()) {
        fs::rename(warm, rebuild.repo.join("target"))
            .map_err(|e| format!("{}: {e}", warm.display()))?;
    }
    // A fetch by path leaves no remote, so the clone has none outside the run folder.
    rebuild.git(&["init", "--quiet"])?;
    rebuild.git(&["config", "user.name", "wtpool-verify"])?;
    rebuild.git(&["config", "user.email", VERIFY_EMAIL])?;
    rebuild.git(&["config", "commit.gpgsign", "false"])?;
    let source = ctx.root.to_string_lossy().into_owned();
    rebuild.git(&[
        "fetch",
        "--quiet",
        "--no-write-fetch-head",
        &source,
        "+HEAD:refs/heads/rebuild",
    ])?;
    // On a branch, a commit leaves `.git/HEAD` unchanged, so a build that watches only that
    // file misses it.
    rebuild.git(&["checkout", "--quiet", "rebuild"])?;
    copy_uncommitted(&ctx.root, &rebuild.repo)?;
    rebuild.git(&["add", "-A"])?;
    if crate::repo::dirty_fingerprint(&rebuild.repo)?.is_some() {
        rebuild.git(&["commit", "--quiet", "-m", "verify: the tree under test"])?;
    }
    rebuild.build("first build")?;
    rebuild.git(&["commit", "--quiet", "--allow-empty", "-m", "verify: empty"])?;
    rebuild.build("rebuild after an empty commit")?;
    let edited = rebuild.repo.join(REBUILD_EDITED_FILE);
    let mut text = fs::read_to_string(&edited).map_err(|e| format!("{}: {e}", edited.display()))?;
    text.push_str("\nwtpool-verify edit\n");
    fs::write(&edited, text).map_err(|e| format!("{}: {e}", edited.display()))?;
    rebuild.build("rebuild after an edit")?;
    rebuild.git(&["checkout", "--quiet", "--", REBUILD_EDITED_FILE])?;
    rebuild.build("rebuild after reverting the edit")?;
    let path = dir.join("steps.json");
    let text = serde_json::to_string_pretty(&rebuild.steps).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}
