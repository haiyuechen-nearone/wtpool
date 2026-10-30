//! The e2e layer: a real VS Code with the packaged extension and the release `wtpool`. Each check
//! attempt builds its own sandbox pool through `cargo xtask pool`, so a check that changes its
//! pool, or a retry, never sees another attempt's changes.

use std::fs;
use std::path::{Path, PathBuf};

use super::Wanted;
use super::builds::release_wtpool;
use super::focus;
use super::js::{
    PlaywrightRun, ResultsFile, every_wanted, playwright, remove_short_links, run_config,
};
use crate::parts::Ctx;
use crate::results::CheckResult;
use crate::web::Format;

/// Builds the pool of scenario `name` in `dir`, replacing its contents, and returns the JSON
/// object the e2e checks read: `scenario`, `dir`, `fg` and `common`.
pub fn build_pool(root: &Path, name: &str, dir: &Path) -> Result<serde_json::Value, String> {
    let scenario = crate::scenario::load(root, name)?;
    if dir.exists() {
        fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let pool = crate::pool::build(dir, &scenario.pool)?;
    Ok(serde_json::json!({
        "scenario": name,
        "dir": pool.dir.to_string_lossy(),
        "fg": pool.fg.to_string_lossy(),
        "common": pool.common.to_string_lossy(),
    }))
}

/// The flow check that runs every scenario of the e2e layer and reports each one's outcome.
pub const SCENARIO_FLOW: &str = "flow__every_activity_shows";

pub(super) fn e2e(ctx: &Ctx, wanted: &Wanted) -> Vec<CheckResult> {
    let fail_all = |message: String| {
        every_wanted(wanted, "e2e", |check| {
            CheckResult::error(check, "e2e", message.clone())
        })
    };
    let release = match release_wtpool(ctx) {
        Ok(path) => path,
        Err(error) => return fail_all(format!("the release build failed: {error}")),
    };
    let xtask: PathBuf = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => return fail_all(format!("setup: the xtask binary: {error}")),
    };
    let _ = fs::remove_dir_all(ctx.run_dir.join("e2e"));
    // Each phase 1 scenario check of the e2e layer runs as a case of flow__every_activity_shows,
    // `flow__every_activity_shows <scenario>`. The case writes its outcome to
    // e2e/scenarios/<scenario>.json. `--only <scenario>` runs that case.
    // Later scenarios are checks of their own in scenarios-agents.spec.mjs.
    let grep = ctx
        .only
        .as_deref()
        .filter(|only| {
            crate::scenario::load(&ctx.root, only).is_ok_and(|scenario| scenario.phase == 1)
        })
        .map(|only| crate::web::title_pattern(&format!("{SCENARIO_FLOW} {only}")));
    let envs = vec![
        (
            "WTPOOL_VERIFY_XTASK".to_owned(),
            xtask.to_string_lossy().into_owned(),
        ),
        (
            "WTPOOL_VERIFY_RELEASE_BIN".to_owned(),
            release.to_string_lossy().into_owned(),
        ),
    ];
    let guard = focus::watch(ctx, "e2e");
    // First every check that may share the machine, in parallel. Its outcomes are read with the
    // exclusive run's. A failure to start shows as checks that reported no result.
    let parallel = PlaywrightRun {
        envs,
        grep,
        ..PlaywrightRun::default()
    };
    let _ = run_config(
        ctx,
        wanted,
        "e2e",
        "vscode/test/e2e/playwright.config.mjs",
        &parallel,
    );
    remove_short_links(ctx, "e2e");
    // Next, the checks that need webview focus to themselves, one at a time.
    let mut found = playwright(
        ctx,
        wanted,
        "e2e",
        "vscode/test/e2e/playwright.exclusive.config.mjs",
        PlaywrightRun {
            results: Some("results-exclusive.json"),
            log: Some("e2e_exclusive"),
            extra: std::iter::once(ResultsFile {
                format: Format::Playwright,
                path: ctx.run_dir.join("e2e/results.json"),
            })
            .chain(crate::scenario::names(&ctx.root).into_iter().map(|name| {
                ResultsFile {
                    format: Format::Extra,
                    path: ctx
                        .run_dir
                        .join("e2e/scenarios")
                        .join(format!("{name}.json")),
                }
            }))
            .collect(),
            ..parallel
        },
    );
    remove_short_links(ctx, "e2e");
    found.extend(guard.finish());
    found.extend(focus::side_effects(ctx, "e2e"));
    found
}
