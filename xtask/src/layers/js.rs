//! The JavaScript layers of the quick and ui parts: `render` under Vitest, `host` under Mocha in a
//! VS Code, and `shot` and `e2e` under Playwright, each read from its reporter's results file.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::builds::{self, chromium, oracle_js, web_build};
use super::{LAYER_LIMIT, Wanted, e2e, focus, manifests, product, statics};
use crate::cmd;
use crate::parts::Ctx;
use crate::results::{CheckResult, FailKind};
use crate::web::{self, Format, Registry};

const NOT_YET: &str = "not yet run (e2e window)";

fn windows_allowed() -> bool {
    std::env::var("WTPOOL_VERIFY_WINDOWS").map_or(true, |value| value != "0")
}

/// The quick part's web layers: npm installs and builds, the static checks of the extension,
/// `render` and `host`.
pub fn quick(ctx: &Ctx) -> Vec<CheckResult> {
    let registry = match Registry::load(&ctx.root) {
        Ok(registry) => registry,
        Err(error) => return vec![CheckResult::error("web:registry", "static", error)],
    };
    let wanted = Wanted {
        ctx,
        registry: &registry,
    };
    let mut results = Vec::new();
    if wanted.check(crate::catalog::NAME) {
        results.push(crate::catalog::check(&ctx.root, ctx.phase));
    }
    if wanted.check(product::NO_SOCKET_APIS) {
        results.push(product::no_socket_apis(ctx));
    }
    if ctx.phase >= 2 && wanted.check(crate::scrub::NAME) {
        results.push(crate::scrub::check(&ctx.root));
    }
    let own = [
        crate::catalog::NAME,
        product::NO_SOCKET_APIS,
        crate::scrub::NAME,
    ];
    let web_layers = ["static", "render", "host"]
        .iter()
        .any(|layer| wanted.layer(layer))
        && ctx.only.as_deref().is_none_or(|only| !own.contains(&only));
    if !web_layers {
        return results;
    }
    let build = web_build(ctx, "static", true);
    let built = builds::built(&build);
    results.extend(build);
    if !built {
        return results;
    }
    if wanted.layer("static") {
        results.extend(manifests::manifest_checks(ctx, &wanted));
        results.extend(statics::static_checks(ctx, &wanted));
    }
    if wanted.layer("render") {
        results.extend(render(ctx, &wanted));
    }
    if wanted.layer("host") {
        results.extend(host(ctx, &wanted));
    }
    web::merge(results)
}

/// The ui part: `shot`, then `e2e`.
pub fn ui(ctx: &Ctx) -> Vec<CheckResult> {
    let registry = match Registry::load(&ctx.root) {
        Ok(registry) => registry,
        Err(error) => return vec![CheckResult::error("web:registry", "ui", error)],
    };
    let wanted = Wanted {
        ctx,
        registry: &registry,
    };
    let mut results = Vec::new();
    let package = wanted.layer("e2e") && windows_allowed();
    let build = web_build(ctx, "ui", package);
    let built = builds::built(&build);
    results.extend(build);
    if !built {
        return results;
    }
    if wanted.layer("shot") {
        results.extend(chromium(ctx, "shot"));
        results.extend(playwright(
            ctx,
            &wanted,
            "shot",
            "vscode/test/shot/playwright.config.mjs",
            PlaywrightRun::default(),
        ));
    }
    if wanted.layer("e2e") {
        results.extend(if !windows_allowed() {
            not_yet(&wanted, "e2e")
        } else if let Some(refused) = socket_refused(&wanted, "e2e") {
            refused
        } else {
            e2e::e2e(ctx, &wanted)
        });
    }
    web::merge(results)
}

/// The same result for every wanted check of `layer`, made by `result` from the check's name.
pub(super) fn every_wanted(
    wanted: &Wanted,
    layer: &str,
    result: impl Fn(&str) -> CheckResult,
) -> Vec<CheckResult> {
    let mut found = Vec::new();
    for check in wanted.in_layer(layer) {
        found.push(result(&check));
    }
    found
}

fn not_yet(wanted: &Wanted, layer: &str) -> Vec<CheckResult> {
    every_wanted(wanted, layer, |check| {
        CheckResult::not_run(check, layer, NOT_YET)
    })
}

/// One error for a layer that starts VS Code when its socket path would be too long
/// ([`crate::dirs::socket_problem`]), and its checks as not run, instead of an error per check.
fn socket_refused(wanted: &Wanted, layer: &str) -> Option<Vec<CheckResult>> {
    let problem = crate::dirs::socket_problem()?;
    let check = format!("{layer}:socket_path");
    let mut found = vec![CheckResult::error(&check, layer, problem)];
    found.extend(every_wanted(wanted, layer, |name| {
        CheckResult::not_run(name, layer, format!("VS Code cannot start: see {check}"))
    }));
    Some(found)
}

/// A results file of a layer's runner, in its reporter's format.
pub(super) struct ResultsFile {
    pub(super) format: Format,
    pub(super) path: PathBuf,
}

/// The end of a layer's runner, and the log of its run.
struct Finished<'a> {
    ran: Result<cmd::Ran, String>,
    log: &'a Path,
}

/// Reads a layer's results file `main`, with the `extra` files a run may leave out, or gives every
/// wanted check of the layer an error that names the layer's log.
fn read_results(
    wanted: &Wanted,
    layer: &str,
    main: &ResultsFile,
    extra: &[ResultsFile],
    finished: Finished,
) -> Vec<CheckResult> {
    let log = finished.log.display();
    let failure = |message: String| {
        let message = format!("the {layer} layer failed, log {log}: {message}");
        every_wanted(wanted, layer, |check| {
            CheckResult::error(check, layer, message.clone())
        })
    };
    let ran = match finished.ran {
        Ok(ran) => ran,
        Err(error) => return failure(error),
    };
    if ran.timed_out() {
        return every_wanted(wanted, layer, |check| {
            CheckResult::fail(
                check,
                layer,
                FailKind::Timeout,
                format!("the layer ran past its limit, log {log}"),
            )
        });
    }
    let text = match fs::read_to_string(&main.path) {
        Ok(text) => text,
        Err(error) => {
            return failure(format!(
                "{}: {error}\n{}",
                main.path.display(),
                ran.tail(30)
            ));
        }
    };
    let mut tests = match web::parse(main.format, &text) {
        Ok(tests) => tests,
        Err(error) => return failure(error),
    };
    for file in extra {
        // A run that wrote no file reports nothing, and its wanted checks show as reporting no
        // result.
        let Ok(text) = fs::read_to_string(&file.path) else {
            continue;
        };
        match web::parse(file.format, &text) {
            Ok(more) => tests.extend(more),
            Err(error) => return failure(format!("{}: {error}", file.path.display())),
        }
    }
    let registered = wanted.registry.in_layer(layer);
    web::layer_results(layer, tests, &registered, |check| wanted.check(check))
}

fn render(ctx: &Ctx, wanted: &Wanted) -> Vec<CheckResult> {
    let dir = ctx.run_dir.join("render");
    let _ = fs::remove_dir_all(&dir);
    let coverage = ctx.run_dir.join("cover/v8-render");
    let _ = fs::remove_dir_all(&coverage);
    let results = ResultsFile {
        format: Format::Vitest,
        path: dir.join("results.json"),
    };
    let mut command = ctx.command("npx");
    command
        .current_dir(oracle_js(ctx))
        .args(["vitest", "run", "--config", "render/vitest.config.mjs"])
        .args(["--reporter=json", "--reporter=default"])
        .arg(format!("--outputFile.json={}", results.path.display()))
        .env("WTPOOL_VERIFY_RUN_DIR", &ctx.run_dir)
        .env("NODE_V8_COVERAGE", &coverage);
    if let Some(pattern) = wanted.grep() {
        command.args(["--testNamePattern", &pattern]);
    }
    let ran = ctx.run_logged("render", &mut command, LAYER_LIMIT);
    let log = ctx.log_path("render");
    let found = read_results(wanted, "render", &results, &[], Finished { ran, log: &log });
    v8_lcov(ctx, &coverage, "render");
    found
}

fn host(ctx: &Ctx, wanted: &Wanted) -> Vec<CheckResult> {
    if !windows_allowed() {
        return not_yet(wanted, "host");
    }
    if let Some(refused) = socket_refused(wanted, "host") {
        return refused;
    }
    let coverage = ctx.run_dir.join("cover/v8-host");
    let _ = fs::remove_dir_all(&coverage);
    let results = ResultsFile {
        format: Format::Mocha,
        path: ctx.run_dir.join("host/results.json"),
    };
    let mut command = ctx.command("node");
    command
        .current_dir(oracle_js(ctx))
        .arg("host/run.mjs")
        .env("WTPOOL_VERIFY_RUN_DIR", &ctx.run_dir)
        .env("WTPOOL_VERIFY_HOST_COVERAGE", &coverage);
    if let Some(pattern) = wanted.grep() {
        command.env("WTPOOL_VERIFY_HOST_GREP", pattern);
    }
    let _ = fs::remove_file(ctx.run_dir.join("host/keychain.txt"));
    let _ = fs::remove_file(ctx.run_dir.join("host/side-effects.txt"));
    let guard = focus::watch(ctx, "host");
    let ran = ctx.run_logged("host", &mut command, LAYER_LIMIT);
    let front = guard.finish();
    remove_short_links(ctx);
    let log = ctx.log_path("host");
    let mut found = read_results(wanted, "host", &results, &[], Finished { ran, log: &log });
    found.extend(keychain_result(ctx, "host"));
    found.extend(focus::side_effects(ctx, "host"));
    found.extend(front);
    v8_lcov(ctx, &coverage, "host");
    found
}

/// An error when a VS Code of the layer printed a keychain error: no check may make macOS
/// show a keychain dialog. The layer's runner writes the lines to `<layer>/keychain.txt`.
fn keychain_result(ctx: &Ctx, layer: &str) -> Option<CheckResult> {
    let path = ctx.run_dir.join(layer).join("keychain.txt");
    let text = fs::read_to_string(&path).ok()?;
    Some(CheckResult::error(
        &format!("{layer}:keychain"),
        layer,
        format!(
            "a VS Code of the {layer} layer reported a keychain error:\n{}",
            text.lines().take(10).collect::<Vec<_>>().join("\n")
        ),
    ))
}

/// Turns the V8 coverage of a layer into `cover/<layer>.lcov` over vscode/src, with each line's
/// hits through the build's source maps. Without coverage of the product it writes nothing, and
/// `cover` fails for want of the file.
fn v8_lcov(ctx: &Ctx, coverage: &Path, layer: &str) {
    if !coverage.is_dir() {
        return;
    }
    let out = ctx.run_dir.join("cover").join(format!("{layer}.lcov"));
    let _ = fs::remove_file(&out);
    let _ = ctx.run_logged(
        &format!("cover_{layer}"),
        ctx.command("node")
            .current_dir(oracle_js(ctx))
            .arg("support/v8-lcov.mjs")
            .arg(coverage)
            .arg(&out),
        Duration::from_secs(600),
    );
    for suffix in ["", "-vscode", "-report"] {
        let mut folder = coverage.as_os_str().to_owned();
        folder.push(suffix);
        let _ = fs::remove_dir_all(PathBuf::from(folder));
    }
}

/// The ways one Playwright run differs from the default.
#[derive(Default)]
pub(super) struct PlaywrightRun {
    pub(super) envs: Vec<(String, String)>,
    /// Replaces the `--grep` that `--only` gives.
    pub(super) grep: Option<String>,
    /// The folder of the results file in the run folder, the layer's name by default.
    pub(super) folder: Option<&'static str>,
    /// The name of the results file the config writes, `results.json` by default.
    pub(super) results: Option<&'static str>,
    /// The name of the run's log, the layer's name by default.
    pub(super) log: Option<&'static str>,
    /// Further results files read with it, such as an earlier run's or the outcomes
    /// [`parse_extra`](crate::web::parse_extra) reads.
    pub(super) extra: Vec<ResultsFile>,
}

/// Starts one Playwright config and waits for it. `log` names its log file.
pub(super) fn run_config(
    ctx: &Ctx,
    wanted: &Wanted,
    log: &str,
    config: &str,
    run: &PlaywrightRun,
) -> Result<cmd::Ran, String> {
    let mut command = ctx.command("npx");
    command
        .current_dir(oracle_js(ctx))
        // A `--only` check of the other e2e config leaves this one nothing to run, and the
        // checks a run wanted but did not report still show as errors.
        .args(["playwright", "test", "--pass-with-no-tests", "--config"])
        .arg(ctx.root.join(config))
        .env("WTPOOL_VERIFY_RUN_DIR", &ctx.run_dir)
        .envs(run.envs.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    if let Some(pattern) = run.grep.clone().or_else(|| wanted.grep()) {
        command.args(["--grep", &pattern]);
    } else if let Some(pattern) = wanted.grep_invert() {
        command.args(["--grep-invert", &pattern]);
    }
    let ran = ctx.run_logged(log, &mut command, LAYER_LIMIT);
    remove_short_links(ctx);
    ran
}

/// Runs a Playwright config for the registry's `layer`.
pub(super) fn playwright(
    ctx: &Ctx,
    wanted: &Wanted,
    layer: &str,
    config: &str,
    run: PlaywrightRun,
) -> Vec<CheckResult> {
    let folder = run.folder.unwrap_or(layer);
    let results = ResultsFile {
        format: Format::Playwright,
        path: ctx
            .run_dir
            .join(folder)
            .join(run.results.unwrap_or("results.json")),
    };
    let _ = fs::remove_file(&results.path);
    let log = run.log.unwrap_or(layer);
    let ran = run_config(ctx, wanted, log, config, &run);
    let log = ctx.log_path(log);
    let mut found = read_results(
        wanted,
        layer,
        &results,
        &run.extra,
        Finished { ran, log: &log },
    );
    found.extend(keychain_result(ctx, folder));
    found
}

/// Removes the short links of `s/` in the harness cache (vscode/test/support/code.mjs) that point
/// into this run, left behind by a check that stopped before it closed its VS Code.
fn remove_short_links(ctx: &Ctx) {
    let Ok(cache) = crate::dirs::cache_dir() else {
        return;
    };
    let Ok(entries) = fs::read_dir(cache.join(crate::dirs::SHORT_LINKS)) else {
        return;
    };
    for entry in entries.flatten() {
        if fs::read_link(entry.path()).is_ok_and(|target| target.starts_with(&ctx.run_dir)) {
            let _ = fs::remove_file(entry.path());
        }
    }
}
