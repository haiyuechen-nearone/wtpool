//! The product builds the layers test through, each once per run: the release `wtpool`, the npm
//! installs, `npm run build` and `npm run package` in vscode/, and the Chromium that Playwright
//! drives.

use std::fs;
use std::path::{Path, PathBuf};

use super::SETUP_LIMIT;
use crate::parts::Ctx;
use crate::results::{CheckResult, FailKind, Outcome};

pub(super) fn vscode(ctx: &Ctx) -> PathBuf {
    ctx.root.join("vscode")
}

pub(super) fn oracle_js(ctx: &Ctx) -> PathBuf {
    ctx.root.join("vscode/test")
}

/// Builds the release `wtpool` as `cargo build --release` installs it, once per run, and returns
/// its path. The first part that needs it builds it and copies it to `<run>/bin/wtpool` with the
/// hash of its inputs. Every later part of the run, in parallel or not, takes that copy while the
/// hash holds.
pub fn release_wtpool(ctx: &Ctx) -> Result<PathBuf, String> {
    let dir = ctx.run_dir.join("bin");
    let copy = dir.join("wtpool");
    let stamp = dir.join("wtpool.inputs");
    // `int` lists exactly the Rust and Cargo files a build reads.
    let hash = crate::reuse::build_hash(&ctx.root, "int")?;
    if copy.exists() && fs::read_to_string(&stamp).is_ok_and(|text| text.trim() == hash) {
        return Ok(copy);
    }
    let package = crate::rebuild::wtpool_package(ctx)?.name;
    let ran = ctx.run_logged(
        "build_release",
        ctx.command("cargo").args([
            "build",
            "--release",
            "--locked",
            "-p",
            &package,
            "--bin",
            "wtpool",
        ]),
        SETUP_LIMIT,
    )?;
    if !ran.ok() {
        return Err(ran.tail(30));
    }
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let built = ctx.root.join("target/release/wtpool");
    let partial = dir.join("wtpool.partial");
    fs::copy(&built, &partial).map_err(|e| format!("{}: {e}", built.display()))?;
    fs::rename(&partial, &copy).map_err(|e| e.to_string())?;
    fs::write(&stamp, &hash).map_err(|e| e.to_string())?;
    Ok(copy)
}

/// Installs a package folder's locked dependencies once per lockfile.
pub(super) fn npm_ci(ctx: &Ctx, dir: &Path, name: &str) -> Result<(), String> {
    let lock = fs::read(dir.join("package-lock.json"))
        .map_err(|e| format!("{}: {e}", dir.join("package-lock.json").display()))?;
    let hash = crate::repo::sha256(&lock);
    let stamp = dir.join("node_modules/.wtpool-verify-lock");
    if fs::read_to_string(&stamp).is_ok_and(|text| text.trim() == hash) {
        return Ok(());
    }
    let ran = ctx.run_logged(
        name,
        ctx.command("npm")
            .current_dir(dir)
            .args(["ci", "--no-audit", "--no-fund"]),
        SETUP_LIMIT,
    )?;
    if !ran.ok() {
        return Err(ran.tail(30));
    }
    fs::write(&stamp, hash).map_err(|e| format!("{}: {e}", stamp.display()))
}

/// Installs the Chromium that the locked Playwright of vscode/test drives, missing on a machine
/// that never ran it. Playwright downloads only the missing parts, so on a machine that has them
/// the step takes about a second. An error names the failed step.
pub(super) fn chromium(ctx: &Ctx, layer: &str) -> Option<CheckResult> {
    let ran = ctx.run_logged(
        "playwright_install",
        ctx.command("npx")
            .current_dir(oracle_js(ctx))
            .args(["playwright", "install", "chromium"]),
        SETUP_LIMIT,
    );
    let error = match ran {
        Ok(ran) if ran.ok() => return None,
        Ok(ran) => ran.tail(30),
        Err(error) => error,
    };
    Some(CheckResult::error("web:playwright_install", layer, error))
}

/// Runs `npm run <script>` in vscode/ as a harness step whose failure is the product's.
fn npm_script(ctx: &Ctx, script: &str) -> CheckResult {
    let check = format!("web:{script}");
    let mut result = ctx.command_check(
        &check,
        "static",
        ctx.command("npm")
            .current_dir(vscode(ctx))
            .args(["run", script]),
        SETUP_LIMIT,
    );
    if result.kind == Some(FailKind::Assertion) {
        result.kind = Some(FailKind::Error);
    }
    result
}

/// An npm script of vscode/ and the files it writes.
struct Script {
    name: &'static str,
    outputs: Vec<PathBuf>,
}

/// The npm installs, `npm run build` and, with `package`, `npm run package`, once per run. The
/// first part that needs them builds them and records the hash of `vscode/` in
/// `<run>/web/<script>.inputs`. A later part of the run takes the outputs while the hash holds
/// and the outputs exist. A stale build is removed before it is rebuilt.
pub(super) fn web_build(ctx: &Ctx, layer: &str, package: bool) -> Vec<CheckResult> {
    for (dir, name) in [(oracle_js(ctx), "npm_ci_oracle"), (vscode(ctx), "npm_ci")] {
        if let Err(error) = npm_ci(ctx, &dir, name) {
            return vec![CheckResult::error(&format!("web:{name}"), layer, error)];
        }
    }
    let hash = match crate::reuse::build_hash(&ctx.root, "ui") {
        Ok(hash) => hash,
        Err(error) => return vec![CheckResult::error("web:inputs", layer, error)],
    };
    let stamps = ctx.run_dir.join("web");
    let _ = fs::create_dir_all(&stamps);
    let stamp = |script: &Script| stamps.join(format!("{}.inputs", script.name));
    let fresh = |script: &Script| {
        fs::read_to_string(stamp(script)).is_ok_and(|text| text.trim() == hash)
            && script.outputs.iter().all(|path| path.exists())
    };
    let dir = vscode(ctx);
    let mut scripts = vec![Script {
        name: "build",
        outputs: vec![dir.join("out/html.js"), dir.join("media/webview.js")],
    }];
    if package {
        scripts.push(Script {
            name: "package",
            outputs: vec![dir.join("wtpool.vsix")],
        });
    }
    if !scripts.iter().all(fresh) {
        remove_build_outputs(ctx);
        for script in &scripts {
            let _ = fs::remove_file(stamp(script));
        }
    }
    let mut results = Vec::new();
    for script in &scripts {
        if fresh(script) {
            results.push(CheckResult::pass(
                &format!("web:{}", script.name),
                layer,
                "built once for this run",
            ));
            continue;
        }
        let result = npm_script(ctx, script.name);
        let passed = result.outcome == Outcome::Pass;
        if passed {
            let _ = fs::write(stamp(script), &hash);
        }
        results.push(result);
        if !passed {
            break;
        }
    }
    results
}

/// The web build and package of a run, done before its parts start.
pub fn web_build_once(ctx: &Ctx) -> Vec<CheckResult> {
    web_build(ctx, "static", true)
}

/// True when every result of a build step passed.
pub(super) fn built(results: &[CheckResult]) -> bool {
    results.iter().all(|result| result.outcome == Outcome::Pass)
}

/// Removes the outputs of an earlier build, so no check reads a stale build.
fn remove_build_outputs(ctx: &Ctx) {
    let dir = vscode(ctx);
    let _ = fs::remove_dir_all(dir.join("out"));
    let _ = fs::remove_file(dir.join("wtpool.vsix"));
    let Ok(entries) = fs::read_dir(dir.join("media")) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("webview.js")
        {
            let _ = fs::remove_file(entry.path());
        }
    }
}
