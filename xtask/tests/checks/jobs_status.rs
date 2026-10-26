//! Heavy patterns in `wtpool status` and `wtpool job ls`: builds are the processes that match a
//! heavy pattern, `queued` when a job holds them, and the policy's flags.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::hook::{Hooks, catalog_spec};
use crate::support::jobs::policy::{
    call, decision, list_job, machine, must_have, policy, proc, started,
};
use crate::support::jobs::settings::add_to_repository;
use crate::support::jobs::{JobSpec, ProcLine, jobs_of, must_job_ls_json, wtpool};
use crate::support::{Sandbox, must_status_json, worktree};

/// Each build of the worktree `name`: its name, pid and `queued`.
fn builds_of(document: &Value, name: &str) -> BTreeSet<(String, u64, String)> {
    worktree(document, name)["builds"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|build| {
            (
                build["name"].as_str().unwrap_or_default().to_owned(),
                build["pid"].as_u64().unwrap_or_default(),
                build["queued"].to_string(),
            )
        })
        .collect()
}

/// The build set of a worktree as [`builds_of`] gives it.
fn set(builds: &[(&str, u64, bool)]) -> BTreeSet<(String, u64, String)> {
    builds
        .iter()
        .map(|(name, pid, queued)| ((*name).to_owned(), *pid, queued.to_string()))
        .collect()
}

/// `machine.toml` with only the heavy patterns `heavy`.
fn heavy_only(heavy: &[&str]) -> String {
    let list: Vec<String> = heavy.iter().map(|p| format!("\"{p}\"")).collect();
    format!("[jobs]\nheavy = [{}]\n", list.join(", "))
}

/// The name of the build each worktree shows for one process, as `(worktree, name)`.
fn shown(document: &Value) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for name in ["fg", "wt1", "wt2", "wt3", "x"] {
        for build in worktree(document, name)["builds"]
            .as_array()
            .into_iter()
            .flatten()
        {
            found.push((
                name.to_owned(),
                build["name"].as_str().unwrap_or_default().to_owned(),
            ));
        }
    }
    found
}

/// `wtpool status --json` with one process, `name` with `cmd` in `cwd` below the pool, and the
/// machine's heavy patterns `heavy`.
fn status_with(
    sandbox_name: &str,
    heavy: &[&str],
    name: &str,
    cmd: Option<&[&str]>,
    cwd: &str,
) -> Value {
    let sandbox = Sandbox::must_new(sandbox_name);
    let pool = sandbox.must_pool(&catalog_spec());
    sandbox.write_machine(&heavy_only(heavy));
    let folder = pool.dir.join(cwd);
    fs::create_dir_all(&folder).expect("setup: create the process folder");
    let procs = sandbox.must_write_proc_lines(&[ProcLine {
        pid: 4242,
        ppid: 1,
        name,
        cmd,
        cwd: &folder,
    }]);
    must_status_json(
        &sandbox,
        &pool.fg,
        &[("WTPOOL_VERIFY_PROCS", procs.as_os_str())],
    )
}

#[rstest]
#[case::arguments_after(&["cargo build"], "cargo", Some(&["cargo", "build", "--release"][..]), "wt1", Some(("wt1", "cargo build")))]
#[case::options_before(&["cargo build"], "cargo", Some(&["cargo", "--locked", "build"][..]), "wt1", Some(("wt1", "cargo build")))]
#[case::toolchain(&["cargo build"], "cargo", Some(&["cargo", "+nightly", "build"][..]), "wt2", Some(("wt2", "cargo build")))]
#[case::other_subcommand(&["cargo build"], "cargo", Some(&["cargo", "test"][..]), "wt1", None)]
#[case::three_words(&["cargo xtask verify"], "cargo", Some(&["cargo", "xtask", "verify", "--phase", "4"][..]), "wt1", Some(("wt1", "cargo xtask verify")))]
#[case::star(&["cargo-*"], "cargo-nextest", Some(&["/opt/cargo/bin/cargo-nextest", "nextest", "run"][..]), "wt3", Some(("wt3", "cargo-nextest")))]
#[case::star_needs_the_dash(&["cargo-*"], "cargox", Some(&["cargox"][..]), "wt3", None)]
#[case::one_word(&["make"], "make", Some(&["make", "-j8"][..]), "wt1", Some(("wt1", "make")))]
#[case::program_path(&["make"], "make", Some(&["/usr/bin/make"][..]), "wt1", Some(("wt1", "make")))]
#[case::one_word_no_arguments(&["make"], "make", None, "wt1", Some(("wt1", "make")))]
#[case::two_words_no_arguments(&["cargo build"], "cargo", None, "wt1", None)]
#[case::subfolder(&["make"], "make", Some(&["make"][..]), "wt3/sub/dir", Some(("wt3", "make")))]
#[case::foreground(&["make"], "make", Some(&["make"][..]), "fg", Some(("fg", "make")))]
#[case::unmanaged(&["make"], "make", Some(&["make"][..]), "x", Some(("x", "make")))]
#[case::folder_named_like_a_slot(&["make"], "make", Some(&["make"][..]), "wt10", None)]
#[case::outside_the_pool(&["make"], "make", Some(&["make"][..]), "../elsewhere", None)]
#[case::no_pattern(&[], "cargo", Some(&["cargo", "build"][..]), "wt1", None)]
fn builds__should_match_heavy_patterns(
    #[case] heavy: &[&str],
    #[case] name: &str,
    #[case] cmd: Option<&[&str]>,
    #[case] cwd: &str,
    #[case] build: Option<(&str, &str)>,
) {
    // Given
    let sandbox_name = "builds-heavy";

    // When
    let document = status_with(sandbox_name, heavy, name, cmd, cwd);

    // Then
    let expected: Vec<(String, String)> = build
        .map(|(worktree, name)| vec![(worktree.to_owned(), name.to_owned())])
        .unwrap_or_default();
    assert_eq!(
        shown(&document),
        expected,
        "the builds of {name} {cmd:?} in {cwd} with the heavy patterns {heavy:?}"
    );
}

#[rstest]
#[case::npm_title(&["npm test"], "node", Some(&["npm test"][..]), Some("npm test"))]
#[case::npm_arguments(&["npm test"], "npm", Some(&["npm", "test"][..]), Some("npm test"))]
#[case::python_module(&["pytest"], "python3", Some(&["python3", "-m", "pytest", "tests/"][..]), Some("pytest"))]
#[case::node_script(&["vite build"], "node", Some(&["node", "x/vite.js", "build"][..]), Some("vite build"))]
#[case::node_option_first(&["vite build"], "node", Some(&["node", "--max-old-space-size=4096", "x/vite.js", "build"][..]), Some("vite build"))]
#[case::shell_script(&["build"], "bash", Some(&["bash", "./scripts/build.sh"][..]), Some("build"))]
#[case::interpreter_alone(&["node"], "node", None, Some("node"))]
#[case::interpreter_without_arguments(&["vite build"], "node", None, None)]
#[case::exe(&["cargo build"], "cargo.exe", Some(&["cargo.exe", "build"][..]), cfg!(windows).then_some("cargo build"))]
#[case::exe_in_capitals(&["cargo build"], "CARGO.EXE", Some(&["CARGO.EXE", "build"][..]), cfg!(windows).then_some("cargo build"))]
fn pattern__should_match_processes(
    #[case] heavy: &[&str],
    #[case] name: &str,
    #[case] cmd: Option<&[&str]>,
    #[case] build: Option<&str>,
) {
    // Given
    let sandbox_name = "patterns-processes";

    // When
    let document = status_with(sandbox_name, heavy, name, cmd, "wt1");

    // Then
    let expected: Vec<(String, String)> = build
        .map(|name| vec![("wt1".to_owned(), name.to_owned())])
        .unwrap_or_default();
    assert_eq!(
        shown(&document),
        expected,
        "the process {name} {cmd:?} with the heavy patterns {heavy:?}"
    );
}

#[test]
fn builds__should_say_when_a_job_holds_them() {
    // Given
    let mut hooks = Hooks::must_new("builds-queued");
    hooks.sandbox.write_machine(&heavy_only(&["cargo test"]));
    fs::write(hooks.pool.fg.join("Makefile"), "all:\n").expect("setup: write a Makefile");
    let (wt1, wt2, wt3) = (
        hooks.pool.worktree("wt1"),
        hooks.pool.worktree("wt2"),
        hooks.pool.worktree("wt3"),
    );
    let (job, record) = started(
        &hooks,
        &JobSpec::held("cargo", &["test"]).in_dir(&wt1),
        "running",
    );
    let record = must_have(&job, record, "cargo test");
    list_job(&mut hooks, &job, &record, 1, &wt1, &["cargo", "test"]);
    let at = "2026-10-20T11:00:00Z";
    hooks.start_proc(proc(5001, job.pid(), &["cargo", "test"], &wt1, at));
    hooks.start_proc(proc(5101, 1, &["cargo", "test"], &wt2, at));
    hooks.start_proc(proc(5201, 1, &["make"], &wt3, at));

    // When
    let document = hooks.status();

    // Then
    let found: Vec<BTreeSet<(String, u64, String)>> = ["wt1", "wt2", "wt3"]
        .iter()
        .map(|name| builds_of(&document, name))
        .collect();
    assert_eq!(
        found,
        vec![
            set(&[("cargo test", 5001, true)]),
            set(&[("cargo test", 5101, false)]),
            set(&[("make", 5201, true)]),
        ],
        "the builds of wt1 (in a job), wt2 (outside one) and wt3 (only a suggestion matches)"
    );
}

/// Each heavy pattern of `jobs` with its source.
fn patterns(jobs: &Value) -> BTreeSet<(String, String)> {
    jobs["heavy"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| (p["pattern"].to_string(), p["source"].to_string()))
        .collect()
}

fn pairs(list: &[(&str, &str)]) -> BTreeSet<(String, String)> {
    list.iter()
        .map(|(a, b)| (json!(a).to_string(), json!(b).to_string()))
        .collect()
}

#[test]
fn heavy__should_merge_machine_repository_and_build_processes() {
    // Given
    let sandbox = Sandbox::must_new("heavy-merge");
    let pool = sandbox.must_pool(&catalog_spec());
    sandbox.write_machine(&heavy_only(&["make"]));
    add_to_repository(
        &pool.common,
        "heavy = [\"cargo build\"]\nbuild_processes = [\"protoc\"]\n",
    );

    // When
    let listed = must_job_ls_json(&sandbox, &pool.fg, &[]);
    let status = must_status_json(&sandbox, &pool.fg, &[]);
    sandbox.remove_machine();
    let without_machine = must_job_ls_json(&sandbox, &pool.fg, &[]);
    let config = wtpool(&sandbox, &pool.fg, &["config"], &[]);

    // Then
    let all = pairs(&[
        ("make", "machine"),
        ("cargo build", "repository"),
        ("protoc", "build_processes"),
    ]);
    assert_eq!(
        patterns(&listed),
        all,
        "wtpool job ls --json heavy: {listed}"
    );
    assert_eq!(
        patterns(jobs_of(&status)),
        all,
        "machine.jobs.heavy of wtpool status: {}",
        jobs_of(&status)
    );
    assert_eq!(
        patterns(&without_machine),
        pairs(&[("cargo build", "repository"), ("protoc", "build_processes")]),
        "without machine.toml the machine adds no pattern: {without_machine}"
    );
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&config.stdout),
        String::from_utf8_lossy(&config.stderr)
    );
    assert!(
        config.status.success()
            && text
                .lines()
                .any(|line| line.contains("build_processes") && line.contains("heavy")),
        "wtpool config names no line of build_processes that says heavy replaces it: {text:?}"
    );
}

#[test]
fn builds__should_count_suggestions_until_a_list() {
    // Given
    let mut hooks = Hooks::must_new("builds-suggested");
    hooks
        .sandbox
        .write_machine("[jobs]\nenforce = \"require\"\n");
    fs::write(hooks.pool.fg.join("Makefile"), "all:\n").expect("setup: write a Makefile");
    let wt1 = hooks.pool.worktree("wt1");
    hooks.start_proc(proc(
        4300,
        1,
        &["make", "-j4"],
        &wt1,
        "2026-10-20T11:00:00Z",
    ));
    let make = call(
        &hooks,
        "Bash",
        &format!("cd {} && make", wt1.display()),
        &hooks.pool.fg,
    );

    // When
    let suggested = hooks.status();
    let refused = policy(&hooks, &make, &[]);
    add_to_repository(&hooks.pool.common, "heavy = [\"cargo build\"]\n");
    let listed = hooks.status();

    // Then
    let jobs = jobs_of(&suggested);
    assert_eq!(
        builds_of(&suggested, "wt1"),
        set(&[("make", 4300, true)]),
        "a make that only the Makefile's suggestion matches is a build nothing flags"
    );
    assert!(
        jobs["suggested"]
            .as_array()
            .is_some_and(|list| list.contains(&json!({"pattern": "make", "from": "Makefile"}))),
        "machine.jobs.suggested lacks make from Makefile: {jobs}"
    );
    assert_eq!(
        jobs["outside"],
        json!([]),
        "a build only a suggestion matches is listed outside a job"
    );
    assert_eq!(
        decision(&refused),
        Ok(None),
        "under require the policy judged a command only a suggestion matches"
    );
    assert_eq!(
        builds_of(&listed, "wt1"),
        BTreeSet::new(),
        "once the repository lists a pattern, its suggestions count for nothing"
    );
}

#[test]
fn policy__should_flag_only_under_warn_and_require() {
    // Given
    let mut hooks = Hooks::must_new("policy-flags");
    hooks.sandbox.write_machine(&machine("warn", &["make"]));
    let (wt1, wt2) = (hooks.pool.worktree("wt1"), hooks.pool.worktree("wt2"));
    let (job, record) = started(&hooks, &JobSpec::held("make", &[]).in_dir(&wt1), "running");
    let record = must_have(&job, record, "make");
    list_job(&mut hooks, &job, &record, 1, &wt1, &["make"]);
    let at = "2026-10-20T11:00:00Z";
    hooks.start_proc(proc(5001, job.pid(), &["make"], &wt1, at));
    hooks.start_proc(proc(5101, 1, &["make"], &wt2, at));
    let envs = hooks.base_envs();
    let envs: Vec<(&str, &OsStr)> = envs.iter().map(|(k, v)| (*k, v.as_os_str())).collect();

    // When
    let mut seen = Vec::new();
    for level in ["off", "warn", "require"] {
        hooks.sandbox.write_machine(&machine(level, &["make"]));
        let document = hooks.status();
        let listed = wtpool(&hooks.sandbox, &hooks.pool.fg, &["job", "ls"], &envs);
        seen.push((level, document, listed));
    }

    // Then
    let mut wrong = Vec::new();
    let outside = json!([{"summary": "make", "label": null, "worktree": wt2.to_string_lossy(), "actor": null}]);
    for (level, document, listed) in &seen {
        let jobs = jobs_of(document);
        if jobs["enforce"] != json!(level) {
            wrong.push(format!(
                "{level}: machine.jobs.enforce is {}",
                jobs["enforce"]
            ));
        }
        if builds_of(document, "wt1") != set(&[("make", 5001, true)])
            || builds_of(document, "wt2") != set(&[("make", 5101, false)])
        {
            wrong.push(format!(
                "{level}: builds {:?} and {:?}",
                builds_of(document, "wt1"),
                builds_of(document, "wt2")
            ));
        }
        let found: Vec<Value> = jobs["outside"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| json!({"summary": p["summary"], "label": p["label"], "worktree": p["worktree"], "actor": p["actor"]}))
            .collect();
        if Value::from(found) != outside {
            wrong.push(format!(
                "{level}: machine.jobs.outside is {}",
                jobs["outside"]
            ));
        }
        let text = String::from_utf8_lossy(&listed.stdout);
        let flagged: Vec<&str> = text
            .lines()
            .filter(|l| l.contains("(not queued)"))
            .collect();
        let wanted = usize::from(*level != "off");
        if !listed.status.success()
            || flagged.len() != wanted
            || !flagged.iter().all(|line| line.contains("make"))
        {
            wrong.push(format!(
                "{level}: wtpool job ls marks {} lines (not queued), not {wanted}: {text:?}",
                flagged.len()
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
