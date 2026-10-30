//! The static checks phase 5 adds: the extension's ESLint, Prettier and `tsc` on the OpenCode
//! plugin of `crates/wtpool-opencode/plugin/`, and the notice of each agent mark the extension
//! ships.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::builds::vscode;
use crate::parts::Ctx;
use crate::results::{CheckResult, FailKind, Outcome};

/// The first phase whose static checks cover the plugin and the marks.
pub(super) const PHASE: u8 = 5;
pub(super) const PLUGIN_DIR: &str = "crates/wtpool-opencode/plugin";
pub(super) const PLUGIN_TYPES: &str = "static:plugin_types";
pub(super) const MARKS: &str = "marks__should_name_their_licence";
const NOTICES: [&str; 2] = ["THIRD-PARTY-NOTICES.md", "vscode/THIRD-PARTY-NOTICES.md"];
const LIMIT: Duration = Duration::from_secs(60);
/// The extension's sources that may hold marks as data: the webview's and the shared panel core's.
const MARK_SOURCES: [&str; 2] = ["vscode/src/webview", "vscode/src/shared"];

/// Each mark id of the panel's contract with the names a notice may give its maker.
const MAKERS: [(&str, &[&str]); 12] = [
    ("anthropic", &["anthropic", "claude"]),
    ("openai", &["openai"]),
    ("google", &["google", "gemini"]),
    ("deepseek", &["deepseek"]),
    ("zhipu", &["zhipu", "z.ai", "z-ai", "glm"]),
    ("qwen", &["qwen", "alibaba"]),
    ("moonshot", &["moonshot", "kimi"]),
    ("mistral", &["mistral"]),
    ("meta", &["meta", "llama"]),
    ("xai", &["xai", "x.ai", "grok"]),
    ("minimax", &["minimax"]),
    ("opencode", &["opencode"]),
];

/// A tool of vscode/node_modules, run from the repository root.
fn tool(ctx: &Ctx, name: &str) -> Command {
    let program = vscode(ctx).join("node_modules/.bin").join(name);
    ctx.command(&program.to_string_lossy())
}

/// The result of `command` under `check`, its log named `log`.
fn run(ctx: &Ctx, check: &str, log: &str, command: &mut Command) -> CheckResult {
    match ctx.run_logged(log, command, LIMIT) {
        Err(error) => CheckResult::error(check, "static", error),
        Ok(ran) if ran.ok() => CheckResult::pass(check, "static", "").with_seconds(ran.seconds),
        Ok(ran) if ran.timed_out() => CheckResult::fail(
            check,
            "static",
            FailKind::Timeout,
            format!("over {} s", LIMIT.as_secs()),
        ),
        Ok(ran) => {
            CheckResult::assertion(check, "static", format!("{PLUGIN_DIR}: {}", ran.tail(40)))
                .with_seconds(ran.seconds)
        }
    }
}

/// An assertion when the plugin's folder does not exist.
fn missing_plugin(ctx: &Ctx, check: &str) -> Option<CheckResult> {
    (!ctx.root.join(PLUGIN_DIR).is_dir())
        .then(|| CheckResult::assertion(check, "static", format!("{PLUGIN_DIR}/ does not exist")))
}

/// `earlier`, the check over the extension, then over the plugin when it passed and the phase
/// covers the plugin.
pub(super) fn and_plugin(ctx: &Ctx, earlier: CheckResult) -> CheckResult {
    if ctx.phase < PHASE || earlier.outcome != Outcome::Pass {
        return earlier;
    }
    let check = earlier.check.clone();
    if let Some(missing) = missing_plugin(ctx, &check) {
        return missing;
    }
    let log = format!("{}_plugin", check.replace(':', "_"));
    let mut command = match check.as_str() {
        "static:eslint" => {
            let mut command = tool(ctx, "eslint");
            command
                .args(["--config", "vscode/eslint.config.mjs"])
                .arg(PLUGIN_DIR);
            command
        }
        "static:prettier" => {
            let mut command = tool(ctx, "prettier");
            command
                .args(["--config", "vscode/.prettierrc", "--check"])
                .arg(PLUGIN_DIR);
            command
        }
        _ => return earlier,
    };
    let mut result = run(ctx, &check, &log, &mut command);
    result.seconds += earlier.seconds;
    result
}

/// `static:plugin_types`: the extension's `tsc` on the plugin, strict, erasable syntax only and
/// with no type package.
pub(super) fn plugin_types(ctx: &Ctx) -> CheckResult {
    if let Some(missing) = missing_plugin(ctx, PLUGIN_TYPES) {
        return missing;
    }
    let dir = ctx.run_dir.join("static");
    let config = dir.join("plugin-tsconfig.json");
    let files: Vec<String> = crate::repo::walk(&ctx.root, &ctx.root.join(PLUGIN_DIR), &|_| false)
        .into_iter()
        .filter(|rel| rel.ends_with(".ts"))
        .map(|rel| ctx.root.join(rel).to_string_lossy().into_owned())
        .collect();
    let text = serde_json::json!({
        "compilerOptions": {
            "strict": true,
            "noEmit": true,
            "erasableSyntaxOnly": true,
            "types": [],
            "typeRoots": [],
        },
        "files": files,
    });
    if let Err(error) = fs::create_dir_all(&dir).and_then(|()| fs::write(&config, text.to_string()))
    {
        return CheckResult::error(
            PLUGIN_TYPES,
            "static",
            format!("setup: {}: {error}", config.display()),
        );
    }
    run(
        ctx,
        PLUGIN_TYPES,
        "static_plugin_types",
        tool(ctx, "tsc").arg("-p").arg(&config),
    )
}

/// One mark the extension ships: its id and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Mark {
    pub(super) id: String,
    pub(super) at: String,
}

/// True when `text` is SVG path data: a moveto with its point, then only path commands and
/// numbers. The first piece of a path split across lines may hold the moveto alone.
fn path_data(text: &str) -> bool {
    let mut chars = text.trim_start().chars();
    let starts = matches!(chars.next(), Some('M' | 'm'));
    let numbers = text
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|part| part.chars().any(|c| c.is_ascii_digit()))
        .count();
    starts
        && text.len() >= 8
        && numbers >= 2
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || " ,.-+eEMmZzLlHhVvCcSsQqTtAa\n\t".contains(c))
}

/// True when a line of source holds SVG path data: a quoted literal of only path data, or a `d`
/// attribute inside markup.
pub(super) fn has_path(line: &str) -> bool {
    let quoted = ['\'', '"', '`'].iter().any(|quote| {
        line.split(*quote)
            .enumerate()
            .any(|(index, part)| index % 2 == 1 && path_data(part))
    });
    quoted
        || line.match_indices("d=").any(|(at, _)| {
            let after = line.get(at.saturating_add(2)..).unwrap_or_default();
            let value = after
                .trim_start_matches('\\')
                .trim_start_matches(['"', '\'']);
            let end = value.find(['"', '\'', '\\']).unwrap_or(value.len());
            path_data(value.get(..end).unwrap_or_default())
        })
}

/// The mark id a line names as a word, the last one when it names several.
fn id_in(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    let mut best: Option<(usize, &'static str)> = None;
    for (id, _) in MAKERS {
        let mut from = 0;
        while let Some(at) = lower.get(from..).and_then(|rest| rest.find(id)) {
            let start = from.saturating_add(at);
            let end = start.saturating_add(id.len());
            let word = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
            let before = lower.get(..start).and_then(|text| text.chars().last());
            let after = lower.get(end..).and_then(|text| text.chars().next());
            if !word(before) && !word(after) && best.is_none_or(|(at, _)| start >= at) {
                best = Some((start, id));
            }
            from = end;
        }
    }
    best.map(|(_, id)| id)
}

/// The marks of one source file: each path's id is the one named on its line or the nearest line
/// before it, as in a table of marks. Returns the marks and the paths no id names.
pub(super) fn marks_in_source(name: &str, text: &str) -> (Vec<Mark>, Vec<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut marks = Vec::new();
    let mut unnamed = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !has_path(line) {
            continue;
        }
        let at = format!("{name}:{}", index.saturating_add(1));
        let id = (0..=index)
            .rev()
            .find_map(|i| lines.get(i).and_then(|l| id_in(l)));
        match id {
            Some(id) => marks.push(Mark {
                id: id.to_owned(),
                at,
            }),
            None => unnamed.push(at),
        }
    }
    (marks, unnamed)
}

/// Every mark the extension ships: each SVG of `vscode/media/marks/`, named by its file, and each
/// path in the [`MARK_SOURCES`].
pub(super) fn shipped_marks(root: &Path) -> (Vec<Mark>, Vec<String>) {
    let mut marks = Vec::new();
    let mut unnamed = Vec::new();
    let skip = |rel: &str| rel.ends_with("node_modules");
    for rel in crate::repo::walk(root, &root.join("vscode/media/marks"), &skip) {
        if let Some(stem) = rel.strip_suffix(".svg").and_then(|r| r.rsplit('/').next()) {
            marks.push(Mark {
                id: stem.to_ascii_lowercase(),
                at: rel.clone(),
            });
        }
    }
    let sources = MARK_SOURCES
        .iter()
        .flat_map(|dir| crate::repo::walk(root, &root.join(dir), &skip));
    for rel in sources {
        if let Some(stem) = rel.strip_suffix(".svg").and_then(|r| r.rsplit('/').next()) {
            marks.push(Mark {
                id: stem.to_ascii_lowercase(),
                at: rel.clone(),
            });
            continue;
        }
        if ![".ts", ".js", ".mjs", ".json", ".css"]
            .iter()
            .any(|ext| rel.ends_with(ext))
        {
            continue;
        }
        let Ok(text) = fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        let (found, nameless) = marks_in_source(&rel, &text);
        marks.extend(found);
        unnamed.extend(nameless);
    }
    marks.sort_by(|a, b| (&a.id, &a.at).cmp(&(&b.id, &b.at)));
    marks.dedup_by(|a, b| a.id == b.id);
    (marks, unnamed)
}

/// The names a notice may give the maker of mark `id`.
fn names_of(id: &str) -> Vec<String> {
    MAKERS.iter().find(|(known, _)| *known == id).map_or_else(
        || vec![id.to_owned()],
        |(_, names)| names.iter().map(|n| (*n).to_owned()).collect(),
    )
}

fn names_word(text: &str, name: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.match_indices(name).any(|(at, _)| {
        let word = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric());
        !word(lower.get(..at).and_then(|t| t.chars().last()))
            && !word(
                lower
                    .get(at.saturating_add(name.len())..)
                    .and_then(|t| t.chars().next()),
            )
    })
}

fn has_source_and_licence(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    (names_word(&lower, "source") || lower.contains("http://") || lower.contains("https://"))
        && (lower.contains("licence") || lower.contains("license"))
}

/// True when `notices` gives mark `id` its source and licence: a section about marks names its
/// maker, in its heading or in list items that name a source and a licence.
pub(super) fn notice_names(notices: &str, id: &str) -> bool {
    let names = names_of(id);
    let mut sections: Vec<Vec<&str>> = vec![Vec::new()];
    for line in notices.lines() {
        if line.starts_with("## ") || line.starts_with("### ") || line.starts_with("#### ") {
            sections.push(Vec::new());
        }
        if let Some(section) = sections.last_mut() {
            section.push(line);
        }
    }
    sections.iter().any(|section| {
        let text = section.join("\n");
        if !names_word(&text, "mark") && !names_word(&text, "marks") {
            return false;
        }
        let heading = section.first().copied().unwrap_or_default();
        if names.iter().any(|name| names_word(heading, name)) {
            return has_source_and_licence(&text);
        }
        let mut items: Vec<String> = Vec::new();
        for line in section {
            if line.trim_start().starts_with("- ") || items.is_empty() {
                items.push((*line).to_owned());
            } else if let Some(item) = items.last_mut() {
                item.push('\n');
                item.push_str(line);
            }
        }
        items.iter().any(|item| {
            names.iter().any(|name| names_word(item, name)) && has_source_and_licence(item)
        })
    })
}

/// `marks__should_name_their_licence`: each mark the extension ships has its source and licence
/// in the repository's notices and in the extension's.
pub(super) fn marks(ctx: &Ctx) -> CheckResult {
    let (marks, unnamed) = shipped_marks(&ctx.root);
    let mut problems: Vec<String> = unnamed
        .iter()
        .map(|at| format!("{at}: an SVG path that names no maker of the mark table"))
        .collect();
    if marks.is_empty() {
        problems.push(
            "the extension ships no mark in vscode/src/webview/, vscode/src/shared/ or vscode/media/marks/"
                .to_owned(),
        );
    }
    for file in NOTICES {
        let Ok(text) = fs::read_to_string(ctx.root.join(file)) else {
            problems.push(format!("{file} cannot be read"));
            continue;
        };
        for mark in &marks {
            if !notice_names(&text, &mark.id) {
                problems.push(format!(
                    "{file} names no source and licence for the {} mark ({})",
                    mark.id, mark.at
                ));
            }
        }
    }
    if problems.is_empty() {
        let ids: Vec<&str> = marks.iter().map(|m| m.id.as_str()).collect();
        CheckResult::pass(MARKS, "static", format!("marks: {}", ids.join(", ")))
    } else {
        CheckResult::assertion(MARKS, "static", problems.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::literal("  anthropic: 'M4.7 1h2.6l4.7 14h-2.6l-1-3H5.3l-1 3H1.7z',", true)]
    #[case::attribute(r#"' d="M8 .5a1 1 0 1 1 0 2 1 1 0 0 1 0-2Z"/>',"#, true)]
    #[case::words("const label = 'Move the mark';", false)]
    #[case::short("x = 'M1 2';", false)]
    #[case::moveto_alone("      'M23.748 4.651',", true)]
    fn has_path__should_find_svg_path_data(#[case] line: &str, #[case] expected: bool) {
        // When
        let found = has_path(line);

        // Then
        assert_eq!(found, expected);
    }

    #[test]
    fn marks_in_source__should_name_each_path_by_the_id_before_it() {
        // Given
        let text = "export const MARKS = {\n  deepseek: {\n    path: 'M2 3h10v10H2zM4 5h6v6H4z',\n  },\n  zhipu: 'M1 1l14 14M15 1L1 15z',\n  robot: 'M3 3h10v10H3zM5 5h6v6H5z',\n};\n";

        // When
        let (marks, unnamed) = marks_in_source("vscode/src/webview/marks.ts", text);

        // Then
        let ids: Vec<&str> = marks.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["deepseek", "zhipu", "zhipu"]);
        assert!(unnamed.is_empty(), "{unnamed:?}");
    }

    #[rstest]
    #[case::bullet(
        "## Agent marks\n\n- DeepSeek: Source: https://example.org/deepseek.svg. Licence: MIT.\n",
        "deepseek",
        true
    )]
    #[case::heading(
        "### Zhipu mark\n\n- File: `src/webview/marks.ts`.\n- Source: drawn for wtpool.\n- Licence: MIT.\n",
        "zhipu",
        true
    )]
    #[case::no_licence(
        "## Agent marks\n\n- DeepSeek: Source: https://example.org/deepseek.svg.\n",
        "deepseek",
        false
    )]
    #[case::not_about_marks(
        "### Claude Code hook recordings\n\n- Source: the payloads of Claude Code, by Anthropic.\n- Licence: none.\n",
        "anthropic",
        false
    )]
    #[case::other_maker(
        "## Agent marks\n\n- OpenAI: Source: https://example.org/o.svg. Licence: MIT.\n",
        "deepseek",
        false
    )]
    fn notice_names__should_need_a_marks_section_with_source_and_licence(
        #[case] notices: &str,
        #[case] id: &str,
        #[case] expected: bool,
    ) {
        // When
        let found = notice_names(notices, id);

        // Then
        assert_eq!(found, expected);
    }
}
