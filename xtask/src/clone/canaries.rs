//! Canaries: patches in `verify/canaries/` that break the product or the harness on purpose, and
//! the runner that shows their check catches each.

use std::fs;
use std::path::{Path, PathBuf};

use crate::cmd::git;
use crate::results::{CheckResult, FailKind, Outcome};

use super::{ScratchClone, first_line};

const NEEDS_YOU_CODES: [&str; 6] = [
    "logged_out",
    "no_network",
    "machine_busy",
    "inconclusive",
    "assumption_broken",
    "oracle_change",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canary {
    pub path: PathBuf,
    pub id: String,
    pub category: String,
    pub check: String,
    pub tree: Tree,
    pub expect: Expect,
}

/// The state of the patched tree a canary's check sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tree {
    /// The patch is committed, so the check sees a clean tree, as in a fresh clone.
    Committed,
    /// The patch stays an uncommitted change, for canaries that break the dirty tree half of a
    /// check. `Tree: uncommitted` above the diff asks for it.
    Uncommitted,
}

/// The outcome a canary's check ends with when it catches the canary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expect {
    /// A failure on an assertion.
    Assertion,
    /// A needs you code, for a change only the owner can allow. An entry widened after its
    /// acceptance ends at `oracle_change`, never at a failure. `Expect: <code>` above the diff
    /// asks for it.
    NeedsYou(String),
}

#[derive(Debug, PartialEq, Eq, Default)]
pub struct CanaryHeader {
    pub id: Option<String>,
    pub category: Option<String>,
    pub check: Option<String>,
    pub tree: Option<String>,
    pub expect: Option<String>,
}

/// Reads the `ID:`, `Category:`, `Check:`, `Tree:` and `Expect:` lines above a canary's diff.
pub fn parse_canary_header(text: &str) -> CanaryHeader {
    let mut header = CanaryHeader::default();
    for line in text.lines() {
        if line.starts_with("diff --git") || line.starts_with("--- ") {
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = Some(value.trim().to_owned());
        match key.trim().to_lowercase().as_str() {
            "id" => header.id = value,
            "category" => header.category = value,
            "check" => header.check = value,
            "tree" => header.tree = value,
            "expect" => header.expect = value,
            _ => {}
        }
    }
    header
}

pub fn load_canary(path: &Path) -> Result<Canary, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let header = parse_canary_header(&text);
    let tree = match header.tree.as_deref() {
        None | Some("committed") => Tree::Committed,
        Some("uncommitted") => Tree::Uncommitted,
        Some(other) => {
            return Err(format!(
                "{}: Tree: is committed or uncommitted, not {other}",
                path.display()
            ));
        }
    };
    let expect = match header.expect.as_deref() {
        None | Some("assertion") => Expect::Assertion,
        Some(code) if NEEDS_YOU_CODES.contains(&code) => Expect::NeedsYou(code.to_owned()),
        Some(other) => {
            return Err(format!(
                "{}: Expect: is assertion or a needs you code, not {other}",
                path.display()
            ));
        }
    };
    match header {
        CanaryHeader {
            id: Some(id),
            category: Some(category),
            check: Some(check),
            ..
        } => Ok(Canary {
            path: path.to_path_buf(),
            id,
            category,
            check,
            tree,
            expect,
        }),
        _ => Err(format!(
            "{}: needs ID:, Category: and Check: lines above the diff",
            path.display()
        )),
    }
}

pub fn oracle_canaries(root: &Path) -> Result<Vec<Canary>, String> {
    let skip = |_: &str| false;
    crate::repo::walk(root, &root.join("verify/canaries"), &skip)
        .into_iter()
        .filter(|rel| rel.ends_with(".patch"))
        .map(|rel| load_canary(&root.join(rel)))
        .collect()
}

pub fn caught(result: &CheckResult) -> bool {
    result.outcome == Outcome::Fail && result.kind == Some(FailKind::Assertion)
}

pub fn caught_as(result: &CheckResult, expect: &Expect) -> bool {
    match expect {
        Expect::Assertion => caught(result),
        Expect::NeedsYou(code) => {
            result.outcome == Outcome::NeedsYou && result.code.as_deref() == Some(code.as_str())
        }
    }
}

/// Applies each canary to a clone of this tree and prints the verdict of its check. True when
/// every check caught its canary.
pub fn canary(root: &Path, phase: u8, patches: &[PathBuf]) -> Result<bool, String> {
    let clone = ScratchClone::prepare(root, "canary")?;
    let base = git(&clone.dir, &["rev-parse", "HEAD"])?.trim().to_owned();
    let dirty = !git(&clone.dir, &["status", "--porcelain"])?
        .trim()
        .is_empty();
    let base = if dirty {
        clone.commit_freeze(phase)?
    } else {
        base
    };
    let mut all_caught = true;
    for patch in patches {
        let canary = load_canary(patch)?;
        clone.reset_to(&base)?;
        clone.apply(&canary)?;
        let result = clone.run_check(phase, &canary.check)?;
        let caught = caught_as(&result, &canary.expect);
        let verdict = match (caught, &canary.expect) {
            (false, _) => "SURVIVED".to_owned(),
            (true, Expect::Assertion) => "caught".to_owned(),
            (true, Expect::NeedsYou(code)) => format!("caught as {code}"),
        };
        all_caught &= caught;
        println!(
            "{verdict}: {} ({}, {}) by {}: {}",
            canary.path.display(),
            canary.id,
            canary.category,
            canary.check,
            first_line(&result.message)
        );
    }
    Ok(all_caught)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn parse_canary_header__should_read_the_lines_above_the_diff() {
        // Given
        let text = "ID: harness\nCategory: harness\nCheck: oracle_unchanged\n\ndiff --git a/x b/x\nCheck: not this\n";

        // When
        let header = parse_canary_header(text);

        // Then
        assert_eq!(
            header,
            CanaryHeader {
                id: Some("harness".into()),
                category: Some("harness".into()),
                check: Some("oracle_unchanged".into()),
                tree: None,
                expect: None,
            }
        );
    }

    fn must_load(header_lines: &str) -> Result<Canary, String> {
        // Cases of two tests share header lines, and the tests of one process run in parallel.
        static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("xtask-canary-{}-{call}.patch", std::process::id(),));
        let text = format!(
            "ID: harness\nCategory: harness\nCheck: oracle_unchanged\n{header_lines}\ndiff --git a/x b/x\n"
        );
        fs::write(&path, text).unwrap();
        let canary = load_canary(&path);
        let _ = fs::remove_file(&path);
        canary
    }

    #[rstest]
    #[case::default("", Ok(Tree::Committed))]
    #[case::committed("Tree: committed\n", Ok(Tree::Committed))]
    #[case::uncommitted("Tree: uncommitted\n", Ok(Tree::Uncommitted))]
    #[case::unknown("Tree: dirty\n", Err(()))]
    fn load_canary__should_read_how_the_check_sees_the_tree(
        #[case] tree_line: &str,
        #[case] expected: Result<Tree, ()>,
    ) {
        // Given
        let lines = tree_line;

        // When
        let canary = must_load(lines);

        // Then
        assert_eq!(canary.map(|c| c.tree).map_err(|_| ()), expected);
    }

    #[rstest]
    #[case::default("", Ok(Expect::Assertion))]
    #[case::assertion("Expect: assertion\n", Ok(Expect::Assertion))]
    #[case::needs_you(
        "Expect: oracle_change\n",
        Ok(Expect::NeedsYou("oracle_change".into()))
    )]
    #[case::unknown_code("Expect: oracle_changed\n", Err(()))]
    fn load_canary__should_read_what_catches_it(
        #[case] expect_line: &str,
        #[case] expected: Result<Expect, ()>,
    ) {
        // Given
        let lines = expect_line;

        // When
        let canary = must_load(lines);

        // Then
        assert_eq!(canary.map(|c| c.expect).map_err(|_| ()), expected);
    }

    #[rstest]
    #[case::assertion_by_assertion(
        Expect::Assertion,
        CheckResult::assertion("c", "harness", "x"),
        true
    )]
    #[case::assertion_by_needs_you(
        Expect::Assertion,
        CheckResult::needs_you("c", "harness", "oracle_change", "x"),
        false
    )]
    #[case::needs_you_by_its_code(
        Expect::NeedsYou("oracle_change".into()),
        CheckResult::needs_you("c", "harness", "oracle_change", "x"),
        true
    )]
    #[case::needs_you_by_another_code(
        Expect::NeedsYou("oracle_change".into()),
        CheckResult::needs_you("c", "harness", "machine_busy", "x"),
        false
    )]
    #[case::needs_you_by_assertion(
        Expect::NeedsYou("oracle_change".into()),
        CheckResult::assertion("c", "harness", "x"),
        false
    )]
    #[case::needs_you_by_pass(
        Expect::NeedsYou("oracle_change".into()),
        CheckResult::pass("c", "harness", "x"),
        false
    )]
    fn caught_as__should_match_only_the_expected_outcome(
        #[case] expect: Expect,
        #[case] result: CheckResult,
        #[case] expected: bool,
    ) {
        // Given
        let wanted = expect;

        // When
        let caught = caught_as(&result, &wanted);

        // Then
        assert_eq!(caught, expected);
    }
}
