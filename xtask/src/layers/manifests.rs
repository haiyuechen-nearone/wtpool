//! The static checks of the extension's manifests: `vscode/package.json` and the one packaged in
//! `vscode/wtpool.vsix`, with the bundles they name.

use std::fmt;
use std::fs;
use std::process::Command;
use std::time::Duration;

use super::Wanted;
use super::builds::vscode;
use super::statics::{Named, Problem};
use crate::cmd;
use crate::parts::Ctx;
use crate::results::CheckResult;

/// Words in a bundle that would contribute a view or a status bar item.
const VIEW_CALLS: [&str; 3] = [
    "createTreeView",
    "registerTreeDataProvider",
    "registerWebviewViewProvider",
];
const STATUS_BAR_CALLS: [&str; 2] = ["createStatusBarItem", "setStatusBarMessage"];
const VIEW_CONTRIBUTIONS: [&str; 3] = ["views", "viewsContainers", "viewsWelcome"];
fn manifest_problem_result(check: &str, problems: Vec<String>, what: &str) -> CheckResult {
    if problems.is_empty() {
        CheckResult::pass(check, "static", what)
    } else {
        CheckResult::assertion(check, "static", problems.join("\n"))
    }
}

/// `vscode/package.json` and the manifest packaged in `vscode/wtpool.vsix`.
fn manifests(ctx: &Ctx) -> Result<[Named<serde_json::Value>; 2], Problem> {
    let dir = vscode(ctx);
    let setup = |e: &dyn fmt::Display| Problem::Setup(format!("vscode/package.json: {e}"));
    let text = fs::read_to_string(dir.join("package.json")).map_err(|e| setup(&e))?;
    let source: serde_json::Value = serde_json::from_str(&text).map_err(|e| setup(&e))?;
    let packaged = vsix_file(ctx, "extension/package.json")?;
    let packaged: serde_json::Value = serde_json::from_str(&packaged).map_err(|e| {
        Problem::Finding(format!(
            "the packaged extension/package.json is no JSON: {e}"
        ))
    })?;
    Ok([
        Named::new("vscode/package.json", source),
        Named::new("wtpool.vsix extension/package.json", packaged),
    ])
}

/// A file of the packaged extension. A missing package is the product's failure.
fn vsix_file(ctx: &Ctx, path: &str) -> Result<String, Problem> {
    let vsix = vscode(ctx).join("wtpool.vsix");
    if !vsix.exists() {
        return Err(Problem::Finding(
            "npm run package wrote no vscode/wtpool.vsix".to_owned(),
        ));
    }
    cmd::output(
        Command::new("unzip").arg("-p").arg(&vsix).arg(path),
        Duration::from_secs(30),
    )
    .map_err(|e| Problem::Finding(format!("wtpool.vsix holds no {path}: {e}")))
}

/// The extension's bundles: the built `out/extension.js` and the one the package ships.
fn extension_bundles(
    ctx: &Ctx,
    packaged: &serde_json::Value,
) -> Result<[Named<String>; 2], Problem> {
    let built = vscode(ctx).join("out/extension.js");
    let text = fs::read_to_string(&built).map_err(|_| {
        Problem::Finding("npm run build wrote no vscode/out/extension.js".to_owned())
    })?;
    let main = packaged
        .get("main")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("./out/extension.js")
        .trim_start_matches("./");
    let main = if main.ends_with(".js") {
        main.to_owned()
    } else {
        format!("{main}.js")
    };
    let shipped = vsix_file(ctx, &format!("extension/{main}"))?;
    Ok([
        Named::new("vscode/out/extension.js", text),
        Named::new(format!("wtpool.vsix extension/{main}"), shipped),
    ])
}

const MANIFEST_CHECKS: [&str; 4] = [
    "manifest__should_contribute_no_views",
    "manifest__should_contribute_no_status_bar_item",
    "manifest__should_be_a_ui_extension",
    "manifest__should_scope_wtpool_path_to_application",
];

pub(super) fn manifest_checks(ctx: &Ctx, wanted: &Wanted) -> Vec<CheckResult> {
    let names: Vec<&str> = MANIFEST_CHECKS
        .into_iter()
        .filter(|name| wanted.check(name))
        .collect();
    if names.is_empty() {
        return Vec::new();
    }
    let loaded = manifests(ctx).and_then(|manifests| {
        let [_, packaged] = &manifests;
        let bundles = extension_bundles(ctx, &packaged.content)?;
        Ok((manifests, bundles))
    });
    let (manifests, bundles) = match loaded {
        Ok(loaded) => loaded,
        Err(problem) => {
            let make = match problem {
                Problem::Setup(_) => CheckResult::error,
                Problem::Finding(_) => CheckResult::assertion,
            };
            let message = problem.to_string();
            return names
                .iter()
                .map(|name| make(name, "static", message.clone()))
                .collect();
        }
    };
    let mut results = Vec::new();
    for name in names {
        let problems = match name {
            "manifest__should_contribute_no_views" => {
                let mut problems = contribution_problems(&manifests, &VIEW_CONTRIBUTIONS);
                problems.extend(bundle_problems(&bundles, &VIEW_CALLS));
                problems
            }
            "manifest__should_contribute_no_status_bar_item" => {
                bundle_problems(&bundles, &STATUS_BAR_CALLS)
            }
            "manifest__should_be_a_ui_extension" => ui_extension_problems(&manifests),
            _ => manifests
                .iter()
                .filter_map(|manifest| {
                    wtpool_path_problem(&manifest.content)
                        .map(|p| format!("{}: {p}", manifest.name))
                })
                .collect(),
        };
        results.push(manifest_problem_result(
            name,
            problems,
            "both manifests and bundles",
        ));
    }
    results
}

fn ui_extension_problems(manifests: &[Named<serde_json::Value>]) -> Vec<String> {
    let mut problems = Vec::new();
    for manifest in manifests {
        let kind = manifest.content.get("extensionKind");
        if kind != Some(&serde_json::json!(["ui"])) {
            problems.push(format!(
                "{}: extensionKind is {}, not [\"ui\"]",
                manifest.name,
                kind.unwrap_or(&serde_json::Value::Null)
            ));
        }
    }
    problems
}

fn contribution_problems(manifests: &[Named<serde_json::Value>], keys: &[&str]) -> Vec<String> {
    let mut problems = Vec::new();
    for manifest in manifests {
        for key in keys {
            if manifest
                .content
                .get("contributes")
                .and_then(|c| c.get(*key))
                .is_some()
            {
                problems.push(format!("{}: contributes.{key}", manifest.name));
            }
        }
    }
    problems
}

fn bundle_problems(bundles: &[Named<String>], words: &[&str]) -> Vec<String> {
    let mut problems = Vec::new();
    for bundle in bundles {
        for word in words {
            if bundle.content.contains(word) {
                problems.push(format!("{} calls {word}", bundle.name));
            }
        }
    }
    problems
}

/// The problem of the `wtpool.path` setting of a manifest, if it has one.
pub fn wtpool_path_problem(manifest: &serde_json::Value) -> Option<String> {
    let configuration = manifest
        .get("contributes")
        .and_then(|c| c.get("configuration"));
    let blocks: Vec<&serde_json::Value> = match configuration {
        Some(serde_json::Value::Array(items)) => items.iter().collect(),
        Some(block) => vec![block],
        None => Vec::new(),
    };
    let setting = blocks
        .iter()
        .find_map(|block| block.get("properties").and_then(|p| p.get("wtpool.path")));
    let Some(setting) = setting else {
        return Some("no wtpool.path setting".to_owned());
    };
    match setting.get("scope").and_then(serde_json::Value::as_str) {
        Some("application") => None,
        other => Some(format!(
            "wtpool.path has scope {other:?}, not \"application\""
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::object(serde_json::json!({"contributes": {"configuration": {"properties": {"wtpool.path": {"scope": "application"}}}}}), None)]
    #[case::array(serde_json::json!({"contributes": {"configuration": [{"properties": {"wtpool.path": {"scope": "application"}}}]}}), None)]
    #[case::machine(serde_json::json!({"contributes": {"configuration": {"properties": {"wtpool.path": {"scope": "machine"}}}}}), Some("wtpool.path has scope Some(\"machine\"), not \"application\""))]
    #[case::absent(serde_json::json!({"contributes": {}}), Some("no wtpool.path setting"))]
    fn wtpool_path_problem__should_require_application_scope(
        #[case] manifest: serde_json::Value,
        #[case] expected: Option<&str>,
    ) {
        // Given
        let json = manifest;

        // When
        let problem = wtpool_path_problem(&json);

        // Then
        assert_eq!(problem.as_deref(), expected);
    }
}
