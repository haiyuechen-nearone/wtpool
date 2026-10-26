use std::fs;
use std::path::Path;

use crate::context::is_own_output_dir;
use crate::oracle::is_oracle_path;
use crate::repo::walk;
use crate::results::CheckResult;

const NAME: &str = "lint__no_suppressions";
/// The farthest the `WTPOOL_VERIFY_PANIC` read may sit below the `#[expect(clippy::panic)]` that
/// guards it, in lines.
const PANIC_EXCEPTION_WINDOW: usize = 15;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Finding {
    pub path: String,
    pub line: usize,
    pub what: String,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Attribute {
    pub line: usize,
    /// The attribute between `#[` and `]`, whitespace outside string literals removed.
    pub body: String,
}

/// Every outer and inner attribute in Rust source text. Comments and string literals are not
/// skipped, and scanning goes on right after each `#`, so a lone `#[` in a comment or string never
/// hides a later attribute. Attribute text inside a comment or string counts too.
pub fn attributes(text: &str) -> Vec<Attribute> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut line = 1;
    let mut index = 0;
    while let Some(&c) = chars.get(index) {
        if c == '\n' {
            line += 1;
        }
        if c != '#' {
            index += 1;
            continue;
        }
        let mut cursor = index + 1;
        if chars.get(cursor) == Some(&'!') {
            cursor += 1;
        }
        while chars.get(cursor).is_some_and(|c| c.is_whitespace()) {
            cursor += 1;
        }
        if chars.get(cursor) != Some(&'[') {
            index += 1;
            continue;
        }
        found.push(Attribute {
            line,
            body: bracket_body(&chars, cursor + 1),
        });
        index += 1;
    }
    found
}

/// Reads from just after `[` to its matching `]`, or to the end of the text.
fn bracket_body(chars: &[char], start: usize) -> String {
    let mut body = String::new();
    let mut depth = 1usize;
    let mut index = start;
    let mut in_string = false;
    while let Some(&c) = chars.get(index) {
        index += 1;
        if in_string {
            body.push(c);
            if c == '\\' {
                if let Some(&next) = chars.get(index) {
                    body.push(next);
                    index += 1;
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                body.push(c);
            }
            '[' => {
                depth += 1;
                body.push(c);
            }
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                body.push(c);
            }
            c if c.is_whitespace() => {}
            c => body.push(c),
        }
    }
    body
}

pub fn is_suppression(body: &str) -> bool {
    body.starts_with("allow(")
        || body.starts_with("expect(")
        || (body.starts_with("cfg_attr(")
            && (body.contains(",allow(") || body.contains(",expect(")))
        || body.contains("rustfmt::skip")
        || body.contains("rustfmt_skip")
}

pub fn is_panic_exception(body: &str) -> bool {
    body == "expect(clippy::panic)" || body.starts_with("expect(clippy::panic,reason=")
}

pub struct RustScan {
    pub findings: Vec<Finding>,
    /// Lines of `#[expect(clippy::panic)]` attributes guarding a `WTPOOL_VERIFY_PANIC` read.
    pub panic_exceptions: Vec<usize>,
}

pub fn scan_rust(path: &str, text: &str) -> RustScan {
    let lines: Vec<&str> = text.lines().collect();
    let mut findings = Vec::new();
    let mut panic_exceptions = Vec::new();
    for attribute in attributes(text) {
        if !is_suppression(&attribute.body) {
            continue;
        }
        let window = lines
            .iter()
            .skip(attribute.line.saturating_sub(1))
            .take(PANIC_EXCEPTION_WINDOW + 1);
        let guards_panic_hook = window
            .clone()
            .any(|line| line.contains("WTPOOL_VERIFY_PANIC"));
        if is_panic_exception(&attribute.body) && guards_panic_hook {
            panic_exceptions.push(attribute.line);
        } else {
            findings.push(Finding {
                path: path.to_owned(),
                line: attribute.line,
                what: format!("#[{}]", attribute.body),
            });
        }
    }
    RustScan {
        findings,
        panic_exceptions,
    }
}

pub fn scan_web(path: &str, text: &str) -> Vec<Finding> {
    const MARKERS: [&str; 4] = [
        "eslint-disable",
        "stylelint-disable",
        "/* eslint ",
        "/*eslint ",
    ];
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            MARKERS
                .iter()
                .find(|marker| line.contains(*marker))
                .map(|marker| Finding {
                    path: path.to_owned(),
                    line: index + 1,
                    what: (*marker).trim().to_owned(),
                })
        })
        .collect()
}

/// A member manifest must inherit the workspace lints and set no lint of its own.
pub fn scan_manifest(path: &str, text: &str) -> Vec<Finding> {
    let finding = |what: &str| Finding {
        path: path.to_owned(),
        line: 0,
        what: what.to_owned(),
    };
    let Ok(table) = text.parse::<toml::Table>() else {
        return vec![finding("manifest does not parse")];
    };
    if !table.contains_key("package") {
        return Vec::new();
    }
    let Some(lints) = table.get("lints").and_then(toml::Value::as_table) else {
        return vec![finding("no `[lints] workspace = true`")];
    };
    let inherits = lints.get("workspace").and_then(toml::Value::as_bool) == Some(true);
    if inherits && lints.len() == 1 {
        Vec::new()
    } else {
        vec![finding("[lints] must be exactly `workspace = true`")]
    }
}

/// Cargo configuration must not pass lint levels or point clippy elsewhere.
pub fn scan_cargo_config(path: &str, text: &str) -> Vec<Finding> {
    const FORBIDDEN_ENV: [&str; 6] = [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_RUSTFLAGS",
        "RUSTDOCFLAGS",
        "CLIPPY_CONF_DIR",
        "CLIPPY_ARGS",
    ];
    let finding = |what: String| Finding {
        path: path.to_owned(),
        line: 0,
        what,
    };
    let Ok(table) = text.parse::<toml::Table>() else {
        return vec![finding("config does not parse".to_owned())];
    };
    let mut findings = Vec::new();
    if let Some(env) = table.get("env").and_then(toml::Value::as_table) {
        for key in env
            .keys()
            .filter(|key| FORBIDDEN_ENV.contains(&key.as_str()))
        {
            findings.push(finding(format!("[env] {key}")));
        }
    }
    let mut flag_lists: Vec<(String, &toml::Value)> = Vec::new();
    if let Some(build) = table.get("build").and_then(toml::Value::as_table) {
        for key in ["rustflags", "rustdocflags"] {
            if let Some(value) = build.get(key) {
                flag_lists.push((format!("build.{key}"), value));
            }
        }
    }
    if let Some(targets) = table.get("target").and_then(toml::Value::as_table) {
        for (name, target) in targets {
            if let Some(value) = target.get("rustflags") {
                flag_lists.push((format!("target.{name}.rustflags"), value));
            }
        }
    }
    for (key, value) in flag_lists {
        if lowers_lints(value) {
            findings.push(finding(format!("{key} lowers a lint level")));
        }
    }
    findings
}

fn lowers_lints(value: &toml::Value) -> bool {
    let flags: Vec<String> = match value {
        toml::Value::String(text) => text.split_whitespace().map(str::to_owned).collect(),
        toml::Value::Array(items) => items
            .iter()
            .filter_map(toml::Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    };
    flags.iter().any(|flag| {
        flag.starts_with("-A") || flag.starts_with("--allow") || flag.starts_with("--cap-lints")
    })
}

/// Lint and format configuration that would override the oracle's own.
pub fn is_shadow_config(rel: &str, phase: u8) -> bool {
    let file = rel.rsplit('/').next().unwrap_or_default();
    let config = matches!(
        file,
        "clippy.toml"
            | ".clippy.toml"
            | "rustfmt.toml"
            | ".rustfmt.toml"
            | ".eslintignore"
            | ".stylelintignore"
    ) || file.starts_with(".eslintrc")
        || file.starts_with("eslint.config.")
        || file.starts_with(".stylelintrc")
        || file.starts_with("stylelint.config.");
    config && !is_oracle_path(rel, phase)
}

fn skip_dir(rel: &str) -> bool {
    is_own_output_dir(rel) || is_oracle_path(rel, 0)
}

const WEB_EXTENSIONS: [&str; 13] = [
    "js", "mjs", "cjs", "ts", "mts", "cts", "tsx", "jsx", "css", "scss", "less", "html", "vue",
];

pub fn check(root: &Path, phase: u8) -> CheckResult {
    let mut findings = Vec::new();
    let mut panic_exceptions = Vec::new();
    for rel in walk(root, root, &skip_dir) {
        if is_oracle_path(&rel, phase) {
            continue;
        }
        if is_shadow_config(&rel, phase) {
            findings.push(Finding {
                path: rel.clone(),
                line: 0,
                what: "lint or format configuration outside the oracle files".to_owned(),
            });
            continue;
        }
        let file = rel.rsplit('/').next().unwrap_or_default();
        let extension = file
            .rsplit_once('.')
            .map(|(_, ext)| ext)
            .unwrap_or_default();
        let is_cargo_config = rel.ends_with(".cargo/config.toml") || rel.ends_with(".cargo/config");
        let is_member_manifest = file == "Cargo.toml" && rel != "Cargo.toml";
        if extension != "rs"
            && !WEB_EXTENSIONS.contains(&extension)
            && !is_cargo_config
            && !is_member_manifest
        {
            continue;
        }
        let Ok(text) = fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        if extension == "rs" {
            let scan = scan_rust(&rel, &text);
            findings.extend(scan.findings);
            panic_exceptions.extend(
                scan.panic_exceptions
                    .into_iter()
                    .map(|line| (rel.clone(), line)),
            );
        } else if is_cargo_config {
            findings.extend(scan_cargo_config(&rel, &text));
        } else if is_member_manifest {
            findings.extend(scan_manifest(&rel, &text));
        } else {
            findings.extend(scan_web(&rel, &text));
        }
    }
    if panic_exceptions.len() > 1 {
        findings.extend(panic_exceptions.iter().map(|(path, line)| Finding {
            path: path.clone(),
            line: *line,
            what: "more than one #[expect(clippy::panic)] for WTPOOL_VERIFY_PANIC".to_owned(),
        }));
    }
    if findings.is_empty() {
        CheckResult::pass(NAME, "static", "no suppression")
    } else {
        let listed: Vec<String> = findings
            .iter()
            .map(|f| format!("{}:{}: {}", f.path, f.line, f.what))
            .collect();
        CheckResult::assertion(NAME, "static", listed.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::outer_allow("#[allow(dead_code)]\nfn f() {}", true)]
    #[case::inner_allow("#![allow(clippy::unwrap_used)]", true)]
    #[case::expect("#[expect(clippy::indexing_slicing)]", true)]
    #[case::spaced(" # [ allow ( unused ) ]", true)]
    #[case::multi_line_cfg_attr("#[cfg_attr(\n    test,\n    allow(clippy::panic)\n)]", true)]
    #[case::rustfmt_skip("#[rustfmt::skip]", true)]
    #[case::old_rustfmt_skip("#[cfg_attr(rustfmt, rustfmt_skip)]", true)]
    #[case::derive("#[derive(Debug)]", false)]
    #[case::cfg_attr_derive("#[cfg_attr(test, derive(Debug))]", false)]
    #[case::deny("#[deny(warnings)]", false)]
    #[case::method_call("let x = y.expect(\"setup\");", false)]
    #[case::string_with_bracket("#[doc = \"a ] b\"]\n#[allow(x)]", true)]
    #[case::open_bracket_in_a_comment(
        "// opens with #[ here\n#[allow(dead_code)]\nfn f() {}",
        true
    )]
    #[case::open_bracket_in_a_string(
        "const OPEN: &str = \"#[\";\n#[allow(dead_code)]\nfn f() {}",
        true
    )]
    #[case::allow_inside_another_attribute("#[doc = \"#[allow(x)]\"]\nfn f() {}", true)]
    #[case::allow_text_in_a_comment("// write #[allow(x)] nowhere\nfn f() {}", true)]
    fn scan_rust__should_flag_lint_suppressions(#[case] source: &str, #[case] flagged: bool) {
        // Given
        let text = source;

        // When
        let scan = scan_rust("a.rs", text);

        // Then
        assert_eq!(!scan.findings.is_empty(), flagged);
    }

    #[rstest]
    #[case::guarding_the_hook(
        "#[expect(clippy::panic)]\nfn maybe_panic() {\n    if var(\"WTPOOL_VERIFY_PANIC\") {}\n}",
        1,
        0
    )]
    #[case::with_reason(
        "#[expect(clippy::panic, reason = \"verify\")]\nfn f() { var(\"WTPOOL_VERIFY_PANIC\"); }",
        1,
        0
    )]
    #[case::elsewhere("#[expect(clippy::panic)]\nfn f() { panic!() }", 0, 1)]
    #[case::other_lint_near_the_hook(
        "#[expect(clippy::unwrap_used)]\nfn f() { var(\"WTPOOL_VERIFY_PANIC\"); }",
        0,
        1
    )]
    #[case::allow_instead_of_expect(
        "#[allow(clippy::panic)]\nfn f() { var(\"WTPOOL_VERIFY_PANIC\"); }",
        0,
        1
    )]
    fn scan_rust__should_except_only_the_panic_hook(
        #[case] source: &str,
        #[case] exceptions: usize,
        #[case] findings: usize,
    ) {
        // Given
        let text = source;

        // When
        let scan = scan_rust("a.rs", text);

        // Then
        assert_eq!(
            (scan.panic_exceptions.len(), scan.findings.len()),
            (exceptions, findings)
        );
    }

    #[rstest]
    #[case::after_blank_lines("fn a() {}\n\n#[allow(unused)]\nfn b() {}", 3)]
    #[case::after_an_open_bracket_in_a_comment("// #[ open\n\n#[allow(unused)]\nfn b() {}", 3)]
    #[case::after_a_multi_line_attribute(
        "#[cfg_attr(\n    test,\n    derive(Debug)\n)]\n#[allow(unused)]\nfn b() {}",
        5
    )]
    fn scan_rust__should_report_the_attribute_line(#[case] source: &str, #[case] line: usize) {
        // Given
        let text = source;

        // When
        let scan = scan_rust("a.rs", text);

        // Then
        assert_eq!(scan.findings.first().map(|f| f.line), Some(line));
    }

    #[rstest]
    #[case::eslint_line("foo(); // eslint-disable-line no-console", true)]
    #[case::eslint_block("/* eslint-disable */", true)]
    #[case::eslint_inline_config("/* eslint no-console: off */", true)]
    #[case::stylelint("/* stylelint-disable-next-line */", true)]
    #[case::plain("const a = 1;", false)]
    fn scan_web__should_flag_disable_comments(#[case] source: &str, #[case] flagged: bool) {
        // Given
        let text = source;

        // When
        let findings = scan_web("a.ts", text);

        // Then
        assert_eq!(!findings.is_empty(), flagged);
    }

    #[rstest]
    #[case::inherits("[package]\nname = \"a\"\n[lints]\nworkspace = true\n", false)]
    #[case::no_lints("[package]\nname = \"a\"\n", true)]
    #[case::own_lints("[package]\nname = \"a\"\n[lints.clippy]\npanic = \"allow\"\n", true)]
    #[case::inherits_and_adds(
        "[package]\nname = \"a\"\n[lints]\nworkspace = true\n[lints.rust]\nx = \"allow\"\n",
        true
    )]
    #[case::not_a_package("[workspace]\nmembers = []\n", false)]
    fn scan_manifest__should_require_workspace_lints_only(
        #[case] source: &str,
        #[case] flagged: bool,
    ) {
        // Given
        let text = source;

        // When
        let findings = scan_manifest("crates/a/Cargo.toml", text);

        // Then
        assert_eq!(!findings.is_empty(), flagged);
    }

    #[rstest]
    #[case::plain_env("[env]\nGIT_CONFIG_GLOBAL = \"/dev/null\"\n", false)]
    #[case::rustflags_env("[env]\nRUSTFLAGS = \"-Awarnings\"\n", true)]
    #[case::clippy_conf_env("[env]\nCLIPPY_CONF_DIR = \"x\"\n", true)]
    #[case::build_allow("[build]\nrustflags = [\"-A\", \"warnings\"]\n", true)]
    #[case::build_cap("[build]\nrustflags = \"--cap-lints warn\"\n", true)]
    #[case::build_other("[build]\nrustflags = [\"-C\", \"target-cpu=native\"]\n", false)]
    #[case::target_allow(
        "[target.aarch64-apple-darwin]\nrustflags = [\"--allow=unused\"]\n",
        true
    )]
    #[case::alias_only("[alias]\nxtask = \"run\"\n", false)]
    fn scan_cargo_config__should_flag_lowered_lints(#[case] source: &str, #[case] flagged: bool) {
        // Given
        let text = source;

        // When
        let findings = scan_cargo_config(".cargo/config.toml", text);

        // Then
        assert_eq!(!findings.is_empty(), flagged);
    }

    #[rstest]
    #[case::nested_target("crates/wtpool/src/target/mod.rs", true)]
    #[case::nested_git_folder("crates/wtpool/.git/x.rs", true)]
    #[case::nested_dependencies("crates/wtpool/node_modules/x.js", true)]
    #[case::build_output("target/debug/build/x/out/x.rs", false)]
    #[case::xtask_output("xtask/target/debug/x.rs", false)]
    #[case::run_folder("verify/out/run/sandbox/x.rs", false)]
    fn check__should_scan_every_folder_but_the_repository_own_output(
        #[case] rel: &str,
        #[case] flagged: bool,
    ) {
        // Given
        let root = std::env::temp_dir().join(format!(
            "xtask-suppress-{}-{}",
            std::process::id(),
            rel.replace('/', "_")
        ));
        let _ = fs::remove_dir_all(&root);
        let file = root.join(rel);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        let source = if rel.ends_with(".js") {
            "// eslint-disable-next-line\n"
        } else {
            "#[allow(clippy::indexing_slicing)]\nfn f() {}\n"
        };
        fs::write(&file, source).unwrap();

        // When
        let result = check(&root, 0);

        // Then
        let _ = fs::remove_dir_all(&root);
        assert_eq!(
            result.outcome == crate::results::Outcome::Fail,
            flagged,
            "{}",
            result.message
        );
    }

    #[rstest]
    #[case::root_clippy("clippy.toml", false)]
    #[case::crate_clippy("crates/wtpool/clippy.toml", true)]
    #[case::hidden_clippy("crates/wtpool/.clippy.toml", true)]
    #[case::crate_rustfmt("crates/wtpool/rustfmt.toml", true)]
    #[case::oracle_eslint("vscode/eslint.config.mjs", false)]
    #[case::other_eslint("vscode/src/eslint.config.js", true)]
    #[case::eslintrc("vscode/.eslintrc.json", true)]
    #[case::ignore_file("vscode/.stylelintignore", true)]
    #[case::source("crates/wtpool/src/main.rs", false)]
    fn is_shadow_config__should_flag_config_outside_the_oracle(
        #[case] rel: &str,
        #[case] flagged: bool,
    ) {
        // Given
        let path = rel;

        // When
        let shadow = is_shadow_config(path, 1);

        // Then
        assert_eq!(shadow, flagged);
    }
}
