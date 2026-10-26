//! The static checks of the extension's bundles and sources: its lints and the calls its bundles
//! may not make.

use std::fmt;
use std::fs;
use std::path::Path;
use std::time::Duration;

use super::Wanted;
use super::builds::{oracle_js, vscode};
use crate::cmd::Ran;
use crate::parts::Ctx;
use crate::results::CheckResult;

const STATIC_LIMIT: Duration = Duration::from_secs(60);

/// Text in a webview bundle that writes markup or runs strings as code.
const HTML_SINKS: [&str; 9] = [
    "innerHTML",
    "outerHTML",
    "insertAdjacentHTML",
    "document.write",
    "srcdoc",
    "createContextualFragment",
    "DOMParser",
    "setHTMLUnsafe",
    "parseHTMLUnsafe",
];
const NETWORK_MODULES: [&str; 6] = ["net", "http", "https", "tls", "dgram", "http2"];

/// One thing a static check found: a step it could not take, an error of the check, or a finding
/// about the product, an assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Problem {
    Setup(String),
    Finding(String),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Setup(text) => write!(f, "setup: {text}"),
            Self::Finding(text) => f.write_str(text),
        }
    }
}

/// A file of the extension, by the name a problem gives it, and its content.
pub(super) struct Named<T> {
    pub(super) name: String,
    pub(super) content: T,
}

impl<T> Named<T> {
    pub(super) fn new(name: impl Into<String>, content: T) -> Self {
        Self {
            name: name.into(),
            content,
        }
    }
}

/// Files below `dir` with one of `extensions`, named relative to `root`.
fn files_below(root: &Path, dir: &Path, extensions: &[&str]) -> Vec<Named<String>> {
    let skip = |rel: &str| rel.ends_with("node_modules");
    let mut found = Vec::new();
    for rel in crate::repo::walk(root, dir, &skip) {
        if !extensions
            .iter()
            .any(|ext| rel.ends_with(&format!(".{ext}")))
        {
            continue;
        }
        if let Ok(text) = fs::read_to_string(root.join(&rel)) {
            found.push(Named::new(rel, text));
        }
    }
    found
}

/// The built JavaScript the extension and webview run.
fn built_bundles(ctx: &Ctx) -> Vec<Named<String>> {
    let mut found = files_below(&ctx.root, &vscode(ctx).join("out"), &["js", "mjs", "cjs"]);
    found.extend(files_below(
        &ctx.root,
        &vscode(ctx).join("media"),
        &["js", "mjs", "cjs"],
    ));
    found
}

/// Runs an ESLint or stylelint group of vscode/test/lint/check.mjs over vscode/src and media/.
fn lint_group(ctx: &Ctx, group: &str) -> Result<Vec<Problem>, Problem> {
    let ran = ctx.run_logged(
        &format!("lint_{group}"),
        ctx.command("node")
            .current_dir(oracle_js(ctx))
            .arg("lint/check.mjs")
            .arg(group),
        STATIC_LIMIT,
    );
    group_findings(group, ran)
}

/// The findings of one run of a lint group.
fn group_findings(group: &str, ran: Result<Ran, String>) -> Result<Vec<Problem>, Problem> {
    let ran =
        ran.map_err(|error| Problem::Setup(format!("the {group} lint did not start: {error}")))?;
    if ran.timed_out() {
        return Err(Problem::Setup(format!(
            "the {group} lint timed out after {} s",
            STATIC_LIMIT.as_secs()
        )));
    }
    let json: serde_json::Value = serde_json::from_str(ran.stdout.trim()).map_err(|e| {
        Problem::Setup(format!(
            "the {group} lint printed no JSON ({e}): {}",
            ran.tail(20)
        ))
    })?;
    if let Some(error) = json.get("error").and_then(serde_json::Value::as_str) {
        return Err(Problem::Setup(format!("the {group} lint failed: {error}")));
    }
    let mut found = Vec::new();
    for finding in json
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(text) = finding.as_str() {
            found.push(Problem::Finding(text.to_owned()));
        }
    }
    Ok(found)
}

/// The findings of a lint group, or the one problem that kept it from running.
fn lint_findings(ctx: &Ctx, group: &str) -> Vec<Problem> {
    lint_group(ctx, group).unwrap_or_else(|problem| vec![problem])
}

/// Lines of `text` that `matches`, as `file:line: <match>`.
pub fn text_findings(
    file: &str,
    text: &str,
    matches: impl Fn(&str) -> Option<String>,
) -> Vec<String> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if let Some(what) = matches(line) {
            found.push(format!("{file}:{}: {what}", index.saturating_add(1)));
        }
    }
    found
}

/// The [`text_findings`] of each file, as findings.
fn findings_in(files: &[Named<String>], matches: impl Fn(&str) -> Option<String>) -> Vec<Problem> {
    let mut found = Vec::new();
    for file in files {
        for finding in text_findings(&file.name, &file.content, &matches) {
            found.push(Problem::Finding(finding));
        }
    }
    found
}

/// A network module a line of a bundle requires or imports.
pub fn network_import(line: &str) -> Option<String> {
    for module in NETWORK_MODULES {
        for quote in ['"', '\''] {
            for name in [module.to_owned(), format!("node:{module}")] {
                let quoted = format!("{quote}{name}{quote}");
                let mut rest = line;
                while let Some(at) = rest.find(&quoted) {
                    let before = rest.get(..at).unwrap_or_default().trim_end();
                    if before.ends_with("require(")
                        || before.ends_with("from")
                        || before.ends_with("import(")
                        || before.ends_with("import")
                    {
                        return Some(format!("imports {name}"));
                    }
                    rest = rest
                        .get(at.saturating_add(quoted.len())..)
                        .unwrap_or_default();
                }
            }
        }
    }
    None
}

/// A listening call in a line of a bundle.
pub fn listen_call(line: &str) -> Option<String> {
    for word in [".listen(", "createServer(", "createSocket("] {
        if line.contains(word) {
            return Some(
                word.trim_start_matches('.')
                    .trim_end_matches('(')
                    .to_owned(),
            );
        }
    }
    None
}

/// The first character of a line outside ASCII, by code point and column. The coverage converter
/// needs each bundle's size in bytes to equal its length in UTF-16 units, as V8 measures it.
fn non_ascii(line: &str) -> Option<String> {
    line.chars()
        .enumerate()
        .find(|(_, character)| !character.is_ascii())
        .map(|(index, character)| {
            format!(
                "U+{:04X} at column {}",
                u32::from(character),
                index.saturating_add(1)
            )
        })
}

/// The first HTML sink a line of a webview bundle names.
fn html_sink(line: &str) -> Option<String> {
    HTML_SINKS
        .iter()
        .find(|sink| line.contains(*sink))
        .map(|sink| (*sink).to_owned())
}

/// The static layer's checks that verify/checks.toml does not list. `--only` still names them.
pub(super) const UNREGISTERED: [&str; 4] = [
    "static:ascii_bundles",
    "static:eslint",
    "static:prettier",
    "static:stylelint",
];

pub(super) fn static_checks(ctx: &Ctx, wanted: &Wanted) -> Vec<CheckResult> {
    let mut results = Vec::new();
    if wanted.check("lint__no_html_sinks") {
        let mut found = lint_findings(ctx, "html-sinks");
        let webview = files_below(&ctx.root, &vscode(ctx).join("media"), &["js", "mjs", "cjs"]);
        found.extend(findings_in(&webview, html_sink));
        results.push(findings_result("lint__no_html_sinks", &found));
    }
    if wanted.check("lint__no_colour_literals") {
        let found = lint_findings(ctx, "colour");
        results.push(findings_result("lint__no_colour_literals", &found));
    }
    if wanted.check("lint__no_network_modules") {
        let mut found = lint_findings(ctx, "network");
        found.extend(findings_in(&built_bundles(ctx), network_import));
        results.push(findings_result("lint__no_network_modules", &found));
    }
    if wanted.check("bundle__should_contain_no_listen_call") {
        let found = findings_in(&built_bundles(ctx), listen_call);
        results.push(findings_result(
            "bundle__should_contain_no_listen_call",
            &found,
        ));
    }
    if wanted.check("static:ascii_bundles") {
        let found = findings_in(&built_bundles(ctx), non_ascii);
        results.push(findings_result("static:ascii_bundles", &found));
    }
    if wanted.check("schema__generated_files_current") {
        results.push(super::product::schema_current(ctx));
    }
    if wanted.check("static:eslint") {
        results.push(
            ctx.command_check(
                "static:eslint",
                "static",
                ctx.command("npx")
                    .current_dir(vscode(ctx))
                    .args(["eslint", "."]),
                STATIC_LIMIT,
            ),
        );
    }
    if wanted.check("static:prettier") {
        results.push(
            ctx.command_check(
                "static:prettier",
                "static",
                ctx.command("npm")
                    .current_dir(vscode(ctx))
                    .args(["run", "format:check"]),
                STATIC_LIMIT,
            ),
        );
    }
    if wanted.check("static:stylelint") {
        results.push(ctx.command_check(
            "static:stylelint",
            "static",
            ctx.command("npx").current_dir(vscode(ctx)).args([
                "stylelint",
                "--allow-empty-input",
                "src/**/*.css",
                "media/**/*.css",
            ]),
            STATIC_LIMIT,
        ));
    }
    results
}

/// A lint result: a setup problem makes it an error, findings an assertion.
pub(super) fn findings_result(check: &str, found: &[Problem]) -> CheckResult {
    if let Some(setup) = found.iter().find(|p| matches!(p, Problem::Setup(_))) {
        return CheckResult::error(check, "static", setup.to_string());
    }
    if found.is_empty() {
        return CheckResult::pass(check, "static", "");
    }
    let lines: Vec<String> = found.iter().map(Problem::to_string).collect();
    CheckResult::assertion(check, "static", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::{FailKind, Outcome};
    use rstest::rstest;
    use std::process::ExitStatus;

    #[rstest]
    #[case::none(&[], Outcome::Pass, None, "")]
    #[case::findings(
        &[Problem::Finding("a.js:1: x".into()), Problem::Finding("b.js:2: y".into())],
        Outcome::Fail,
        Some(FailKind::Assertion),
        "a.js:1: x\nb.js:2: y"
    )]
    #[case::setup_first(
        &[Problem::Finding("a.js:1: x".into()), Problem::Setup("the colour lint failed: no eslint".into())],
        Outcome::Fail,
        Some(FailKind::Error),
        "setup: the colour lint failed: no eslint"
    )]
    fn findings_result__should_make_a_setup_problem_an_error(
        #[case] found: &[Problem],
        #[case] outcome: Outcome,
        #[case] kind: Option<FailKind>,
        #[case] message: &str,
    ) {
        // When
        let result = findings_result("lint__no_colour_literals", found);

        // Then
        assert_eq!(
            (result.outcome, result.kind, result.message.as_str()),
            (outcome, kind, message)
        );
    }

    fn ran(status: Option<ExitStatus>, stdout: &str) -> Ran {
        Ran {
            status,
            stdout: stdout.to_owned(),
            stderr: String::new(),
            seconds: 0.0,
        }
    }

    #[rstest]
    #[case::did_not_start(
        Err("cannot start \"node\": No such file or directory".to_owned()),
        "the colour lint did not start: cannot start \"node\": No such file or directory"
    )]
    #[case::timed_out(Ok(ran(None, "")), "the colour lint timed out after 60 s")]
    fn group_findings__should_make_a_runner_that_did_not_finish_a_setup_problem(
        #[case] run: Result<Ran, String>,
        #[case] message: &str,
    ) {
        // When
        let found = group_findings("colour", run);

        // Then
        assert_eq!(found, Err(Problem::Setup(message.to_owned())));
    }

    #[test]
    fn group_findings__should_keep_a_reported_finding_a_finding() {
        // Given
        let run = Ok(ran(
            Some(ExitStatus::default()),
            r#"{"findings": ["src/panel.css:3: color-no-hex"]}"#,
        ));

        // When
        let found = group_findings("colour", run);

        // Then
        assert_eq!(
            found,
            Ok(vec![Problem::Finding(
                "src/panel.css:3: color-no-hex".to_owned()
            )])
        );
    }

    #[rstest]
    #[case::require("const n = require(\"net\");", Some("imports net"))]
    #[case::node_prefix("import x from 'node:https';", Some("imports node:https"))]
    #[case::dynamic("await import(\"dgram\")", Some("imports dgram"))]
    #[case::text_only("const label = \"net\";", None)]
    #[case::other_module("require(\"child_process\")", None)]
    fn network_import__should_find_requires_and_imports(
        #[case] line: &str,
        #[case] expected: Option<&str>,
    ) {
        // Given
        let text = line;

        // When
        let found = network_import(text);

        // Then
        assert_eq!(found.as_deref(), expected);
    }

    #[rstest]
    #[case::ascii("const dot = \"\\u00b7\";", None)]
    #[case::middle_dot("/** a \u{b7} b */", Some("U+00B7 at column 7"))]
    #[case::astral("x = '\u{1f600}'", Some("U+1F600 at column 6"))]
    fn non_ascii__should_name_the_first_character_outside_ascii(
        #[case] line: &str,
        #[case] expected: Option<&str>,
    ) {
        // Given
        let text = line;

        // When
        let found = non_ascii(text);

        // Then
        assert_eq!(found.as_deref(), expected);
    }

    #[rstest]
    #[case::listen("server.listen(0)", Some("listen"))]
    #[case::create_server("http.createServer(handler)", Some("createServer"))]
    #[case::udp("dgram.createSocket('udp4')", Some("createSocket"))]
    #[case::listener_word("addEventListener('message', f)", None)]
    fn listen_call__should_find_listening_calls(
        #[case] line: &str,
        #[case] expected: Option<&str>,
    ) {
        // Given
        let text = line;

        // When
        let found = listen_call(text);

        // Then
        assert_eq!(found.as_deref(), expected);
    }
}
