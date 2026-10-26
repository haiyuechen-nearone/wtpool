//! A scratch clone of the repository in the harness cache, where freeze and the canary runner
//! apply patches and run checks without touching the working tree.

mod canaries;
mod inventory;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::cmd::{self, git};
use crate::isolation::VERIFY_EMAIL;
use crate::oracle::FREEZE_SUBJECT;
use crate::results::CheckResult;

pub use canaries::{
    Canary, CanaryHeader, Expect, Tree, canary, caught, caught_as, load_canary, oracle_canaries,
    parse_canary_header,
};
pub use inventory::{HARNESS_CHECKS, absent_checks, implemented_checks, part_for, part_of};

/// A reused clone in `clones/` of the harness cache (crate::dirs), so its `target/` stays warm.
pub struct ScratchClone {
    pub dir: PathBuf,
}

/// Removes untracked and ignored files, so nothing a canary added stays behind, a file in a
/// folder git ignores included. Build and run output stays warm.
const CLEAN: [&str; 14] = [
    "clean",
    "-ffdqx",
    "-e",
    "/target/",
    "-e",
    "/xtask/target/",
    "-e",
    "/verify/out/",
    "-e",
    "/node_modules/",
    "-e",
    "/vscode/node_modules/",
    "-e",
    "/vscode/test/node_modules/",
];

/// The source's HEAD, as [`ScratchClone::prepare`] fetches it.
const SOURCE_HEAD: &str = "refs/verify/head";

fn cache_dir() -> Result<PathBuf, String> {
    Ok(crate::dirs::cache_dir()?.join("clones"))
}

impl ScratchClone {
    /// Checks out `root`'s HEAD with its uncommitted changes on top.
    pub fn prepare(root: &Path, name: &str) -> Result<Self, String> {
        let dir = cache_dir()?.join(name);
        if !dir.join(".git").exists() {
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            git(&dir, &["init", "--quiet"])?;
        }
        let source = root.to_string_lossy().into_owned();
        git(
            &dir,
            &[
                "fetch",
                "--quiet",
                "--force",
                "--no-write-fetch-head",
                &source,
                &format!("+HEAD:{SOURCE_HEAD}"),
                "+refs/tags/*:refs/tags/*",
            ],
        )?;
        git(
            &dir,
            &["checkout", "--quiet", "--force", "--detach", SOURCE_HEAD],
        )?;
        git(&dir, &CLEAN)?;
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
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                fs::copy(&from, &to).map_err(|e| format!("{rel}: {e}"))?;
            } else {
                let _ = fs::remove_file(&to);
            }
        }
        Ok(Self { dir })
    }

    pub fn commit_freeze(&self, phase: u8) -> Result<String, String> {
        git(&self.dir, &["add", "-A"])?;
        let message = format!("{FREEZE_SUBJECT} {phase}");
        git(
            &self.dir,
            &[
                "-c",
                "user.name=wtpool-verify",
                "-c",
                &format!("user.email={VERIFY_EMAIL}"),
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                &message,
            ],
        )?;
        Ok(git(&self.dir, &["rev-parse", "HEAD"])?.trim().to_owned())
    }

    /// The checks the source's HEAD implements, without its uncommitted changes. Leaves the clone
    /// at that commit.
    pub fn checks_at_source_head(&self) -> Result<std::collections::BTreeSet<String>, String> {
        self.reset_to(SOURCE_HEAD)?;
        implemented_checks(&self.dir)
    }

    pub(crate) fn reset_to(&self, commit: &str) -> Result<(), String> {
        git(
            &self.dir,
            &["checkout", "--quiet", "--force", "--detach", commit],
        )?;
        git(&self.dir, &CLEAN)?;
        Ok(())
    }

    /// Applies a canary and, unless it asks for an uncommitted tree, commits it. A patch may use
    /// hunks without context, so it applies to an oracle file that later entries extend.
    pub(crate) fn apply(&self, canary: &Canary) -> Result<(), String> {
        let patch = fs::canonicalize(&canary.path)
            .map_err(|e| format!("{}: {e}", canary.path.display()))?;
        let patch = patch.to_string_lossy();
        let mut apply = vec!["apply", "--whitespace=nowarn", "--unidiff-zero"];
        if canary.tree == Tree::Committed {
            apply.push("--index");
        }
        apply.push(&patch);
        git(&self.dir, &apply)?;
        if canary.tree == Tree::Uncommitted {
            return Ok(());
        }
        let name = canary
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        git(
            &self.dir,
            &[
                "-c",
                "user.name=wtpool-verify",
                "-c",
                &format!("user.email={VERIFY_EMAIL}"),
                "commit",
                "--quiet",
                "--no-verify",
                "-m",
                &format!("verify: canary {name}"),
            ],
        )
        .map(|_| ())
    }

    fn xtask(&self, args: &[&str]) -> Result<(), String> {
        // Freeze needs one failure on an assertion per new check: no retries in e2e, shot and
        // host. A verify run keeps them, since they catch flaky passes.
        let ran = cmd::run(
            Command::new("cargo")
                .current_dir(&self.dir)
                .arg("xtask")
                .args(args)
                .env_remove("CARGO_TARGET_DIR")
                .env("WTPOOL_VERIFY_RETRIES", "0")
                .env("WTPOOL_VERIFY_HOST_RETRIES", "0"),
            Duration::from_secs(3 * 3600),
        )?;
        if ran.timed_out() {
            return Err(format!("cargo xtask {} timed out", args.join(" ")));
        }
        Ok(())
    }

    /// Runs each of `parts` whole, once, in one run with `--fresh`, and returns every result:
    /// int before cc, whose checks read the int part's `cc_replay:<name>`; report checks after
    /// the parts the report needs.
    pub fn run_parts(&self, phase: u8, parts: &[&str]) -> Result<Vec<CheckResult>, String> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        let run = format!("freeze-{millis}");
        let phase_text = phase.to_string();
        let run_dir = self.dir.join("verify/out").join(&run);
        let mut order: Vec<&str> = Vec::new();
        for part in ["int", "quick", "ui", "bench", "cc"] {
            let needed = parts.contains(&part)
                || (part == "int" && parts.contains(&"cc"))
                || (parts.contains(&"report")
                    && crate::report::required_parts(phase).contains(&part));
            if needed {
                order.push(part);
            }
        }
        // One invocation: independent parts run in parallel, each artifact built once.
        let list = order.join(",");
        self.xtask(&[
            "verify",
            "--phase",
            &phase_text,
            "--run",
            &run,
            "--part",
            &list,
            "--fresh",
        ])?;
        let mut results = Vec::new();
        for part in &order {
            if let Some(file) = crate::results::load_part(&run_dir, part)? {
                results.extend(file.results);
            }
        }
        if parts.contains(&"report") {
            self.xtask(&["verify", "--phase", &phase_text, "--run", &run, "--report"])?;
            let text =
                fs::read_to_string(run_dir.join("report.json")).map_err(|e| e.to_string())?;
            let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            let reported: Vec<CheckResult> = json
                .get("results")
                .and_then(|r| serde_json::from_value(r.clone()).ok())
                .unwrap_or_default();
            results.extend(reported);
        }
        Ok(crate::web::merge(results))
    }

    /// Runs one check the way `verify --only` does and returns its result.
    pub fn run_check(&self, phase: u8, check: &str) -> Result<CheckResult, String> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        let run = format!("canary-{millis}");
        let phase_text = phase.to_string();
        let part = part_for(&self.dir, check);
        let base = [
            "verify",
            "--phase",
            phase_text.as_str(),
            "--run",
            run.as_str(),
        ];
        let run_dir = self.dir.join("verify/out").join(&run);
        if part == "report" {
            for part in crate::report::required_parts(phase) {
                let mut args = base.to_vec();
                args.extend(["--part", part]);
                self.xtask(&args)?;
            }
            let mut args = base.to_vec();
            args.push("--report");
            self.xtask(&args)?;
            let text =
                fs::read_to_string(run_dir.join("report.json")).map_err(|e| e.to_string())?;
            let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            let results: Vec<CheckResult> = json
                .get("results")
                .and_then(|r| serde_json::from_value(r.clone()).ok())
                .unwrap_or_default();
            return results
                .into_iter()
                .find(|r| r.check == check)
                .ok_or_else(|| format!("the report has no result for {check}"));
        }
        // A cc check counts by the oracle step's live result and this tree's replay of its
        // recordings; freeze starts no claude session (verification.md, "Contract test").
        if part == "cc" {
            let replay = format!("{}{check}", crate::replay::PREFIX);
            let mut args = base.to_vec();
            args.extend(["--part", "int", "--only", replay.as_str()]);
            self.xtask(&args)?;
        }
        let mut args = base.to_vec();
        args.extend(["--part", part, "--only", check]);
        self.xtask(&args)?;
        crate::results::load_part(&run_dir, part)?
            .and_then(|file| file.results.into_iter().find(|r| r.check == check))
            .ok_or_else(|| format!("part {part} has no result for {check}"))
    }
}

pub fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}
