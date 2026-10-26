//! The quick part's own checks, before its web layers (crate::layers::quick): the harness's self
//! test and lints, the oracle and repository checks, and the static Rust checks of the product.

use std::time::Duration;

use super::{Ctx, NEXTEST_CONFIG, PART_LIMIT};
use crate::results::CheckResult;
use crate::{context, doclinks, oracle, suppress};

const STATIC_LIMIT: Duration = Duration::from_secs(60);

/// The cargo arguments that pick the harness's own workspace, `xtask/`, left out of the
/// product's `--workspace`.
const HARNESS: [&str; 2] = ["--manifest-path", "xtask/Cargo.toml"];

pub(super) fn quick(ctx: &Ctx) -> Vec<CheckResult> {
    let mut results = Vec::new();
    if ctx.wants("harness:self_test") {
        results.push(
            ctx.command_check(
                "harness:self_test",
                "harness",
                ctx.command("cargo")
                    .args(["nextest", "run", "--locked"])
                    .args(HARNESS)
                    .arg("--lib")
                    .args(["--profile", "harness", "--config-file", NEXTEST_CONFIG]),
                PART_LIMIT,
            ),
        );
    }
    let harness_fmt = [HARNESS.as_slice(), &["--all"]].concat();
    lints(ctx, "harness", &harness_fmt, &HARNESS, &mut results);
    if ctx.wants("oracle_unchanged") {
        results.push(oracle::oracle_unchanged(&ctx.root, ctx.phase));
    }
    if ctx.wants("repo_context__should_be_unchanged") {
        results.push(context::check(&ctx.root));
    }
    if ctx.wants("lint__no_suppressions") {
        results.push(suppress::check(&ctx.root, ctx.phase));
    }
    if ctx.wants("static:doc_links") {
        results.push(doclinks::check(&ctx.root));
    }
    lints(ctx, "static", &["--all"], &["--workspace"], &mut results);
    results
}

/// `<layer>:fmt`, `<layer>:clippy` and `<layer>:rustdoc` of one workspace, `fmt` picking it for
/// `cargo fmt` and `workspace` for the other cargo commands. The product's clippy also runs
/// with every feature, as `static:clippy_all_features`.
fn lints(ctx: &Ctx, layer: &str, fmt: &[&str], workspace: &[&str], results: &mut Vec<CheckResult>) {
    let check = format!("{layer}:fmt");
    if ctx.wants(&check) {
        results.push(ctx.command_check(
            &check,
            layer,
            ctx.command("cargo").arg("fmt").args(fmt).arg("--check"),
            STATIC_LIMIT,
        ));
    }
    let mut clippy_variants: Vec<(String, &[&str])> = vec![(format!("{layer}:clippy"), &[])];
    if layer == "static" {
        clippy_variants.push(("static:clippy_all_features".to_owned(), &["--all-features"]));
    }
    for (check, features) in clippy_variants {
        if !ctx.wants(&check) {
            continue;
        }
        // Clippy's own limit counts only its lints: the warm check builds the dependencies.
        let warm = ctx.run_logged(
            &format!("{}_warm", check.replace(':', "_")),
            ctx.command("cargo")
                .arg("check")
                .args(workspace)
                .args(["--all-targets", "--locked"])
                .args(features),
            PART_LIMIT,
        );
        if let Err(error) = warm {
            results.push(CheckResult::error(&check, layer, error));
            continue;
        }
        results.push(
            ctx.command_check(
                &check,
                layer,
                ctx.command("cargo")
                    .arg("clippy")
                    .args(workspace)
                    .args(["--all-targets", "--locked"])
                    .args(features)
                    .args(["--", "-D", "warnings"]),
                STATIC_LIMIT,
            ),
        );
    }
    let check = format!("{layer}:rustdoc");
    if ctx.wants(&check) {
        results.push(
            ctx.command_check(
                &check,
                layer,
                ctx.command("cargo")
                    .arg("doc")
                    .args(workspace)
                    .args(["--no-deps", "--document-private-items", "--locked"])
                    .env("RUSTDOCFLAGS", "-D warnings"),
                STATIC_LIMIT,
            ),
        );
    }
}
