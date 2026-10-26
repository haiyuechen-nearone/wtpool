//! `verify --report`: the checks the report computes over a run's parts, and its files.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::Serialize;

use super::markdown::{Diffs, Header, html, markdown};
use super::verdict::{Verdict, required_parts, verdict};
use crate::cmd::{self, git};
use crate::cover::{self, Coverage};
use crate::flaky;
use crate::isolation::{self, Places};
use crate::oracle::{ACCEPTED, ORACLE_KEYS, freeze_commit, is_oracle_path};
use crate::results::{CheckResult, load_all, load_part};
use crate::trace;

#[derive(Serialize)]
struct ReportJson<'a> {
    verdict: Verdict,
    phase: u8,
    run: &'a str,
    commit: &'a str,
    base: &'a str,
    base_source: &'a str,
    claude_code: &'a str,
    results: &'a [CheckResult],
    coverage: &'a Option<Coverage>,
}

pub fn report(root: &Path, phase: u8, run: &str, run_dir: &Path) -> Result<Verdict, String> {
    let mut results = load_all(run_dir)?;
    let earlier = run_dir
        .parent()
        .map(|out| flaky::earlier_runs(out, run, phase))
        .unwrap_or_default();
    let streaks = flaky::apply(&mut results, &earlier);
    let mut contract = if required_parts(phase).contains(&"cc") {
        "by the cc part (the live result, or --contract)".to_owned()
    } else {
        format!("not in phase {phase}")
    };
    for part in required_parts(phase) {
        if load_part(run_dir, part)?.is_none() {
            // Without the cc part, a machine that never recorded a live run counts the cc checks
            // by this run's replays (crate::layers::cc).
            if *part == "cc"
                && let Some(by_replay) = crate::layers::report_by_replay(root, run_dir)
            {
                results.extend(by_replay);
                continue;
            }
            results.push(CheckResult::not_run(
                &format!("part:{part}"),
                "report",
                format!("run `cargo xtask verify --phase {phase} --part {part}`"),
            ));
        }
    }
    if results
        .iter()
        .any(|r| r.layer == "cc" && r.message.starts_with(crate::layers::REPLAY_ONLY))
    {
        contract = format!(
            "{}, each by this run's cc_replay:<name>; no live Claude Code session ({})",
            crate::layers::REPLAY_ONLY,
            crate::layers::replay_only_reason()
                .unwrap_or_else(|| "a live result appeared after the cc part ran".to_owned())
        );
    }
    let (cover_result, coverage) = cover::check(root, run_dir);
    results.push(cover_result);
    let sandbox_roots = sandbox_roots(run_dir);
    let places = Places::real().with_extension_of(root);
    let last = isolation::snapshot(&places, run_dir);
    // The places the final snapshot compared, for a reviewer. Outside `isolation/`, the folder of
    // part records only.
    let snapshot_text = serde_json::to_string_pretty(&last).map_err(|e| e.to_string())?;
    fs::write(run_dir.join("isolation-report.json"), snapshot_text).map_err(|e| e.to_string())?;
    results.push(isolation::check(run_dir, &places, &sandbox_roots, &last));
    if phase >= 1 {
        results.push(crate::flows::check(root, phase, &results));
    }
    let complete = trace::check(root, phase, &results);
    results.push(complete);
    let verdict = verdict(&results);

    let commit = crate::repo::head(root)?;
    let dirty = crate::repo::dirty_fingerprint(root)?.is_some();
    let commit_text = if dirty {
        format!("{commit} (uncommitted changes)")
    } else {
        commit
    };
    let (base, base_source) = match crate::repo::base(root) {
        Ok(found) => (found.commit, found.source),
        Err(error) => ("missing".to_owned(), error),
    };
    let claude = claude_version();

    let markdown = markdown(
        &Header {
            verdict,
            phase,
            run,
            commit: &commit_text,
            base: &base,
            base_source: &base_source,
            claude: &claude,
            contract: &contract,
        },
        &results,
        &streaks,
        &coverage,
        &diffs(root, &base, phase),
        &reused_parts(run_dir),
    );
    fs::write(run_dir.join("report.md"), &markdown).map_err(|e| e.to_string())?;
    fs::write(run_dir.join("report.html"), html(&markdown, verdict)).map_err(|e| e.to_string())?;
    let json = ReportJson {
        verdict,
        phase,
        run,
        commit: &commit_text,
        base: &base,
        base_source: &base_source,
        claude_code: &claude,
        results: &results,
        coverage: &coverage,
    };
    let text = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    fs::write(run_dir.join("report.json"), text).map_err(|e| e.to_string())?;
    Ok(verdict)
}

/// Each part of the run that copied an earlier result: its source and input hash.
fn reused_parts(run_dir: &Path) -> Vec<String> {
    crate::PARTS
        .iter()
        .filter_map(|part| load_part(run_dir, part).ok().flatten())
        .filter_map(|file| {
            file.reused.map(|r| {
                format!(
                    "`{}` from `{}`, commit {}, input hash {}",
                    file.part, r.from, r.commit, r.hash
                )
            })
        })
        .collect()
}

fn sandbox_roots(run_dir: &Path) -> Vec<String> {
    let mut roots = vec![run_dir.to_string_lossy().into_owned()];
    for dir in [crate::dirs::cache_dir(), crate::dirs::data_dir()]
        .into_iter()
        .flatten()
    {
        roots.push(dir.to_string_lossy().into_owned());
    }
    roots
}

fn claude_version() -> String {
    cmd::output(
        Command::new("claude").arg("--version"),
        Duration::from_secs(20),
    )
    .map(|text| text.trim().to_owned())
    .unwrap_or_else(|_| "not installed".to_owned())
}

/// The files of `listing`, git's `-z` output, that are oracle files at `phase` or hold oracle keys.
fn oracle_names(listing: &str, phase: u8) -> Vec<&str> {
    let mut found = Vec::new();
    for rel in listing.split('\0') {
        let holds_keys = ORACLE_KEYS.iter().any(|(file, _)| *file == rel);
        if !rel.is_empty() && (is_oracle_path(rel, phase) || holds_keys) {
            found.push(rel);
        }
    }
    found
}

/// The `--stat` of the oracle files that differ from `freeze`, then the untracked ones.
fn oracle_diff(root: &Path, freeze: &str, phase: u8) -> String {
    let changed = git(
        root,
        &["diff", "--name-only", "--no-renames", "-z", freeze, "--"],
    );
    let stat = match changed {
        Err(error) => error,
        Ok(listing) => {
            let changed = oracle_names(&listing, phase);
            if changed.is_empty() {
                String::new()
            } else {
                let mut args = vec!["diff", "--stat", freeze, "--"];
                args.extend(changed);
                git(root, &args).unwrap_or_else(|e| e)
            }
        }
    };
    let untracked =
        git(root, &["ls-files", "--others", "--exclude-standard", "-z"]).unwrap_or_default();
    let untracked = oracle_names(&untracked, phase);
    if untracked.is_empty() {
        stat
    } else {
        format!("{stat}untracked:\n{}\n", untracked.join("\n"))
    }
}

fn diffs(root: &Path, base: &str, phase: u8) -> Diffs {
    let (oracle, accepted) = match freeze_commit(root) {
        None => ("no freeze commit".to_owned(), "no freeze commit".to_owned()),
        Some(freeze) => {
            let oracle = oracle_diff(root, &freeze, phase);
            let accepted =
                git(root, &["diff", freeze.as_str(), "--", ACCEPTED]).unwrap_or_else(|e| e);
            (oracle, accepted)
        }
    };
    let spec = if base == "missing" {
        "no base commit".to_owned()
    } else {
        git(root, &["diff", "--stat", base, "--", "docs/design"]).unwrap_or_else(|e| e)
    };
    Diffs {
        oracle: or_none(oracle),
        accepted: or_none(accepted),
        spec: or_none(spec),
    }
}

fn or_none(text: String) -> String {
    if text.trim().is_empty() {
        "none".to_owned()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_names__should_keep_oracle_files_and_files_with_oracle_keys() {
        // Given
        let listing = "xtask/src/lib.rs\0crates/wtpool/src/main.rs\0verify/reviews/x.md\0Cargo.toml\0verify/oracle-changes.toml\0vscode/package.json\0";

        // When
        let phase_0 = oracle_names(listing, 0);
        let phase_1 = oracle_names(listing, 1);

        // Then
        assert_eq!(phase_0, vec!["xtask/src/lib.rs", "Cargo.toml"]);
        assert_eq!(
            phase_1,
            vec!["xtask/src/lib.rs", "Cargo.toml", "vscode/package.json"]
        );
    }
}
