//! The oc layer: `vscode/test/oc/run.mjs` runs the installed `opencode` with the release
//! `wtpool` and the plugin `wtpool hook install --harness opencode --policy` prints, in a sandbox
//! of the run folder, on a scripted local model and on one free model of OpenCode's own provider.
//! It needs no credential and costs nothing, so it runs without `--contract`.
//!
//! Every `opencode` command, `--version` too, runs with the sandbox's home and XDG folders, so
//! none touches the owner's OpenCode. A machine without `opencode` counts the oc checks by this
//! run's `oc_replay:<session>` alone ([`by_replay`]).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::LAYER_LIMIT;
use super::builds::{oracle_js, release_wtpool};
use super::oc_judge::{LAYER, REPLAY_PREFIX, RunnerResult, by_replay, fail_all};
use crate::parts::Ctx;
use crate::results::CheckResult;
use crate::web::Registry;

/// The variable that names the free model of `oc_real_model`.
const MODEL_VARIABLE: &str = "WTPOOL_VERIFY_OC_MODEL";

/// The installed `opencode` on the harness's `PATH`, found without running it.
fn installed_opencode() -> Option<PathBuf> {
    let text = crate::cmd::output(
        std::process::Command::new("/usr/bin/which").arg("opencode"),
        Duration::from_secs(10),
    )
    .ok()?;
    let path = PathBuf::from(text.trim());
    path.is_file().then_some(path)
}

/// This run's `oc_replay:<session>` results of the int part.
fn replayed(run_dir: &Path) -> Vec<CheckResult> {
    let mut found = crate::results::load_part(run_dir, "int")
        .ok()
        .flatten()
        .map(|file| file.results)
        .unwrap_or_default();
    found.retain(|result| result.check.starts_with(REPLAY_PREFIX));
    found
}

/// The reason the oc checks count by replay alone: no `opencode` is installed.
pub fn replay_only_reason() -> Option<String> {
    installed_opencode()
        .is_none()
        .then(|| "no opencode is installed".to_owned())
}

/// For a run whose oc part did not run: its oc checks by replay where no `opencode` is installed,
/// or `None` when the oc part must run.
pub fn report_by_replay(root: &Path, run_dir: &Path) -> Option<Vec<CheckResult>> {
    let reason = replay_only_reason()?;
    Some(match Registry::load(root) {
        Ok(registry) => by_replay(&replayed(run_dir), &registry.in_layer(LAYER), &reason),
        Err(error) => vec![CheckResult::error("oc:registry", LAYER, error)],
    })
}

pub fn oc(ctx: &Ctx) -> Vec<CheckResult> {
    let registry = match Registry::load(&ctx.root) {
        Ok(registry) => registry,
        Err(error) => return vec![CheckResult::error("oc:registry", LAYER, error)],
    };
    let mut wanted = registry.in_layer(LAYER);
    wanted.retain(|check| ctx.wants(check));
    if wanted.is_empty() {
        return Vec::new();
    }
    let Some(opencode) = installed_opencode() else {
        return by_replay(&replayed(&ctx.run_dir), &wanted, "no opencode is installed");
    };
    let release = match release_wtpool(ctx) {
        Ok(release) => release,
        Err(error) => {
            return fail_all(
                &wanted,
                CheckResult::error,
                &format!("the release build failed: {error}"),
            );
        }
    };
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
    let dir = ctx.run_dir.join(LAYER);
    let _ = fs::remove_dir_all(&dir);
    if let Err(error) = fs::create_dir_all(&dir) {
        return fail_all(
            &wanted,
            CheckResult::error,
            &format!("setup: {}: {error}", dir.display()),
        );
    }
    let results = dir.join("results.json");
    let mut command = ctx.normal_priority_command("node");
    command
        .current_dir(oracle_js(ctx))
        .arg("oc/run.mjs")
        .env("WTPOOL_VERIFY_RUN_DIR", &ctx.run_dir)
        .env("WTPOOL_VERIFY_OC_WTPOOL", &release)
        .env("WTPOOL_VERIFY_OC_OPENCODE", &opencode)
        .env("WTPOOL_VERIFY_OC_RESULTS", &results)
        .env("WTPOOL_VERIFY_XTASK", &xtask)
        .env("WTPOOL_VERIFY_OC_CHECKS", joined(&wanted));
    if let Ok(model) = std::env::var(MODEL_VARIABLE) {
        command.env(MODEL_VARIABLE, model);
    }
    let ran = ctx.run_logged(LAYER, &mut command, LAYER_LIMIT);
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
    let mut found: Vec<CheckResult> = entries
        .iter()
        .filter(|entry| wanted.contains(&entry.check))
        .map(RunnerResult::to_result)
        .collect();
    for check in &wanted {
        if !found.iter().any(|r| &r.check == check) {
            found.push(CheckResult::error(
                check,
                LAYER,
                "the oc layer reported no result for it",
            ));
        }
    }
    found.sort_by(|a, b| a.check.cmp(&b.check));
    found
}

fn joined(checks: &BTreeSet<String>) -> String {
    checks
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(",")
}
