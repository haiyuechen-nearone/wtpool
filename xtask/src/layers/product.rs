//! The static checks of the Rust workspace and of the generated files.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::Duration;

use super::SETUP_LIMIT;
use super::statics::{Problem, findings_result};
use crate::cmd::{self, git};
use crate::isolation::VERIFY_EMAIL;
use crate::parts::Ctx;
use crate::results::CheckResult;

pub(super) const NO_SOCKET_APIS: &str = "lint__no_socket_apis";

/// Crates whose purpose is networking.
const NETWORK_CRATES: [&str; 31] = [
    "actix-web",
    "async-net",
    "async-std",
    "axum",
    "curl",
    "grpcio",
    "h2",
    "h3",
    "hyper",
    "hyper-util",
    "interprocess",
    "isahc",
    "mio",
    "native-tls",
    "nng",
    "openssl",
    "quinn",
    "reqwest",
    "rocket",
    "rustls",
    "smol",
    "socket2",
    "surf",
    "tarpc",
    "tiny_http",
    "tokio",
    "tonic",
    "tungstenite",
    "ureq",
    "warp",
    "zmq",
];

/// No socket type or call in the workspace, by clippy's `disallowed_types` and
/// `disallowed_methods` of clippy.toml, and none of [`NETWORK_CRATES`] in `cargo tree`.
pub(super) fn no_socket_apis(ctx: &Ctx) -> CheckResult {
    const CHECK: &str = NO_SOCKET_APIS;
    let clippy = ctx.run_logged(
        "lint_no_socket_apis_clippy",
        ctx.command("cargo")
            .args([
                "clippy",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--locked",
            ])
            .args(["--message-format=json", "--quiet", "--"])
            .args([
                "-W",
                "clippy::disallowed_types",
                "-W",
                "clippy::disallowed_methods",
            ]),
        SETUP_LIMIT,
    );
    let clippy = match clippy {
        Ok(ran) => ran,
        Err(error) => return CheckResult::error(CHECK, "static", error),
    };
    let mut found = Vec::new();
    let mut compiled = false;
    for line in clippy.stdout.lines() {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if message.get("reason").and_then(serde_json::Value::as_str) == Some("build-finished") {
            compiled = message.get("success").and_then(serde_json::Value::as_bool) == Some(true);
        }
        let diagnostic = message.get("message");
        let code = diagnostic
            .and_then(|m| m.get("code"))
            .and_then(|c| c.get("code"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if code == "clippy::disallowed_types" || code == "clippy::disallowed_methods" {
            let rendered = diagnostic
                .and_then(|m| m.get("rendered"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            found.push(Problem::Finding(
                rendered.lines().take(4).collect::<Vec<_>>().join("\n"),
            ));
        }
    }
    if !compiled && found.is_empty() {
        return CheckResult::error(
            CHECK,
            "static",
            format!("cargo clippy did not finish: {}", clippy.tail(20)),
        );
    }
    let tree = cmd::output(
        ctx.command("cargo").args([
            "tree",
            "--workspace",
            "--locked",
            "--all-features",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ]),
        Duration::from_secs(300),
    );
    match tree {
        Err(error) => return CheckResult::error(CHECK, "static", error),
        Ok(text) => {
            let crates: BTreeSet<String> = text
                .lines()
                .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
                .collect();
            for name in NETWORK_CRATES {
                if crates.contains(name) {
                    found.push(Problem::Finding(format!(
                        "cargo tree holds the networking crate {name}"
                    )));
                }
            }
        }
    }
    findings_result(CHECK, &found)
}

/// `npm run generate` in a clone of the tree under test rewrites nothing, and every JSON Schema
/// the product tracks accepts every scenario document.
pub(super) fn schema_current(ctx: &Ctx) -> CheckResult {
    const CHECK: &str = "schema__generated_files_current";
    let dir = ctx.run_dir.join("generate");
    let _ = fs::remove_dir_all(&dir);
    let repo = dir.join("repo");
    if let Err(error) = clone_tree(ctx, &repo) {
        return CheckResult::error(CHECK, "static", Problem::Setup(error).to_string());
    }
    let ran = ctx.run_logged(
        "generate",
        ctx.command("npm")
            .current_dir(repo.join("vscode"))
            .args(["run", "generate"])
            .env("CARGO_TARGET_DIR", repo.join("target")),
        SETUP_LIMIT,
    );
    let ran = match ran {
        Ok(ran) => ran,
        Err(error) => return CheckResult::error(CHECK, "static", error),
    };
    if !ran.ok() {
        let _ = fs::remove_dir_all(repo.join("target"));
        return CheckResult::assertion(
            CHECK,
            "static",
            format!("npm run generate failed:\n{}", ran.tail(20)),
        );
    }
    let mut found = Vec::new();
    match git(&repo, &["status", "--porcelain", "--untracked-files=all"]) {
        Ok(status) if status.trim().is_empty() => {}
        Ok(status) => found.push(Problem::Finding(format!(
            "npm run generate changed files the tree holds stale:\n{}",
            status.trim_end()
        ))),
        Err(error) => return CheckResult::error(CHECK, "static", error),
    }
    found.extend(product_schema_problems(&ctx.root));
    let _ = fs::remove_dir_all(repo.join("target"));
    findings_result(CHECK, &found)
}

/// Copies the tree under test, uncommitted changes included, into a committed clone at `repo`
/// with `vscode/node_modules` linked. Its `target/` starts empty: no portable call copies a
/// folder on write, and a plain copy of the tree's would cost its size.
fn clone_tree(ctx: &Ctx, repo: &Path) -> Result<(), String> {
    fs::create_dir_all(repo).map_err(|e| format!("{}: {e}", repo.display()))?;
    git(repo, &["init", "--quiet"])?;
    git(repo, &["config", "user.name", "wtpool-verify"])?;
    git(repo, &["config", "user.email", VERIFY_EMAIL])?;
    git(repo, &["config", "commit.gpgsign", "false"])?;
    let source = ctx.root.to_string_lossy().into_owned();
    git(
        repo,
        &[
            "fetch",
            "--quiet",
            "--no-write-fetch-head",
            &source,
            "+HEAD:refs/heads/generate",
        ],
    )?;
    git(repo, &["checkout", "--quiet", "generate"])?;
    crate::rebuild::copy_uncommitted(&ctx.root, repo)?;
    git(repo, &["add", "-A"])?;
    if crate::repo::dirty_fingerprint(repo)?.is_some() {
        git(
            repo,
            &["commit", "--quiet", "-m", "verify: the tree under test"],
        )?;
    }
    let exclude = repo.join(".git/info/exclude");
    fs::write(&exclude, "/vscode/node_modules\n/target\n").map_err(|e| e.to_string())?;
    let modules = ctx.root.join("vscode/node_modules");
    link_folder(&modules, &repo.join("vscode/node_modules"))
        .map_err(|e| format!("link vscode/node_modules: {e}"))?;
    Ok(())
}

#[cfg(unix)]
fn link_folder(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(original, link)
}

#[cfg(windows)]
fn link_folder(original: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(original, link)
}

/// A JSON document and its name in a problem.
struct Document {
    name: String,
    json: serde_json::Value,
}

/// Every tracked JSON Schema of the product must exist and accept every scenario document.
fn product_schema_problems(root: &Path) -> Vec<Problem> {
    let listed = git(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            "*.json",
        ],
    )
    .unwrap_or_default();
    let mut schemas = Vec::new();
    for rel in listed.split('\0').filter(|r| !r.is_empty()) {
        if crate::oracle::is_oracle_path(rel, 1) || rel.split('/').any(|p| p == "node_modules") {
            continue;
        }
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let meta = json
            .get("$schema")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let describes_status = json
            .get("properties")
            .and_then(|p| p.get("pools"))
            .is_some();
        if meta.contains("json-schema.org/") && describes_status {
            schemas.push(Document {
                name: rel.to_owned(),
                json,
            });
        }
    }
    if schemas.is_empty() {
        return vec![Problem::Finding(
            "the product tracks no generated JSON Schema of the status document".to_owned(),
        )];
    }
    let mut documents = Vec::new();
    for name in crate::scenario::names(root) {
        match crate::scenario::load(root, &name) {
            Ok(scenario) => {
                for (index, document) in scenario.documents().into_iter().enumerate() {
                    documents.push(Document {
                        name: format!("{name} document {}", index.saturating_add(1)),
                        json: document.clone(),
                    });
                }
            }
            Err(error) => return vec![Problem::Setup(error)],
        }
    }
    let mut problems = Vec::new();
    for schema in &schemas {
        let rel = &schema.name;
        let validator = match jsonschema::validator_for(&schema.json) {
            Ok(validator) => validator,
            Err(error) => {
                problems.push(Problem::Finding(format!(
                    "{rel} is no valid JSON Schema: {error}"
                )));
                continue;
            }
        };
        for document in &documents {
            if let Some(error) = validator.iter_errors(&document.json).next() {
                problems.push(Problem::Finding(format!(
                    "{rel} rejects {}: {}: {error}",
                    document.name,
                    error.instance_path()
                )));
            }
        }
    }
    problems
}
