//! The js layer of the quick part: Node's own test runner on `vscode/test/opencode/`, which loads
//! the OpenCode plugin of `crates/wtpool-opencode/plugin/` as Node runs it, its types removed, and
//! replays the recorded OpenCode streams through it. It needs no npm package, so it runs before,
//! and without, the extension's build. It runs from phase 5, the plugin's first.

use std::fs;

use super::builds::oracle_js;
use super::js::{Finished, ResultsFile, read_results};
use super::{LAYER_LIMIT, Wanted};
use crate::parts::Ctx;
use crate::results::CheckResult;
use crate::web::Format;

pub(super) const LAYER: &str = "js";

/// The longest one test may run: the replays of every recorded stream take seconds.
const TEST_LIMIT_MS: u32 = 120_000;

pub(super) fn js(ctx: &Ctx, wanted: &Wanted) -> Vec<CheckResult> {
    if ctx.phase < super::oc_static::PHASE || wanted.in_layer(LAYER).is_empty() {
        return Vec::new();
    }
    let dir = ctx.run_dir.join(LAYER);
    let _ = fs::remove_dir_all(&dir);
    if let Err(error) = fs::create_dir_all(&dir) {
        return super::js::every_wanted(wanted, LAYER, |check| {
            CheckResult::error(check, LAYER, format!("setup: {}: {error}", dir.display()))
        });
    }
    let results = ResultsFile {
        format: Format::Mocha,
        path: dir.join("results.json"),
    };
    let mut command = ctx.command("node");
    command
        .current_dir(oracle_js(ctx))
        .arg("--test")
        .arg(format!("--test-timeout={TEST_LIMIT_MS}"))
        .arg("--test-reporter=./opencode/reporter.mjs")
        .arg(format!(
            "--test-reporter-destination={}",
            results.path.display()
        ))
        .env("WTPOOL_VERIFY_RUN_DIR", &ctx.run_dir);
    if let Some(pattern) = wanted.grep() {
        command.arg(format!("--test-name-pattern={pattern}"));
    }
    command.arg("opencode/*.test.mjs");
    let ran = ctx.run_logged(LAYER, &mut command, LAYER_LIMIT);
    let log = ctx.log_path(LAYER);
    read_results(wanted, LAYER, &results, &[], Finished { ran, log: &log })
}
