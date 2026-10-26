//! The bench part and the machine_busy rule of verification.md, "Budgets".

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::builds::{chromium, release_wtpool, web_build};
use super::js::{PlaywrightRun, every_wanted, playwright};
use super::{LAYER_LIMIT, Wanted};
use crate::machine_lock;
use crate::parts::Ctx;
use crate::results::CheckResult;
use crate::system::{self, Process};
use crate::web::{self, Registry};

/// The bench part: waits for an idle machine, then runs the Rust budgets and B8.
pub fn bench(ctx: &Ctx) -> Vec<CheckResult> {
    let registry = match Registry::load(&ctx.root) {
        Ok(registry) => registry,
        Err(error) => return vec![CheckResult::error("web:registry", "bench", error)],
    };
    let wanted = Wanted {
        ctx,
        registry: &registry,
    };
    let rust = wanted.in_layer("bench");
    let browser = wanted.in_layer("bench-chromium");
    if rust.is_empty() && browser.is_empty() {
        return Vec::new();
    }
    let mut results = Vec::new();
    // Its builds hold the machine's verify lock shared, as any part does; the budgets hold it
    // alone.
    let builds = machine_lock::turn();
    let release = match release_wtpool(ctx) {
        Ok(path) => path,
        Err(error) => {
            let message = format!("the release build failed: {error}");
            let mut failed = every_wanted(&wanted, "bench", |check| {
                CheckResult::error(check, "bench", message.clone())
            });
            failed.extend(every_wanted(&wanted, "bench-chromium", |check| {
                CheckResult::error(check, "bench", message.clone())
            }));
            return failed;
        }
    };
    results.extend(web_build(ctx, "bench", false));
    if !browser.is_empty() {
        results.extend(chromium(ctx, "bench-chromium"));
    }
    drop(builds);
    let lock = machine_lock::path();
    let lock = lock.as_deref().map_err(String::as_str);
    let idle = match wait_for_idle_machine(&ctx.run_dir, lock, Duration::from_secs(600)) {
        Ok(idle) => idle,
        Err(busy) => {
            for check in rust.iter().chain(&browser) {
                results.push(CheckResult::needs_you(
                    check,
                    "bench",
                    "machine_busy",
                    busy.clone(),
                ));
            }
            return web::merge(results);
        }
    };
    let mut budgets = Vec::new();
    if !rust.is_empty() {
        budgets.extend(rust_bench(ctx, &wanted, &release));
    }
    if !browser.is_empty() {
        budgets.extend(playwright(
            ctx,
            &wanted,
            "bench-chromium",
            "vscode/test/bench/playwright.config.mjs",
            PlaywrightRun {
                folder: Some("bench"),
                ..PlaywrightRun::default()
            },
        ));
    }
    // A busy machine only slows a budget down, so a pass still holds; each result says the load
    // went unread, never skipping the wait in silence.
    if let Some(reason) = &idle.unmeasured {
        for result in &mut budgets {
            result.message = format!(
                "{} (the machine's load was not read: {reason})",
                result.message
            )
            .trim_start()
            .to_owned();
        }
    }
    results.extend(budgets);
    web::merge(results)
}

/// The module of the int test binary that holds the Rust budgets.
pub const BENCH_MODULE: &str = "checks::bench::";

fn rust_bench(ctx: &Ctx, wanted: &Wanted, release: &Path) -> Vec<CheckResult> {
    let junit_path = ctx.root.join("xtask/target/nextest/bench/junit.xml");
    let _ = fs::remove_file(&junit_path);
    let mut tests = ctx.command("cargo");
    tests
        .args([
            "nextest",
            "run",
            "--locked",
            "--manifest-path",
            "xtask/Cargo.toml",
        ])
        .args(["--test", "int", "--profile", "bench", "--config-file"])
        .arg(ctx.root.join("verify/nextest.toml"))
        .env("WTPOOL_VERIFY_REPO", &ctx.root)
        .env("WTPOOL_VERIFY_RUN_DIR", &ctx.run_dir)
        .env("WTPOOL_RELEASE_BIN", release)
        .env("WTPOOL_BIN", release)
        .env("WTPOOL_VERIFY_PHASE", ctx.phase.to_string());
    let filter = match &ctx.only {
        Some(only) => format!("test(/^{BENCH_MODULE}{only}$/)"),
        None => format!("test(/^{BENCH_MODULE}/)"),
    };
    tests.args(["-E", &filter]);
    let ran = ctx.run_logged("bench_tests", &mut tests, LAYER_LIMIT);
    let xml = fs::read_to_string(&junit_path).unwrap_or_default();
    let tests = crate::junit::to_outcomes(&xml);
    if tests.is_empty() {
        let message = match ran {
            Ok(ran) => ran.tail(40),
            Err(error) => error,
        };
        return vec![CheckResult::error("bench:tests", "bench", message)];
    }
    let registered = wanted.registry.in_layer("bench");
    web::layer_results("bench", tests, &registered, |check| wanted.check(check))
}

/// An idle machine to measure on.
#[derive(Debug)]
struct Idle {
    /// The machine's verify lock, held alone until the budgets end so that no part starts while
    /// they measure.
    _lock: Option<fs::File>,
    /// The reason the load went unread, when this system gives no reading.
    unmeasured: Option<String>,
}

/// Waits, at most `limit`, until no other verify part holds the machine's verify `lock`, at most
/// half the cores are busy and no build runs outside `run_dir`. `Ok` once idle, or at once when
/// this system gives no reading, so the budgets run and say so; the reason it is still busy
/// otherwise.
fn wait_for_idle_machine(
    run_dir: &Path,
    lock: Result<&Path, &str>,
    limit: Duration,
) -> Result<Idle, String> {
    let started = Instant::now();
    let mut held = None;
    let mut said = false;
    loop {
        let reading = if held.is_some() {
            machine(run_dir, CPU_SAMPLE)
        } else {
            match lock.map_err(str::to_owned).and_then(machine_lock::alone) {
                Ok(Some(file)) => {
                    held = Some(file);
                    machine(run_dir, CPU_SAMPLE)
                }
                Ok(None) => Machine::Busy("another verify part is under way".to_owned()),
                Err(reason) => Machine::Unmeasured(reason),
            }
        };
        match reading {
            Machine::Idle => {
                return Ok(Idle {
                    _lock: held,
                    unmeasured: None,
                });
            }
            Machine::Unmeasured(reason) => {
                return Ok(Idle {
                    _lock: held,
                    unmeasured: Some(reason),
                });
            }
            Machine::Busy(reason) if started.elapsed() >= limit => {
                return Err(format!("busy for {} s: {reason}", limit.as_secs()));
            }
            Machine::Busy(reason) => {
                if !said {
                    eprintln!("bench: waiting up to {} s: {reason}", limit.as_secs());
                    said = true;
                }
                std::thread::sleep(Duration::from_secs(15));
            }
        }
    }
}

/// The length of one reading of the CPU use.
const CPU_SAMPLE: Duration = Duration::from_secs(3);

/// The state of the machine, read the same way on every system (crate::system). The use of all
/// cores over a sample stands for the load: Windows has no load average.
#[derive(Debug)]
enum Machine {
    Idle,
    Busy(String),
    /// This system gives no reading, for this reason.
    Unmeasured(String),
}

type Processes = BTreeMap<u32, Process>;

/// The run's own processes: `me`, its ancestors and its descendants. Builds of other sessions
/// under the same ancestors are not the run's.
fn own_processes(listed: &Processes, me: u32) -> BTreeSet<u32> {
    let parent = |pid: u32| listed.get(&pid).and_then(|process| process.parent);
    let mut own = BTreeSet::new();
    let mut current = Some(me);
    while let Some(pid) = current.filter(|pid| *pid > 1 && own.insert(*pid)) {
        current = parent(pid);
    }
    let mut below = BTreeSet::from([me]);
    let mut changed = true;
    while changed {
        changed = false;
        for (pid, process) in listed {
            if process.parent.is_some_and(|ppid| below.contains(&ppid)) && below.insert(*pid) {
                changed = true;
            }
        }
    }
    own.extend(below);
    own
}

/// The first build of `listed` that is not the run's own and works outside `run_dirs`.
fn foreign_build(listed: &Processes, me: u32, run_dirs: &[PathBuf]) -> Option<String> {
    let own = own_processes(listed, me);
    listed.iter().find_map(|(pid, process)| {
        let name = &process.name;
        let is_build = name == "cargo" || name.starts_with("cargo-") || name == "rustc";
        // An empty folder is one this account may not read: outside, as nothing says otherwise.
        let inside = run_dirs.iter().any(|dir| process.cwd.starts_with(dir));
        (is_build && !own.contains(pid) && !inside)
            .then(|| format!("a build runs outside the sandbox: {name} ({pid})"))
    })
}

/// Reads the machine over `sample`; a build inside `run_dir` is the run's.
fn machine(run_dir: &Path, sample: Duration) -> Machine {
    let (used, cores) = match system::cpu_use(sample) {
        Ok(reading) => reading,
        Err(reason) => return Machine::Unmeasured(reason),
    };
    if used > 50.0 {
        return Machine::Busy(format!(
            "{used:.0} % of {cores} cores busy over {} s, over half",
            sample.as_secs_f32()
        ));
    }
    let listed = match system::processes() {
        Ok(listed) => listed,
        Err(reason) => return Machine::Unmeasured(reason),
    };
    let mut run_dirs = vec![run_dir.to_path_buf()];
    run_dirs.extend(fs::canonicalize(run_dir).ok());
    match foreign_build(&listed, std::process::id(), &run_dirs) {
        Some(reason) => Machine::Busy(reason),
        None => Machine::Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn listed(rows: &[(u32, u32, &str, &str)]) -> Processes {
        rows.iter()
            .map(|(pid, ppid, name, cwd)| {
                let process = Process {
                    parent: Some(*ppid),
                    name: (*name).to_owned(),
                    cwd: PathBuf::from(cwd),
                };
                (*pid, process)
            })
            .collect()
    }

    #[rstest]
    #[case::idle(&[(10, 1, "zsh", "/home/u")], None)]
    #[case::own_build(&[(10, 1, "zsh", "/"), (20, 10, "cargo", "/elsewhere")], None)]
    #[case::build_in_the_run(&[(10, 1, "zsh", "/"), (30, 1, "rustc", "/run/x/sandbox")], None)]
    #[case::foreign_cargo(&[(10, 1, "zsh", "/"), (30, 1, "cargo", "/home/u/app")], Some(30))]
    #[case::foreign_subcommand(&[(10, 1, "zsh", "/"), (30, 1, "cargo-nextest", "")], Some(30))]
    #[case::sibling_of_an_ancestor(
        &[(5, 1, "claude", "/"), (10, 5, "zsh", "/"), (30, 5, "rustc", "/home/u/app")],
        Some(30)
    )]
    #[case::other_program(&[(10, 1, "zsh", "/"), (30, 1, "cargox", "/home/u/app")], None)]
    fn foreign_build__should_name_only_builds_of_others_outside_the_run(
        #[case] rows: &[(u32, u32, &str, &str)],
        #[case] expected: Option<u32>,
    ) {
        // Given
        let table = listed(rows);

        // When
        let found = foreign_build(&table, 10, &[PathBuf::from("/run/x")]);

        // Then
        let pid = found.as_deref().and_then(|text| {
            text.rsplit_once('(')
                .and_then(|(_, rest)| rest.trim_end_matches(')').parse::<u32>().ok())
        });
        assert_eq!(pid, expected, "{found:?}");
    }

    #[test]
    fn wait_for_idle_machine__should_report_busy_while_another_part_holds_the_lock() {
        // Given
        let dir = std::env::temp_dir().join(format!("xtask-bench-lock-{}", std::process::id()));
        let lock = dir.join("verify.lock");
        let part = machine_lock::shared(&lock).expect("another part's turn");

        // When
        let found = wait_for_idle_machine(&dir, Ok(&lock), Duration::ZERO);

        // Then
        assert_eq!(
            found.err().as_deref(),
            Some("busy for 0 s: another verify part is under way")
        );
        drop(part);
        let ended = wait_for_idle_machine(&dir, Err("no cache folder"), Duration::ZERO);
        assert!(
            ended.is_ok_and(|idle| idle._lock.is_none()
                && idle.unmeasured.as_deref() == Some("no cache folder"))
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(not(windows))]
    #[test]
    fn machine__should_read_this_system() {
        // Given
        let run_dir = std::env::temp_dir().join("xtask-machine");

        // When
        let found = machine(&run_dir, Duration::from_millis(300));

        // Then
        assert!(!matches!(found, Machine::Unmeasured(_)), "{found:?}");
    }
}
