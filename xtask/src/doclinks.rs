//! `static:doc_links`: every relative link and anchor of the repository's Markdown resolves.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::cmd::git;
use crate::results::CheckResult;

const NAME: &str = "static:doc_links";

/// Link targets of Markdown text outside code fences and code spans.
pub fn links(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut in_fence = false;
    for (index, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let without_code: String = line
            .split('`')
            .enumerate()
            .filter(|(i, _)| i % 2 == 0)
            .map(|(_, part)| part)
            .collect::<Vec<_>>()
            .join("");
        let mut rest = without_code.as_str();
        while let Some(at) = rest.find("](") {
            let after = rest.get(at.saturating_add(2)..).unwrap_or_default();
            let Some(end) = after.find(')') else {
                break;
            };
            let target = after.get(..end).unwrap_or_default();
            let target = target.split_whitespace().next().unwrap_or_default();
            found.push((index.saturating_add(1), target.to_owned()));
            rest = after.get(end..).unwrap_or_default();
        }
    }
    found
}

/// The ids of the `<a id="…">` and `<a name="…">` tags of a line, anchors GitHub keeps.
fn html_anchors(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    for attribute in ["<a id=\"", "<a name=\""] {
        let mut rest = line;
        while let Some(at) = rest.find(attribute) {
            rest = rest
                .get(at.saturating_add(attribute.len())..)
                .unwrap_or_default();
            if let Some((id, after)) = rest.split_once('"') {
                found.push(id.to_owned());
                rest = after;
            }
        }
    }
    found
}

/// GitHub's anchors for the headings and HTML anchors of Markdown text.
pub fn anchors(text: &str) -> BTreeSet<String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut found = BTreeSet::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        found.extend(html_anchors(line));
        if !line.starts_with('#') {
            continue;
        }
        let title = line.trim_start_matches('#').trim();
        let slug: String = title
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
            .map(|c| if c == ' ' { '-' } else { c })
            .collect();
        let count = seen.entry(slug.clone()).or_insert(0);
        let anchor = if *count == 0 {
            slug.clone()
        } else {
            format!("{slug}-{count}")
        };
        *count = count.saturating_add(1);
        found.insert(anchor);
    }
    found
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

pub fn check(root: &Path) -> CheckResult {
    let listing = match git(
        root,
        &["ls-files", "-co", "--exclude-standard", "-z", "--", "*.md"],
    ) {
        Ok(listing) => listing,
        Err(error) => return CheckResult::error(NAME, "static", error),
    };
    let mut broken = Vec::new();
    for rel in listing.split('\0').filter(|rel| !rel.is_empty()) {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        let dir = Path::new(rel).parent().unwrap_or(Path::new(""));
        for (line, target) in links(&text) {
            if target.contains("://") || target.starts_with("mailto:") || target.is_empty() {
                continue;
            }
            let (file, anchor) = target.split_once('#').unwrap_or((target.as_str(), ""));
            let target_rel = if file.is_empty() {
                PathBuf::from(rel)
            } else {
                normalize(&dir.join(file))
            };
            let target_path = root.join(&target_rel);
            if !target_path.exists() {
                broken.push(format!("{rel}:{line}: {target} does not exist"));
                continue;
            }
            let is_markdown = target_rel.extension().and_then(|e| e.to_str()) == Some("md");
            if anchor.is_empty() || !is_markdown {
                continue;
            }
            let headings = fs::read_to_string(&target_path)
                .map(|text| anchors(&text))
                .unwrap_or_default();
            if !headings.contains(anchor) {
                broken.push(format!("{rel}:{line}: {target} names no heading"));
            }
        }
    }
    if broken.is_empty() {
        CheckResult::pass(NAME, "static", "every relative Markdown link resolves")
    } else {
        CheckResult::assertion(NAME, "static", broken.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links__should_skip_code() {
        // Given
        let text = "See [a](a.md) and `[b](b.md)`.\n```\n[c](c.md)\n```\n[d](d.md#x \"t\")";

        // When
        let found = links(text);

        // Then
        assert_eq!(
            found,
            vec![(1, "a.md".to_owned()), (5, "d.md#x".to_owned())]
        );
    }

    #[test]
    fn anchors__should_take_html_anchors() {
        // Given
        let text = "<a id=\"small-crates\"></a>\n### Prefer small crates\nSee <a name=\"x\">x</a>.\n```\n<a id=\"code\"></a>\n```";

        // When
        let found = anchors(text);

        // Then
        let expected: BTreeSet<String> = ["small-crates", "prefer-small-crates", "x"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn anchors__should_follow_github_slugs() {
        // Given
        let text =
            "# Worktree status panel\n## 1. `wtpool hook`\n## Done\n## Done\n```\n# not\n```";

        // When
        let found = anchors(text);

        // Then
        let expected: BTreeSet<String> =
            ["worktree-status-panel", "1-wtpool-hook", "done", "done-1"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect();
        assert_eq!(found, expected);
    }
}
