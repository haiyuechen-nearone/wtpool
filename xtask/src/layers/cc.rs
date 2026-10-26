//! The cc layer (verification.md, "Contract test"): `vscode/test/cc/run.mjs` drives the
//! installed `claude` through the Agent SDK, on a build copy of the release `wtpool` in `bin/` of the
//! harness data folder (crate::dirs, `~/.local/share/wtpool-verify` by default). It runs only with
//! `--contract`. With `--record`, the oracle step's live run, raw recordings go to `recordings/`
//! there, for the lock and the scrubber; any other live run records into its run folder. The
//! harness supports the latest installed Claude Code alone: a record in which every cc check
//! passes moves the recordings of each other version into `recordings-backup/`
//! ([`retire_recordings`]).
//!
//! `claude` keeps the real home and the owner's login and loads only the run's settings file
//! (`settingSources: []`). The runner deletes the `~/.claude/projects/` transcript folders its
//! own sessions created and touches nothing else under `~/.claude/`.
//!
//! A machine that never recorded a live run (no recordings folder, so no live result for any
//! Claude Code) counts the cc checks by this run's `cc_replay:<name>` alone, without `--contract`,
//! and the report says so ([`replay_only_reason`]).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::LAYER_LIMIT;
use super::builds::{npm_ci, oracle_js, release_wtpool};
use super::cc_judge::{
    LAYER, LIVE_RESULTS, LiveResult, RunnerResult, by_replay, fail_all, judge_live,
};
use crate::parts::Ctx;
use crate::replay::PREFIX;
use crate::results::CheckResult;
use crate::web::Registry;

/// The build copy of the release `wtpool` the contract sessions install as their hook.
fn build_copy(release: &Path) -> Result<PathBuf, String> {
    let dir = crate::dirs::data_dir()?.join("bin");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let copy = dir.join("wtpool");
    fs::copy(release, &copy).map_err(|e| format!("{}: {e}", copy.display()))?;
    Ok(copy)
}

/// The installed Claude Code's version, the first word `claude --version` prints.
pub fn claude_version() -> Result<String, String> {
    let text = crate::cmd::output(
        std::process::Command::new("claude").arg("--version"),
        Duration::from_secs(30),
    )?;
    text.split_whitespace()
        .next()
        .map(str::to_owned)
        .ok_or_else(|| "claude --version printed nothing".to_owned())
}

/// The live result of the oracle step for `version`: `cc-results.json` beside its recordings,
/// hashed by the lock.
pub fn live_results_path(version: &str) -> Option<PathBuf> {
    crate::scrub::recordings_dir()
        .map(|dir| dir.join(format!("claude-{version}")).join(LIVE_RESULTS))
}

/// The reason the cc checks count by this run's replays alone, or `None` when they need the live
/// result.
/// They need it on a machine that keeps the recordings of a live run: it holds the live result of
/// each Claude Code version it ran, and a new version needs its own live run (`--contract`).
pub fn replay_only_reason() -> Option<String> {
    let recordings = crate::scrub::recordings_dir()?;
    if recordings.exists() {
        return None;
    }
    let claude = claude_version().map_or_else(
        |_| "no claude is installed".to_owned(),
        |version| format!("Claude Code {version} has no live result"),
    );
    Some(format!(
        "{claude} and {} does not exist: no live contract run recorded on this machine",
        recordings.display()
    ))
}

/// This run's `cc_replay:<name>` results of the int part.
fn replayed(run_dir: &Path) -> Vec<CheckResult> {
    let mut found = crate::results::load_part(run_dir, "int")
        .ok()
        .flatten()
        .map(|file| file.results)
        .unwrap_or_default();
    found.retain(|result| result.check.starts_with(PREFIX));
    found
}

/// For a run whose cc part did not run: its cc checks by replay, or `None` when this machine needs
/// the live result, so the cc part must run.
pub fn report_by_replay(root: &Path, run_dir: &Path) -> Option<Vec<CheckResult>> {
    let reason = replay_only_reason()?;
    Some(match Registry::load(root) {
        Ok(registry) => by_replay(&replayed(run_dir), &registry.in_layer(LAYER), &reason),
        Err(error) => vec![CheckResult::error("cc:registry", LAYER, error)],
    })
}

/// The cc checks without `--contract` (verification.md, "Contract test" and "Trace list"): the
/// Claude Code asserts by the live result for the installed version, hashed by the lock,
/// and the wtpool asserts by this run's `cc_replay:<name>` of the int part. It starts no session.
fn from_live_result(ctx: &Ctx, wanted: &BTreeSet<String>) -> Vec<CheckResult> {
    if let Some(reason) = replay_only_reason() {
        return by_replay(&replayed(&ctx.run_dir), wanted, &reason);
    }
    let version = match claude_version() {
        Ok(version) => version,
        Err(error) => {
            return fail_all(
                wanted,
                CheckResult::not_run,
                &format!("no installed claude: {error}"),
            );
        }
    };
    let Some(path) = live_results_path(&version) else {
        return fail_all(wanted, CheckResult::not_run, "HOME is not set");
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return fail_all(
            wanted,
            CheckResult::not_run,
            &format!(
                "Claude Code {version} has no live result ({}): run `--part cc --contract`",
                path.display()
            ),
        );
    };
    let locked = fs::read_to_string(ctx.root.join(crate::oracle::LOCK))
        .ok()
        .and_then(|text| crate::oracle::load_lock_text(&text).ok())
        .map(|lock| lock.recordings)
        .unwrap_or_default();
    let live = LiveResult {
        version: &version,
        path: &path,
        text: &text,
    };
    judge_live(&live, &locked, &replayed(&ctx.run_dir), wanted)
}

/// Keeps the live result `text` of the installed Claude Code beside its recordings, for the lock;
/// the version it kept it for.
fn keep_live_result(text: &str) -> Result<String, String> {
    let version = claude_version()?;
    let path = live_results_path(&version).ok_or("HOME is not set")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(version)
}

/// The folder beside `recordings/` that keeps retired recordings.
const BACKUP: &str = "recordings-backup";

/// Moves each `claude-<version>` folder of `recordings` but `claude-{keep}` into [`BACKUP`]
/// beside it, as `<folder>.bak-<seconds since the epoch>`; one line per folder.
fn retire_recordings(recordings: &Path, keep: &str) -> Vec<String> {
    let keep = format!("claude-{keep}");
    let Some(backup) = recordings.parent().map(|dir| dir.join(BACKUP)) else {
        return Vec::new();
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let mut names: Vec<String> = fs::read_dir(recordings)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with("claude-") && *name != keep)
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let to = backup.join(format!("{name}.bak-{stamp}"));
            let moved =
                fs::create_dir_all(&backup).and_then(|()| fs::rename(recordings.join(&name), &to));
            match moved {
                Ok(()) => format!("retired {name} to {}", to.display()),
                Err(error) => format!(
                    "kept {name}, as it did not move to {}: {error}",
                    to.display()
                ),
            }
        })
        .collect()
}

pub fn cc(ctx: &Ctx, contract: bool, record: bool) -> Vec<CheckResult> {
    let registry = match Registry::load(&ctx.root) {
        Ok(registry) => registry,
        Err(error) => return vec![CheckResult::error("cc:registry", LAYER, error)],
    };
    let mut wanted = registry.in_layer(LAYER);
    wanted.retain(|check| ctx.wants(check));
    if wanted.is_empty() {
        return Vec::new();
    }
    if !contract {
        return from_live_result(ctx, &wanted);
    }
    let copy = match release_wtpool(ctx).and_then(|release| build_copy(&release)) {
        Ok(copy) => copy,
        Err(error) => {
            return fail_all(
                &wanted,
                CheckResult::error,
                &format!("the release build or its copy failed: {error}"),
            );
        }
    };
    if let Err(error) = npm_ci(ctx, &oracle_js(ctx), "npm_ci_oracle") {
        return fail_all(
            &wanted,
            CheckResult::error,
            &format!("setup: npm ci in vscode/test: {error}"),
        );
    }
    let xtask = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            return fail_all(
                &wanted,
                CheckResult::error,
                &format!("setup: the xtask binary: {error}"),
            );
        }
    };
    let dir = ctx.run_dir.join("cc");
    let _ = fs::remove_dir_all(&dir);
    if let Err(error) = fs::create_dir_all(&dir) {
        return fail_all(
            &wanted,
            CheckResult::error,
            &format!("setup: {}: {error}", dir.display()),
        );
    }
    let recordings = if record {
        crate::scrub::recordings_dir().unwrap_or_else(|| dir.join("recordings"))
    } else {
        dir.join("recordings")
    };
    let results = dir.join("results.json");
    let mut command = ctx.command("node");
    command
        .current_dir(oracle_js(ctx))
        .arg("cc/run.mjs")
        .env("WTPOOL_VERIFY_RUN_DIR", &ctx.run_dir)
        .env("WTPOOL_VERIFY_CC_WTPOOL", &copy)
        .env("WTPOOL_VERIFY_CC_RECORDINGS", &recordings)
        .env("WTPOOL_VERIFY_CC_RESULTS", &results)
        .env("WTPOOL_VERIFY_XTASK", &xtask)
        .env(
            "WTPOOL_VERIFY_CC_CHECKS",
            wanted
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(","),
        );
    let ran = ctx.run_logged("cc", &mut command, LAYER_LIMIT);
    let text = match fs::read_to_string(&results) {
        Ok(text) => text,
        Err(error) => {
            let tail = ran.map_or_else(|e| e, |r| r.tail(30));
            return fail_all(
                &wanted,
                CheckResult::error,
                &format!("{}: {error}\n{tail}", results.display()),
            );
        }
    };
    let entries: Vec<RunnerResult> = match serde_json::from_str(&text) {
        Ok(entries) => entries,
        Err(error) => {
            return fail_all(
                &wanted,
                CheckResult::error,
                &format!("{}: {error}", results.display()),
            );
        }
    };
    let mut found = Vec::new();
    for entry in &entries {
        if wanted.contains(&entry.check) {
            found.push(entry.to_result());
        }
    }
    for check in &wanted {
        if !found.iter().any(|r| &r.check == check) {
            found.push(CheckResult::error(
                check,
                LAYER,
                "the cc layer reported no result for it",
            ));
        }
    }
    // The oracle step's live run keeps its result beside its recordings, for the lock, and once
    // every cc check passed, the recordings of the other versions retire.
    if record {
        match keep_live_result(&text) {
            Ok(version) => {
                let every_passed = registry
                    .in_layer(LAYER)
                    .iter()
                    .all(|check| entries.iter().any(|e| &e.check == check && e.passed()));
                if every_passed {
                    for line in retire_recordings(&recordings, &version) {
                        eprintln!("cc: {line}");
                    }
                }
            }
            Err(error) => found.push(CheckResult::error(
                "cc:live_result",
                LAYER,
                format!("the live result was not kept for the lock: {error}"),
            )),
        }
    }
    found.sort_by(|a, b| a.check.cmp(&b.check));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retire_recordings__should_move_every_other_version_to_the_backup() {
        // Given
        let data =
            std::env::temp_dir().join(format!("wtpool-verify-retire-{}", std::process::id()));
        let recordings = data.join("recordings");
        for name in ["claude-2.1.306", "claude-2.1.310"] {
            fs::create_dir_all(recordings.join(name)).unwrap();
            fs::write(recordings.join(name).join("cc_permission.jsonl"), name).unwrap();
        }
        let names = |dir: &Path| -> Vec<String> {
            let mut found: Vec<String> = fs::read_dir(dir)
                .map(|e| {
                    e.flatten()
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            found.sort();
            found
        };

        // When
        let lines = retire_recordings(&recordings, "2.1.310");

        // Then
        let kept = names(&recordings);
        let backup = names(&data.join(BACKUP));
        let _ = fs::remove_dir_all(&data);
        assert_eq!(kept, vec!["claude-2.1.310"]);
        assert!(
            matches!(backup.as_slice(), [one] if one.starts_with("claude-2.1.306.bak-")),
            "{backup:?}"
        );
        assert!(
            matches!(lines.as_slice(), [line] if line.starts_with("retired claude-2.1.306 to ")),
            "{lines:?}"
        );
    }
}
