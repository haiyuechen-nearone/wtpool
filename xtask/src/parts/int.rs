//! The int part: `unit`, the builds of `wtpool`, and the int checks, with line coverage.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::{Ctx, NEXTEST_CONFIG, PART_LIMIT, VERSION_CHECK};
use crate::cmd;
use crate::junit;
use crate::rebuild::{rebuild_steps, wtpool_package};
use crate::results::{CheckResult, FailKind, Outcome};

pub(super) fn int(ctx: &Ctx) -> Vec<CheckResult> {
    let mut results = Vec::new();
    let coverage = Coverage::new(ctx, &mut results);
    if ctx.wants("unit") {
        results.extend(unit(ctx, &coverage));
    }
    let wants_int = ctx
        .only
        .as_deref()
        .is_none_or(|only| only != "unit" && crate::clone::part_for(&ctx.root, only) == "int");
    if !wants_int {
        return results;
    }
    remove_profraw(&coverage.target);
    let mut env = TestEnv {
        repo: ctx.root.clone(),
        run_dir: ctx.run_dir.clone(),
        profile: coverage.profile(),
        dirty: crate::repo::dirty_fingerprint(&ctx.root)
            .ok()
            .flatten()
            .is_some(),
        ..TestEnv::default()
    };
    results.extend(builds(ctx, &coverage, &mut env));
    results.extend(int_tests(ctx, &env));
    results.extend(replays(ctx, &coverage));
    let _ = fs::remove_dir_all(ctx.run_dir.join("rebuild/repo/target"));
    if ctx.only.is_none() {
        results.extend(coverage.report(ctx, "int"));
    }
    results
}

/// The instrumented build the part's checks run, and the environment `cargo llvm-cov show-env`
/// gives for builds that write coverage into it.
struct Coverage {
    target: PathBuf,
    env: Vec<(String, String)>,
}

impl Coverage {
    /// The coverage build in `target/llvm-cov-target`; without its environment the builds write no
    /// coverage, and `results` gets the error.
    fn new(ctx: &Ctx, results: &mut Vec<CheckResult>) -> Self {
        let target = ctx.root.join("target/llvm-cov-target");
        let env = match coverage_env(ctx, &target) {
            Ok(env) => env,
            Err(error) => {
                results.push(CheckResult::error("int:coverage_env", "int", error));
                Vec::new()
            }
        };
        Self { target, env }
    }

    fn apply(&self, command: &mut Command) {
        command.envs(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }

    /// Where the processes of the int checks write their coverage.
    fn profile(&self) -> PathBuf {
        self.target.join("wtpool-int-%p-%16m.profraw")
    }

    /// The `debug` build of `wtpool`, with coverage.
    fn wtpool(&self) -> PathBuf {
        self.target.join("debug/wtpool")
    }

    /// Writes `cover/<name>.lcov` from the coverage so far; the error result when it fails.
    fn report(&self, ctx: &Ctx, name: &str) -> Option<CheckResult> {
        let check = format!("int:coverage_{name}");
        lcov_report(ctx, &self.env, name)
            .err()
            .map(|error| CheckResult::error(&check, "cover", error))
    }
}

/// The environment `cargo llvm-cov show-env` gives, for builds that write coverage.
fn coverage_env(ctx: &Ctx, target: &Path) -> Result<Vec<(String, String)>, String> {
    let text = cmd::output(
        ctx.command("cargo")
            .args(["llvm-cov", "show-env"])
            .env("CARGO_TARGET_DIR", target),
        Duration::from_secs(120),
    )?;
    let mut env: Vec<(String, String)> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| {
            let value = value
                .strip_prefix('\'')
                .and_then(|v| v.strip_suffix('\''))
                .unwrap_or(value);
            (key.trim().to_owned(), value.to_owned())
        })
        .filter(|(key, _)| !key.is_empty() && !key.contains(' '))
        .collect();
    env.push((
        "CARGO_TARGET_DIR".to_owned(),
        target.to_string_lossy().into_owned(),
    ));
    Ok(env)
}

fn remove_profraw(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("profraw") {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn lcov_report(ctx: &Ctx, env: &[(String, String)], name: &str) -> Result<(), String> {
    let out = ctx.run_dir.join("cover");
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let ran = ctx.run_logged(
        &format!("cover_{name}"),
        ctx.command("cargo")
            .args(["llvm-cov", "report", "--include-build-script", "--lcov"])
            .arg("--output-path")
            .arg(out.join(format!("{name}.lcov")))
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
        Duration::from_secs(600),
    )?;
    if ran.ok() { Ok(()) } else { Err(ran.tail(20)) }
}

/// How an int check's panic starts when the check's subject does not exist on this machine, such
/// as a Windows job outside Windows: the check ends `skipped` with the rest as its reason.
pub const SKIPPED: &str = "skipped on this machine: ";

/// Each check of the nextest JUnit report at `path`, by its first test case that did not pass.
fn junit_results(path: &Path, layer: &str) -> Vec<CheckResult> {
    let Ok(xml) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut by_check: BTreeMap<String, CheckResult> = BTreeMap::new();
    for case in junit::parse(&xml) {
        let keep_earlier = by_check
            .get(&case.check)
            .is_some_and(|earlier| earlier.outcome != Outcome::Pass);
        if keep_earlier {
            continue;
        }
        let check = case.check.clone();
        let skipped = case
            .failure
            .as_ref()
            .and_then(|failure| failure.message.strip_prefix(SKIPPED))
            .map(str::to_owned);
        let result = match skipped {
            Some(reason) => CheckResult::skipped(&check, layer, reason),
            None => crate::web::test_result(&check, layer, junit::outcome(case)),
        };
        by_check.insert(check, result);
    }
    by_check.into_values().collect()
}

/// `unit`: the workspace's own tests, with coverage, and its lcov.
fn unit(ctx: &Ctx, coverage: &Coverage) -> Vec<CheckResult> {
    let mut results = Vec::new();
    // Instrumented binaries of earlier builds, with other features, profiles or sources,
    // stay in the target folder and `llvm-cov report` would read them all.
    let mut clean = ctx.command("cargo");
    clean.args(["llvm-cov", "clean", "--workspace"]);
    coverage.apply(&mut clean);
    if let Err(error) = ctx
        .run_logged("cover_clean", &mut clean, Duration::from_secs(600))
        .and_then(|ran| if ran.ok() { Ok(()) } else { Err(ran.tail(20)) })
    {
        results.push(CheckResult::error("int:coverage_clean", "cover", error));
    }
    remove_profraw(&coverage.target);
    let junit_path = coverage.target.join("nextest/unit/junit.xml");
    let _ = fs::remove_file(&junit_path);
    let mut command = ctx.command("cargo");
    command
        .args(["nextest", "run", "--workspace", "--locked"])
        .args(["--profile", "unit", "--config-file"])
        .arg(ctx.root.join(NEXTEST_CONFIG));
    coverage.apply(&mut command);
    let mut unit = ctx.command_check("unit", "unit", &mut command, PART_LIMIT);
    let mut failed = Vec::new();
    for result in junit_results(&junit_path, "unit") {
        if result.outcome != Outcome::Pass {
            failed.push(format!("{}: {}", result.check, result.message));
        }
    }
    if !failed.is_empty() {
        unit.message = failed.join("\n");
    }
    results.push(unit);
    if ctx.only.is_none() {
        results.extend(coverage.report(ctx, "unit"));
    }
    results
}

/// The environment of the int checks (xtask/tests/support): the tree, the run folder, where their
/// processes write coverage, and the builds they test.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct TestEnv {
    repo: PathBuf,
    run_dir: PathBuf,
    profile: PathBuf,
    /// The tree has uncommitted changes, and the version checks expect them in the build.
    dirty: bool,
    package: Option<String>,
    wtpool: Option<PathBuf>,
    release: Option<PathBuf>,
    rebuild_steps: Option<PathBuf>,
}

impl TestEnv {
    /// The variables, a build the int checks lack left out.
    fn vars(&self) -> Vec<(&'static str, String)> {
        let text = |path: &Path| path.to_string_lossy().into_owned();
        let mut vars = vec![
            ("WTPOOL_VERIFY_REPO", text(&self.repo)),
            ("WTPOOL_VERIFY_RUN_DIR", text(&self.run_dir)),
            ("LLVM_PROFILE_FILE", text(&self.profile)),
            (
                "WTPOOL_VERIFY_BUILD_DIRTY",
                if self.dirty { "1" } else { "0" }.to_owned(),
            ),
        ];
        if let Some(package) = &self.package {
            vars.push(("WTPOOL_VERIFY_PACKAGE", package.clone()));
        }
        for (name, path) in [
            ("WTPOOL_BIN", &self.wtpool),
            ("WTPOOL_RELEASE_BIN", &self.release),
            ("WTPOOL_VERIFY_REBUILD_STEPS", &self.rebuild_steps),
        ] {
            if let Some(path) = path {
                vars.push((name, text(path)));
            }
        }
        vars
    }
}

/// The builds the int checks test: `wtpool` with coverage and the `verify` feature, the release
/// `wtpool`, and the rebuild steps of the version check; each one that builds goes into `env`.
fn builds(ctx: &Ctx, coverage: &Coverage, env: &mut TestEnv) -> Vec<CheckResult> {
    let package = match wtpool_package(ctx) {
        Ok(package) => package,
        Err(error) => return vec![CheckResult::error("int:build", "int", error)],
    };
    let mut results = Vec::new();
    let has_verify = package.features.contains_key("verify");
    env.package = Some(package.name.clone());
    let mut build = ctx.command("cargo");
    build.args(["build", "--locked", "-p", &package.name, "--bin", "wtpool"]);
    if has_verify {
        build.args(["--features", "verify"]);
    }
    coverage.apply(&mut build);
    let mut built = ctx.command_check("int:build", "int", &mut build, PART_LIMIT);
    if built.kind == Some(FailKind::Assertion) {
        built.kind = Some(FailKind::Error);
    }
    if built.outcome == Outcome::Pass {
        env.wtpool = Some(coverage.wtpool());
    }
    results.push(built);
    // The release wtpool is built once per run and shared (layers::release_wtpool).
    results.push(match crate::layers::release_wtpool(ctx) {
        Ok(path) => {
            env.release = Some(path);
            CheckResult::pass("int:build_release", "int", "")
        }
        Err(error) => CheckResult::error("int:build_release", "int", error),
    });
    if ctx.wants(VERSION_CHECK) {
        let features: &[&str] = if has_verify {
            &["--features", "verify"]
        } else {
            &[]
        };
        match rebuild_steps(ctx, &package.name, features, None) {
            Ok(path) => env.rebuild_steps = Some(path),
            Err(error) => results.push(CheckResult::error("int:rebuild", "int", error)),
        }
    }
    results
}

/// The int checks of the `int` test binary, the Rust budgets left to the bench part: built at the
/// part's priority, run at the normal one.
fn int_tests(ctx: &Ctx, env: &TestEnv) -> Vec<CheckResult> {
    let junit_path = ctx.root.join("xtask/target/nextest/int/junit.xml");
    let _ = fs::remove_file(&junit_path);
    let vars = env.vars();
    let int_tests = |command: &mut Command| {
        command
            .args(["nextest", "run", "--locked"])
            .args(["--manifest-path", "xtask/Cargo.toml", "--test", "int"])
            .envs(vars.iter().map(|(k, v)| (*k, v.as_str())));
    };
    // The checks time sleeps and file events, so only their build yields the processor.
    let mut build_tests = ctx.command("cargo");
    int_tests(&mut build_tests);
    build_tests.arg("--no-run");
    let mut tests = ctx.normal_priority_command("cargo");
    int_tests(&mut tests);
    tests.args(["--profile", "int", "--config-file", NEXTEST_CONFIG]);
    // The Rust budgets share the binary; only the bench part runs them.
    let budgets = format!("test(/^{}/)", crate::layers::BENCH_MODULE);
    let later = crate::trace::nextest_exclusion(&ctx.later);
    let filter = match &ctx.only {
        Some(only) => format!("test(/(^|::){only}($|::)/) and not {budgets}{later}"),
        None => format!("not {budgets}{later}"),
    };
    tests.args(["-E", &filter]);
    // `--only cc_replay:<name>` or `--only cc_replay:*` runs only the replay, no int test.
    let replay_only = ctx
        .only
        .as_deref()
        .is_some_and(|only| only.starts_with(crate::replay::PREFIX));
    let ran = if replay_only {
        Err("only the replay runs".to_owned())
    } else {
        ctx.run_logged("int_tests_build", &mut build_tests, PART_LIMIT)
            .and_then(|built| {
                if built.ok() {
                    Ok(())
                } else {
                    Err(built.tail(40))
                }
            })
            .and_then(|()| ctx.run_logged("int_tests", &mut tests, PART_LIMIT))
    };
    let mut results = Vec::new();
    let int_results = junit_results(&junit_path, "int");
    if int_results.is_empty() && !replay_only {
        let message = match ran {
            Ok(ran) => ran.tail(40),
            Err(error) => error,
        };
        results.push(CheckResult::error("int:tests", "int", message));
    }
    results.extend(int_results);
    results
}

/// `cc_replay:<name>`: the recorded contract sessions through the verify build (phase 2 on).
fn replays(ctx: &Ctx, coverage: &Coverage) -> Vec<CheckResult> {
    let wanted = ctx.phase >= 2
        && ctx
            .only
            .as_deref()
            .is_none_or(|only| only.starts_with(crate::replay::PREFIX));
    let wtpool = coverage.wtpool();
    if !wanted || !wtpool.exists() {
        return Vec::new();
    }
    let profile = coverage.profile();
    let every = ctx.only.as_deref() == Some("cc_replay:*");
    let mut results = crate::replay::replay_all(&ctx.root, &ctx.run_dir, &wtpool, Some(&profile));
    results.retain(|r| every || ctx.wants(&r.check) || ctx.only.is_none());
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn env() -> TestEnv {
        TestEnv {
            repo: PathBuf::from("/repo"),
            run_dir: PathBuf::from("/run"),
            profile: PathBuf::from("/cov/wtpool-int-%p-%16m.profraw"),
            ..TestEnv::default()
        }
    }

    #[rstest]
    #[case::nothing_built(env(), &[])]
    #[case::every_build(
        TestEnv {
            dirty: true,
            package: Some("wtpool".into()),
            wtpool: Some("/cov/debug/wtpool".into()),
            release: Some("/run/bin/wtpool".into()),
            rebuild_steps: Some("/run/rebuild/steps.json".into()),
            ..env()
        },
        &[
            ("WTPOOL_VERIFY_PACKAGE", "wtpool"),
            ("WTPOOL_BIN", "/cov/debug/wtpool"),
            ("WTPOOL_RELEASE_BIN", "/run/bin/wtpool"),
            ("WTPOOL_VERIFY_REBUILD_STEPS", "/run/rebuild/steps.json"),
        ]
    )]
    #[case::release_alone(
        TestEnv {
            release: Some("/run/bin/wtpool".into()),
            ..env()
        },
        &[("WTPOOL_RELEASE_BIN", "/run/bin/wtpool")]
    )]
    fn test_env__should_name_the_tree_run_and_each_build(
        #[case] env: TestEnv,
        #[case] builds: &[(&str, &str)],
    ) {
        // When
        let vars = env.vars();

        // Then
        let mut expected = vec![
            ("WTPOOL_VERIFY_REPO", "/repo"),
            ("WTPOOL_VERIFY_RUN_DIR", "/run"),
            ("LLVM_PROFILE_FILE", "/cov/wtpool-int-%p-%16m.profraw"),
            (
                "WTPOOL_VERIFY_BUILD_DIRTY",
                if env.dirty { "1" } else { "0" },
            ),
        ];
        expected.extend_from_slice(builds);
        assert_eq!(
            vars.iter()
                .map(|(name, value)| (*name, value.as_str()))
                .collect::<Vec<_>>(),
            expected
        );
    }
}
