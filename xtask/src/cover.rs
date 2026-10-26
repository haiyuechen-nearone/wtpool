//! `cover`: every line of product Rust and extension source changed since the base commit is hit
//! in the coverage reports of the run.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cmd::git;
use crate::oracle::is_oracle_path;
use crate::repo::base;
use crate::results::CheckResult;

pub const NAME: &str = "cover";

/// Hit counts per line, per repository relative path.
pub type Lines = BTreeMap<String, BTreeMap<u32, u64>>;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Uncovered {
    pub path: String,
    pub line: u32,
    /// Only the product's own unit tests run this line.
    pub unit_only: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Coverage {
    pub changed_lines: usize,
    pub covered: usize,
    /// Changed lines that no oracle layer runs, every changed line of an unmeasured file
    /// included.
    pub not_oracle_covered: Vec<Uncovered>,
    /// Changed product files that neither coverage file names, so no line of them was measured.
    #[serde(default)]
    pub not_measured: Vec<String>,
}

pub fn parse_lcov(text: &str, root: &Path) -> Lines {
    let mut lines: Lines = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("SF:") {
            current = Path::new(path)
                .strip_prefix(root)
                .ok()
                .map(|rel| rel.to_string_lossy().into_owned());
        } else if let Some(data) = line.strip_prefix("DA:") {
            let mut fields = data.split(',');
            let number = fields.next().and_then(|n| n.parse::<u32>().ok());
            let hits = fields.next().and_then(|n| n.parse::<u64>().ok());
            if let (Some(path), Some(number), Some(hits)) = (&current, number, hits) {
                let entry = lines
                    .entry(path.clone())
                    .or_default()
                    .entry(number)
                    .or_insert(0);
                *entry = entry.saturating_add(hits);
            }
        } else if line == "end_of_record" {
            current = None;
        }
    }
    lines
}

/// New side line numbers of a `git diff -U0`.
pub fn changed_lines(diff: &str) -> BTreeMap<String, BTreeSet<u32>> {
    let mut changed: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ ") {
            current = path.strip_prefix("b/").map(str::to_owned);
        } else if let Some(hunk) = line.strip_prefix("@@ ") {
            let Some(path) = &current else {
                continue;
            };
            let Some(new_side) = hunk.split_whitespace().find(|part| part.starts_with('+')) else {
                continue;
            };
            let mut numbers = new_side.trim_start_matches('+').split(',');
            let start = numbers
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(0);
            let count = numbers
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(1);
            let set = changed.entry(path.clone()).or_default();
            set.extend((0..count).filter_map(|offset| start.checked_add(offset)));
        }
    }
    changed
}

pub fn measure(changed: &BTreeMap<String, BTreeSet<u32>>, unit: &Lines, int: &Lines) -> Coverage {
    let mut coverage = Coverage::default();
    for (path, numbers) in changed {
        let measured = unit.contains_key(path) || int.contains_key(path);
        if !measured && is_measured_kind(path) {
            coverage.not_measured.push(path.clone());
            coverage
                .not_oracle_covered
                .extend(numbers.iter().map(|number| Uncovered {
                    path: path.clone(),
                    line: *number,
                    unit_only: false,
                }));
            continue;
        }
        for number in numbers {
            let hits = |lines: &Lines| {
                lines
                    .get(path)
                    .and_then(|by_line| by_line.get(number))
                    .copied()
            };
            let (unit_hits, int_hits) = (hits(unit), hits(int));
            if unit_hits.is_none() && int_hits.is_none() {
                continue;
            }
            coverage.changed_lines = coverage.changed_lines.saturating_add(1);
            let by_unit = unit_hits.unwrap_or(0) > 0;
            let by_int = int_hits.unwrap_or(0) > 0;
            if by_unit || by_int {
                coverage.covered = coverage.covered.saturating_add(1);
            }
            if !by_int {
                coverage.not_oracle_covered.push(Uncovered {
                    path: path.clone(),
                    line: *number,
                    unit_only: by_unit,
                });
            }
        }
    }
    coverage
}

/// False for the integration tests, benches and examples of a crate: `cargo llvm-cov` leaves them
/// out of its reports.
pub fn is_measured_kind(rel: &str) -> bool {
    !rel.split('/')
        .any(|part| matches!(part, "tests" | "benches" | "examples"))
}

pub fn is_product_source(rel: &str) -> bool {
    let rust = rel.ends_with(".rs");
    // `render` and `host` measure the extension's sources through the build's source maps (V8
    // coverage).
    let extension = rel.starts_with("vscode/src/")
        && WEB_SOURCES
            .iter()
            .any(|ext| rel.ends_with(&format!(".{ext}")))
        && !rel.ends_with(".d.ts");
    (rust || extension) && !is_oracle_path(rel, 0) && !rel.starts_with("verify/")
}

const WEB_SOURCES: [&str; 7] = ["ts", "mts", "cts", "tsx", "js", "mjs", "cjs"];

/// Adds the hit counts of `more` to `lines`.
pub fn merge_lines(lines: &mut Lines, more: Lines) {
    for (path, by_line) in more {
        let entry = lines.entry(path).or_default();
        for (number, hits) in by_line {
            let count = entry.entry(number).or_insert(0);
            *count = count.saturating_add(hits);
        }
    }
}

/// Lines changed since `base` in product Rust and extension sources, untracked files included
/// whole.
fn changed_since_base(root: &Path, base: &str) -> Result<BTreeMap<String, BTreeSet<u32>>, String> {
    let diff = git(
        root,
        &[
            "diff",
            "-U0",
            "--no-color",
            "--no-ext-diff",
            base,
            "--",
            "*.rs",
            "vscode/src",
        ],
    )?;
    let mut changed = changed_lines(&diff);
    let untracked = git(
        root,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "*.rs",
            "vscode/src",
        ],
    )?;
    for rel in untracked.split('\0').filter(|rel| !rel.is_empty()) {
        let count = fs::read_to_string(root.join(rel))
            .map(|text| text.lines().count())
            .unwrap_or(0);
        let count = u32::try_from(count).unwrap_or(u32::MAX);
        changed.insert(rel.to_owned(), (1..=count).collect());
    }
    changed.retain(|rel, _| is_product_source(rel));
    Ok(changed)
}

pub fn check(root: &Path, run_dir: &Path) -> (CheckResult, Option<Coverage>) {
    let base = match base(root) {
        Ok(base) => base,
        Err(error) => return (CheckResult::error(NAME, "cover", error), None),
    };
    let read = |name: &str| fs::read_to_string(run_dir.join("cover").join(name)).ok();
    let (Some(unit_text), Some(int_text)) = (read("unit.lcov"), read("int.lcov")) else {
        return (
            CheckResult::assertion(
                NAME,
                "cover",
                "no coverage of unit and int: run the int part in full",
            ),
            None,
        );
    };
    let changed = match changed_since_base(root, &base.commit) {
        Ok(changed) => changed,
        Err(error) => return (CheckResult::error(NAME, "cover", error), None),
    };
    // The oracle layers are int, and render and host over the extension's sources. Without their
    // lcov files no extension line would count, so cover cannot decide.
    let mut oracle_lines = parse_lcov(&int_text, root);
    let extension_changed = changed.keys().any(|rel| rel.starts_with("vscode/src/"));
    for layer in ["render.lcov", "host.lcov"] {
        match read(layer) {
            Some(text) => merge_lines(&mut oracle_lines, parse_lcov(&text, root)),
            None if extension_changed => {
                return (
                    CheckResult::assertion(
                        NAME,
                        "cover",
                        format!(
                            "no {layer} for the changed extension sources: run the quick part in full"
                        ),
                    ),
                    None,
                );
            }
            None => {}
        }
    }
    let coverage = measure(&changed, &parse_lcov(&unit_text, root), &oracle_lines);
    let mut summary = format!(
        "{} of {} lines changed since {} ({}) covered, {} not covered by an oracle layer",
        coverage.covered,
        coverage.changed_lines,
        base.commit.get(..12).unwrap_or(&base.commit),
        base.source,
        coverage.not_oracle_covered.len()
    );
    if !coverage.not_measured.is_empty() {
        summary.push_str(&format!(
            "; not measured, listed as not covered: {}",
            coverage.not_measured.join(", ")
        ));
    }
    let result = if coverage.covered == coverage.changed_lines {
        CheckResult::pass(NAME, "cover", summary)
    } else {
        CheckResult::assertion(NAME, "cover", format!("below 100 %: {summary}"))
    };
    (result, Some(coverage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn parse_lcov__should_sum_hits_per_line_below_the_root() {
        // Given
        let text = "SF:/r/crates/a.rs\nDA:1,2\nDA:2,0\nend_of_record\nSF:/r/crates/a.rs\nDA:1,3\nend_of_record\nSF:/elsewhere/b.rs\nDA:1,1\nend_of_record\n";

        // When
        let lines = parse_lcov(text, Path::new("/r"));

        // Then
        let expected: Lines =
            BTreeMap::from([("crates/a.rs".to_owned(), BTreeMap::from([(1, 5), (2, 0)]))]);
        assert_eq!(lines, expected);
    }

    #[test]
    fn changed_lines__should_read_new_side_ranges() {
        // Given
        let diff = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -1 +1,2 @@\n-a\n+b\n+c\n@@ -9,2 +10,0 @@\n-d\n-e\n@@ -20 +20 @@\n-f\n+g\ndiff --git a/gone.rs b/gone.rs\n--- a/gone.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-h\n";

        // When
        let changed = changed_lines(diff);

        // Then
        let expected = BTreeMap::from([("x.rs".to_owned(), BTreeSet::from([1, 2, 20]))]);
        assert_eq!(changed, expected);
    }

    #[test]
    fn measure__should_count_union_and_mark_unit_only_lines() {
        // Given
        let changed = BTreeMap::from([("a.rs".to_owned(), BTreeSet::from([1, 2, 3, 4, 5]))]);
        let unit: Lines =
            BTreeMap::from([("a.rs".to_owned(), BTreeMap::from([(1, 1), (2, 1), (3, 0)]))]);
        let int: Lines =
            BTreeMap::from([("a.rs".to_owned(), BTreeMap::from([(1, 1), (3, 0), (4, 0)]))]);

        // When
        let coverage = measure(&changed, &unit, &int);

        // Then
        assert_eq!(
            coverage,
            Coverage {
                changed_lines: 4,
                covered: 2,
                not_oracle_covered: vec![
                    Uncovered {
                        path: "a.rs".into(),
                        line: 2,
                        unit_only: true
                    },
                    Uncovered {
                        path: "a.rs".into(),
                        line: 3,
                        unit_only: false
                    },
                    Uncovered {
                        path: "a.rs".into(),
                        line: 4,
                        unit_only: false
                    },
                ],
                not_measured: vec![],
            }
        );
    }

    #[rstest]
    #[case::build_script("crates/wtpool/build.rs", &["crates/wtpool/build.rs"][..], 2)]
    #[case::module_list("crates/wtpool-core/src/lib.rs", &["crates/wtpool-core/src/lib.rs"][..], 2)]
    #[case::integration_test("crates/wtpool/tests/cli.rs", &[][..], 0)]
    #[case::bench("crates/wtpool/benches/hook.rs", &[][..], 0)]
    #[case::measured("crates/wtpool/src/main.rs", &[][..], 0)]
    fn measure__should_list_every_changed_line_of_an_unmeasured_file(
        #[case] path: &str,
        #[case] not_measured: &[&str],
        #[case] listed_lines: usize,
    ) {
        // Given
        let changed = BTreeMap::from([(path.to_owned(), BTreeSet::from([1, 2]))]);
        let measured: Lines = BTreeMap::from([(
            "crates/wtpool/src/main.rs".to_owned(),
            BTreeMap::from([(1, 1), (2, 1)]),
        )]);

        // When
        let coverage = measure(&changed, &measured, &measured);

        // Then
        assert_eq!(
            (coverage.not_measured, coverage.not_oracle_covered.len()),
            (
                not_measured
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect::<Vec<_>>(),
                listed_lines
            )
        );
    }

    #[rstest]
    #[case::rust("crates/wtpool/src/main.rs", true)]
    #[case::extension("vscode/src/extension.ts", true)]
    #[case::webview_js("vscode/src/webview/main.mjs", true)]
    #[case::declarations("vscode/src/types.d.ts", false)]
    #[case::style("vscode/src/webview/panel.css", false)]
    #[case::oracle_test("vscode/test/render/rows.test.mjs", false)]
    #[case::build_script("vscode/scripts/build.mjs", false)]
    #[case::oracle_rust("xtask/src/cover.rs", false)]
    fn is_product_source__should_take_rust_and_extension_sources(
        #[case] path: &str,
        #[case] expected: bool,
    ) {
        // Given
        let rel = path;

        // When
        let product = is_product_source(rel);

        // Then
        assert_eq!(product, expected);
    }
}
