use std::collections::BTreeMap;
use std::path::Path;

use crate::cmd::git;
use crate::repo::{base, walk};
use crate::results::CheckResult;

const NAME: &str = "repo_context__should_be_unchanged";

/// Files Claude Code reads as instructions. macOS file systems ignore case, so names compare
/// without it.
pub fn is_context_path(rel: &str) -> bool {
    let lower = rel.to_lowercase();
    let mut parts = lower.split('/');
    let file = lower.rsplit('/').next().unwrap_or_default();
    matches!(file, "claude.md" | "claude.local.md" | "agents.md")
        || parts.any(|part| part == ".claude")
}

/// Folders of this repository that only build tools, git and the harness write. A folder of the
/// same name elsewhere in the tree is product source and gets walked.
pub const OWN_OUTPUT_DIRS: [&str; 7] = [
    ".git",
    "target",
    "xtask/target",
    "node_modules",
    "vscode/node_modules",
    "vscode/test/node_modules",
    "verify/out",
];

pub fn is_own_output_dir(rel: &str) -> bool {
    OWN_OUTPUT_DIRS.contains(&rel)
}

pub fn skip_dir(rel: &str) -> bool {
    is_own_output_dir(rel)
}

/// Paths whose blob differs between the two maps of path to blob id.
pub fn compare(base: &BTreeMap<String, String>, now: &BTreeMap<String, String>) -> Vec<String> {
    let mut differing: Vec<String> = base
        .iter()
        .filter(|(path, blob)| now.get(*path) != Some(*blob))
        .map(|(path, _)| path.clone())
        .collect();
    differing.extend(now.keys().filter(|path| !base.contains_key(*path)).cloned());
    differing.sort();
    differing.dedup();
    differing
}

pub fn check(root: &Path) -> CheckResult {
    let base = match base(root) {
        Ok(base) => base,
        Err(error) => return CheckResult::error(NAME, "harness", error),
    };
    let short = base.commit.get(..12).unwrap_or(&base.commit).to_owned();
    let listing = match git(root, &["ls-tree", "-r", "-z", &base.commit]) {
        Ok(listing) => listing,
        Err(error) => return CheckResult::error(NAME, "harness", error),
    };
    let at_base: BTreeMap<String, String> = listing
        .split('\0')
        .filter_map(|line| {
            let (meta, path) = line.split_once('\t')?;
            let blob = meta.split_whitespace().nth(2)?;
            is_context_path(path).then(|| (path.to_owned(), blob.to_owned()))
        })
        .collect();
    let mut now = BTreeMap::new();
    for rel in walk(root, root, &skip_dir) {
        if !is_context_path(&rel) {
            continue;
        }
        let blob = git(root, &["hash-object", "--no-filters", "--", &rel])
            .map(|sha| sha.trim().to_owned())
            .unwrap_or_else(|error| format!("unreadable: {error}"));
        now.insert(rel, blob);
    }
    let differing = compare(&at_base, &now);
    if differing.is_empty() {
        CheckResult::pass(
            NAME,
            "harness",
            format!("{} files equal base {short} ({})", now.len(), base.source),
        )
    } else {
        CheckResult::assertion(
            NAME,
            "harness",
            format!(
                "differs from base {short} ({}): {}",
                base.source,
                differing.join(", ")
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::root_claude("CLAUDE.md", true)]
    #[case::nested_claude("crates/wtpool/CLAUDE.md", true)]
    #[case::other_case("crates/Claude.md", true)]
    #[case::local_claude("CLAUDE.local.md", true)]
    #[case::agents("AGENTS.md", true)]
    #[case::settings("crates/.claude/settings.json", true)]
    #[case::skill(".claude/skills/x/SKILL.md", true)]
    #[case::readme("README.md", false)]
    #[case::lookalike("docs/claude.md.txt", false)]
    fn is_context_path__should_match_instruction_files(#[case] rel: &str, #[case] expected: bool) {
        // Given
        let path = rel;

        // When
        let context = is_context_path(path);

        // Then
        assert_eq!(context, expected);
    }

    #[rstest]
    #[case::root_target("target", true)]
    #[case::xtask_target("xtask/target", true)]
    #[case::root_git(".git", true)]
    #[case::run_folders("verify/out", true)]
    #[case::extension_dependencies("vscode/node_modules", true)]
    #[case::test_dependencies("vscode/test/node_modules", true)]
    #[case::nested_target("crates/wtpool/src/target", false)]
    #[case::nested_git("crates/wtpool/.git", false)]
    #[case::nested_dependencies("crates/wtpool/node_modules", false)]
    #[case::source("crates/wtpool/src", false)]
    fn skip_dir__should_skip_only_the_repository_own_output(
        #[case] rel: &str,
        #[case] skipped: bool,
    ) {
        // Given
        let path = rel;

        // When
        let skip = skip_dir(path);

        // Then
        assert_eq!(skip, skipped);
    }

    #[rstest]
    #[case::equal(&[("CLAUDE.md", "1")], &[("CLAUDE.md", "1")], &[])]
    #[case::added(&[], &[("CLAUDE.md", "1")], &["CLAUDE.md"])]
    #[case::removed(&[("AGENTS.md", "1")], &[], &["AGENTS.md"])]
    #[case::edited(&[("AGENTS.md", "1")], &[("AGENTS.md", "2")], &["AGENTS.md"])]
    fn compare__should_list_every_difference(
        #[case] base: &[(&str, &str)],
        #[case] now: &[(&str, &str)],
        #[case] expected: &[&str],
    ) {
        // Given
        let to_map = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs
                .iter()
                .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
                .collect()
        };

        // When
        let differing = compare(&to_map(base), &to_map(now));

        // Then
        assert_eq!(
            differing,
            expected.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()
        );
    }
}
