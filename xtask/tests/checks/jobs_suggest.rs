//! Suggested heavy patterns: the plugin's skill names none, the stack markers of a repository's
//! main worktree give the table's suggestions, reading them reads names only, and a
//! `package.json` gives its scripts.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use rstest::rstest;
use serde_json::Value;
use xtask::pool::git_in;

use super::status::spec;
use crate::support::jobs::settings::{must_lone_pool, snapshot, stderr_of};
use crate::support::jobs::{describe, must_job_ls_json, wtpool};
use crate::support::{Sandbox, must_repo, must_symlink};

/// Commands and program names of the suggestion table and the package managers. The skill may
/// name them only inside a `wtpool job` example. `make` is left out: it is an English word too.
const NAMED: [&str; 20] = [
    "cargo",
    "mvn",
    "mvnw",
    "gradle",
    "gradlew",
    "pytest",
    "tox",
    "bazel",
    "cmake",
    "ninja",
    "dotnet",
    "xcodebuild",
    "npm",
    "pnpm",
    "yarn",
    "bun",
    "go build",
    "go test",
    "swift build",
    "swift test",
];

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

/// True when the text before a pattern ends a `wtpool job` command: `wtpool job -- ` or
/// `wtpool job --label <label> -- `, the label one word or quoted.
fn wrapped(before: &str) -> bool {
    if before.ends_with("wtpool job -- ") {
        return true;
    }
    let Some((_, rest)) = before.rsplit_once("wtpool job --label ") else {
        return false;
    };
    let Some(label) = rest.strip_suffix(" -- ") else {
        return false;
    };
    let quoted = |q: char| {
        label.len() >= 2
            && label.starts_with(q)
            && label.ends_with(q)
            && !label
                .get(1..label.len() - 1)
                .unwrap_or_default()
                .contains(q)
    };
    quoted('"') || quoted('\'') || (!label.is_empty() && !label.contains(char::is_whitespace))
}

/// Each place of `text` where `name` stands as a word outside a `wtpool job` example.
fn unwrapped(text: &str, name: &str) -> Vec<String> {
    text.match_indices(name)
        .filter(|(at, _)| {
            let before = text.get(..*at).unwrap_or_default();
            let after = text.get(at + name.len()..).unwrap_or_default();
            !before.chars().next_back().is_some_and(is_word)
                && !after.chars().next().is_some_and(is_word)
                && !wrapped(before)
        })
        .map(|(at, _)| {
            let start = text
                .get(..at)
                .map_or(0, |b| b.rfind('\n').map_or(0, |n| n + 1));
            let end = text
                .get(at..)
                .and_then(|a| a.find('\n'))
                .map_or(text.len(), |n| at + n);
            text.get(start..end).unwrap_or_default().to_owned()
        })
        .collect()
}

#[test]
fn plugin__should_hold_the_heavy_jobs_skill() {
    // Given
    let path = must_repo().join("plugins/wtpool/skills/heavy-jobs/SKILL.md");

    // When
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the plugin holds no {}: {e}", path.display()));

    // Then
    let mut wrong = Vec::new();
    let front = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(front, _)| front);
    let field = |name: &str| {
        front.and_then(|f| {
            f.lines()
                .find_map(|line| line.strip_prefix(&format!("{name}: ")))
                .map(str::trim)
        })
    };
    if field("name") != Some("heavy-jobs") {
        wrong.push(format!(
            "the frontmatter does not name heavy-jobs: {front:?}"
        ));
    }
    if field("description").is_none_or(str::is_empty) {
        wrong.push(format!("the frontmatter has no description: {front:?}"));
    }
    if !text.contains("wtpool job ls") {
        wrong.push("the skill does not tell the agent to read wtpool job ls".to_owned());
    }
    for name in NAMED {
        for line in unwrapped(&text, name) {
            wrong.push(format!(
                "the skill names {name:?} outside a wtpool job example: {line:?}"
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Where the markers lie.
enum Place {
    /// The main worktree.
    Main,
    /// A slot alone, `wtpool job ls` run there.
    Slot,
    /// The foreground of a bare repository.
    Bare,
    /// The main worktree, the machine listing `cargo test` and the repository `cargo build`.
    Held,
}

/// A bare repository `app.git` in the pool folder with its foreground `fg`, the folder the
/// sandbox's `WTPOOL_FOREGROUND` names.
fn must_bare_pool(sandbox: &Sandbox) -> std::path::PathBuf {
    let dir = sandbox.pool_dir();
    let seed = sandbox.root.join("seed");
    let text = |p: &Path| p.to_string_lossy().into_owned();
    let git = |dir: &Path, args: &[&str]| {
        git_in(dir, args).unwrap_or_else(|e| panic!("setup: git {args:?}: {e}"));
    };
    git(
        &sandbox.root,
        &["init", "--quiet", "-b", "main", &text(&seed)],
    );
    fs::write(seed.join("f.txt"), "base\n").expect("setup: write a file");
    git(&seed, &["add", "f.txt"]);
    git(&seed, &["commit", "--quiet", "-m", "base"]);
    let bare = dir.join("app.git");
    git(
        &dir,
        &["clone", "--quiet", "--bare", &text(&seed), &text(&bare)],
    );
    git(&bare, &["remote", "remove", "origin"]);
    fs::remove_dir_all(&seed).expect("setup: remove the seed");
    let fg = dir.join("fg");
    git(&bare, &["worktree", "add", "--quiet", &text(&fg), "main"]);
    fs::write(
        bare.join("wtpool.toml"),
        "slot_dir = \".\"\nslot_prefix = \"wt\"\nslot_count = 3\n",
    )
    .expect("setup: write wtpool.toml");
    fg
}

/// Creates each entry of `entries` in `dir`: a trailing `/` makes a folder, a `/` inside a file
/// below a folder.
fn create(dir: &Path, entries: &[&str]) {
    for entry in entries {
        let path = dir.join(entry.trim_end_matches('/'));
        if entry.ends_with('/') {
            fs::create_dir_all(&path).expect("setup: create a folder");
        } else {
            fs::create_dir_all(path.parent().expect("setup: a parent")).expect("setup: mkdir");
            fs::write(&path, "").expect("setup: write a marker");
        }
    }
}

fn suggested(document: &Value) -> BTreeSet<(String, String)> {
    document["suggested"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| {
            let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
            (text(&item["pattern"]), text(&item["from"]))
        })
        .collect()
}

fn from(marker: &str, patterns: &[&str]) -> Vec<(String, String)> {
    patterns
        .iter()
        .map(|p| ((*p).to_owned(), marker.to_owned()))
        .collect()
}

const CARGO: [&str; 4] = ["cargo build", "cargo test", "cargo nextest", "cargo clippy"];

#[rstest]
#[case::cargo(Place::Main, &["Cargo.toml"], from("Cargo.toml", &CARGO))]
#[case::go(Place::Main, &["go.mod"], from("go.mod", &["go build", "go test"]))]
#[case::maven(Place::Main, &["pom.xml"], from("pom.xml", &["mvn"]))]
#[case::maven_wrapper(Place::Main, &["pom.xml", "mvnw"], from("pom.xml", &["mvnw"]))]
#[case::gradle(Place::Main, &["build.gradle"], from("build.gradle", &["gradle"]))]
#[case::gradle_kotlin(Place::Main, &["build.gradle.kts"], from("build.gradle.kts", &["gradle"]))]
#[case::gradle_wrapper(Place::Main, &["build.gradle.kts", "gradlew"], from("build.gradle.kts", &["gradlew"]))]
#[case::pyproject(Place::Main, &["pyproject.toml"], from("pyproject.toml", &["pytest"]))]
#[case::pytest_ini(Place::Main, &["pytest.ini"], from("pytest.ini", &["pytest"]))]
#[case::tox(Place::Main, &["tox.ini"], from("tox.ini", &["tox", "pytest"]))]
#[case::bazel_build(Place::Main, &["BUILD.bazel"], from("BUILD.bazel", &["bazel build", "bazel test"]))]
#[case::bazel_workspace(Place::Main, &["WORKSPACE"], from("WORKSPACE", &["bazel build", "bazel test"]))]
#[case::bazel_module(Place::Main, &["MODULE.bazel"], from("MODULE.bazel", &["bazel build", "bazel test"]))]
#[case::makefile(Place::Main, &["Makefile"], from("Makefile", &["make"]))]
#[case::lowercase_makefile(Place::Main, &["makefile"], from("makefile", &["make"]))]
#[case::gnu_makefile(Place::Main, &["GNUmakefile"], from("GNUmakefile", &["make"]))]
#[case::cmake(Place::Main, &["CMakeLists.txt"], from("CMakeLists.txt", &["cmake", "ninja"]))]
#[case::csproj(Place::Main, &["App.csproj"], from("App.csproj", &["dotnet build", "dotnet test"]))]
#[case::sln(Place::Main, &["App.sln"], from("App.sln", &["dotnet build", "dotnet test"]))]
#[case::swift(Place::Main, &["Package.swift"], from("Package.swift", &["swift build", "swift test"]))]
#[case::xcodeproj_folder(Place::Main, &["App.xcodeproj/"], from("App.xcodeproj", &["xcodebuild"]))]
#[case::two_markers(Place::Main, &["Cargo.toml", "Makefile"], [from("Cargo.toml", &CARGO), from("Makefile", &["make"])].concat())]
#[case::empty(Place::Main, &[], vec![])]
#[case::no_marker_at_the_top(Place::Main, &["Cargo.lock", "README.md", "docs/Cargo.toml", "Makefile.am"], vec![])]
#[case::a_slot_alone(Place::Slot, &["Cargo.toml", "Makefile"], vec![])]
#[case::bare_foreground(Place::Bare, &["Makefile"], from("Makefile", &["make"]))]
#[case::listed_ones_hidden(Place::Held, &["Cargo.toml"], from("Cargo.toml", &["cargo nextest", "cargo clippy"]))]
fn suggest__should_follow_the_marker_table(
    #[case] place: Place,
    #[case] entries: &[&str],
    #[case] expected: Vec<(String, String)>,
) {
    // Given
    let sandbox = Sandbox::must_new("suggest-table");
    sandbox.remove_machine();
    let dir = match place {
        Place::Main => must_lone_pool(&sandbox).fg,
        Place::Slot => sandbox.must_pool(&spec(&[])).worktree("wt1"),
        Place::Bare => must_bare_pool(&sandbox),
        Place::Held => {
            let pool = must_lone_pool(&sandbox);
            sandbox.write_machine("[jobs]\nheavy = [\"cargo test\"]\n");
            let config = pool.common.join("wtpool.toml");
            let mut text = fs::read_to_string(&config).expect("setup: read wtpool.toml");
            text.push_str("heavy = [\"cargo build\"]\n");
            fs::write(&config, text).expect("setup: write wtpool.toml");
            pool.fg
        }
    };
    create(&dir, entries);

    // When
    let document = must_job_ls_json(&sandbox, &dir, &[]);

    // Then
    let found = suggested(&document);
    let expected: BTreeSet<(String, String)> = expected.into_iter().collect();
    assert_eq!(found, expected, "suggestions for {entries:?}");
    assert!(
        !found.iter().any(|(pattern, _)| pattern == "cargo check"),
        "a marker suggested cargo check"
    );
}

#[test]
fn suggest__should_read_only_names() {
    // Given
    let sandbox = Sandbox::must_new("suggest-names");
    let pool = must_lone_pool(&sandbox);
    sandbox.remove_machine();
    create(
        &pool.fg,
        &["Makefile", "Cargo.toml", "go.mod", "pnpm-lock.yaml"],
    );
    fs::write(
        pool.fg.join("package.json"),
        r#"{"scripts": {"build": "make all", "test": "cargo test"}}"#,
    )
    .expect("setup: write package.json");
    for shim in ["npm", "pnpm", "yarn", "bun", "node", "make", "cargo", "go"] {
        sandbox.must_install_probe(shim);
    }
    let marks = sandbox.marks_dir();
    let mut git = sandbox.must_log_git();
    let skip = [marks.as_path(), &sandbox.root.join("git-starts.log")];
    let envs = [("WTPOOL_PROBE_MARKS", marks.as_os_str())];
    let crowded = Sandbox::must_new("suggest-crowded");
    let crowded_pool = must_lone_pool(&crowded);
    crowded.remove_machine();
    for n in 0..10_001 {
        fs::write(crowded_pool.fg.join(format!("e{n:05}")), "").expect("setup: write an entry");
    }
    create(&crowded_pool.fg, &["Makefile"]);

    // When
    let before = snapshot(&sandbox.root, &skip);
    let suggest = wtpool(&sandbox, &pool.fg, &["job", "heavy", "suggest"], &envs);
    let git_starts = git.starts().len();
    let listed = wtpool(&sandbox, &pool.fg, &["job", "ls"], &envs);
    let document = must_job_ls_json(&sandbox, &pool.fg, &envs);
    let after = snapshot(&sandbox.root, &skip);
    let started: Vec<String> = fs::read_dir(&marks)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let clock = Instant::now();
    let crowded_run = wtpool(
        &crowded,
        &crowded_pool.fg,
        &["job", "heavy", "suggest"],
        &[],
    );
    let crowded_time = clock.elapsed();
    let crowded_document = must_job_ls_json(&crowded, &crowded_pool.fg, &[]);

    // Then
    let mut wrong = Vec::new();
    for (what, output) in [("heavy suggest", &suggest), ("job ls", &listed)] {
        if !output.status.success() {
            wrong.push(format!("{what}: {}", describe(output)));
        }
    }
    if !suggested(&document).contains(&("make".to_owned(), "Makefile".to_owned())) {
        wrong.push(format!(
            "the markers gave no suggestion: {}",
            document["suggested"]
        ));
    }
    if git_starts != 0 {
        wrong.push(format!(
            "wtpool job heavy suggest started git {git_starts} times"
        ));
    }
    if !started.is_empty() {
        wrong.push(format!(
            "reading the suggestions started programs: {started:?}"
        ));
    }
    if before != after {
        let changed: Vec<&std::path::PathBuf> = after
            .keys()
            .chain(before.keys())
            .filter(|p| before.get(*p) != after.get(*p))
            .collect();
        wrong.push(format!("reading the suggestions wrote {changed:?}"));
    }
    if !crowded_run.status.success() || crowded_time > Duration::from_secs(5) {
        wrong.push(format!(
            "heavy suggest over 10 001 entries took {crowded_time:?}: {}",
            describe(&crowded_run)
        ));
    }
    let crowded_found = suggested(&crowded_document);
    if !crowded_found
        .iter()
        .all(|s| s == &("make".to_owned(), "Makefile".to_owned()))
    {
        wrong.push(format!("10 001 entries suggested {crowded_found:?}"));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The `package.json` of a case.
enum Package {
    Text(&'static str),
    /// Valid JSON with a `build` script, padded to this many bytes.
    Sized(usize),
    /// A symlink to a valid file with a `build` script.
    Symlink,
    /// Scripts `build-01` to `build-21`.
    TwentyOne,
}

fn padded(size: usize) -> String {
    let head = r#"{"scripts": {"build": "x"}, "padding": ""#;
    let tail = "\"}";
    format!(
        "{head}{}{tail}",
        "x".repeat(size.saturating_sub(head.len() + tail.len()))
    )
}

fn owned(patterns: &[&str]) -> Vec<String> {
    patterns.iter().map(|p| (*p).to_owned()).collect()
}

fn npm_build() -> Vec<String> {
    owned(&["npm run build"])
}

/// Script names outside the grammar of a heavy pattern, one of them with a right-to-left
/// override, and `test`.
const OUTSIDE_THE_GRAMMAR: &str = "{\"scripts\": {\"test:a b\": \"x\", \"build;rm\": \"x\", \"test/x\": \"x\", \"test:\\u202e\": \"x\", \"test:xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\": \"x\", \"build$(id)\": \"x\", \"test\": \"x\"}}";

#[rstest]
#[case::npm_by_default(Package::Text(r#"{"scripts": {"build": "x", "test": "x", "start": "x"}}"#), &[], owned(&["npm run build", "npm run test", "npm test"]), false)]
#[case::npm_over_a_lockfile(Package::Text(r#"{"packageManager": "npm@10.2.0", "scripts": {"test": "x"}}"#), &["yarn.lock"], owned(&["npm run test", "npm test"]), false)]
#[case::pnpm_manager(Package::Text(r#"{"packageManager": "pnpm@9.1.0", "scripts": {"build": "x", "test": "x"}}"#), &[], owned(&["pnpm run build", "pnpm build", "pnpm run test", "pnpm test"]), false)]
#[case::yarn_manager(Package::Text(r#"{"packageManager": "yarn@4.1.0", "scripts": {"build": "x"}}"#), &[], owned(&["yarn run build", "yarn build"]), false)]
#[case::bun_manager(Package::Text(r#"{"packageManager": "bun@1.1.0", "scripts": {"build": "x"}}"#), &[], owned(&["bun run build"]), false)]
#[case::unknown_manager(Package::Text(r#"{"packageManager": "deno@1.0.0", "scripts": {"build": "x"}}"#), &[], npm_build(), false)]
#[case::unknown_manager_and_a_lockfile(Package::Text(r#"{"packageManager": "deno@1.0.0", "scripts": {"build": "x"}}"#), &["pnpm-lock.yaml"], owned(&["pnpm run build", "pnpm build"]), false)]
#[case::pnpm_lockfile(Package::Text(r#"{"scripts": {"build": "x"}}"#), &["pnpm-lock.yaml"], owned(&["pnpm run build", "pnpm build"]), false)]
#[case::yarn_lockfile(Package::Text(r#"{"scripts": {"build": "x"}}"#), &["yarn.lock"], owned(&["yarn run build", "yarn build"]), false)]
#[case::bun_lock(Package::Text(r#"{"scripts": {"build": "x"}}"#), &["bun.lock"], owned(&["bun run build"]), false)]
#[case::bun_lockb(Package::Text(r#"{"scripts": {"build": "x"}}"#), &["bun.lockb"], owned(&["bun run build"]), false)]
#[case::first_lockfile(Package::Text(r#"{"scripts": {"build": "x"}}"#), &["yarn.lock", "pnpm-lock.yaml"], owned(&["pnpm run build", "pnpm build"]), false)]
#[case::the_name_rule(Package::Text(r#"{"scripts": {"build": "x", "Build": "x", "test": "x", "compile": "x", "bundle": "x", "e2e": "x", "test:unit": "x", "test:e2e": "x", "build-prod": "x", "e2e_ci": "x", "start": "x", "testing": "x", "prebuild": "x", "lint": "x", "buildx": "x"}}"#), &[], owned(&["npm run build", "npm run Build", "npm run test", "npm test", "npm run compile", "npm run bundle", "npm run e2e", "npm run test:unit", "npm run test:e2e", "npm run build-prod", "npm run e2e_ci"]), false)]
#[case::names_outside_the_grammar(Package::Text(OUTSIDE_THE_GRAMMAR), &[], owned(&["npm run test", "npm test"]), false)]
#[case::twenty_one_scripts(Package::TwentyOne, &[], (1..=20).map(|n| format!("npm run build-{n:02}")).collect(), false)]
#[case::at_256_kib(Package::Sized(256 * 1024), &[], npm_build(), false)]
#[case::over_256_kib(Package::Sized(257 * 1024), &[], vec![], true)]
#[case::a_symlink(Package::Symlink, &[], vec![], true)]
#[case::not_json(Package::Text(r#"{"scripts": "#), &[], vec![], true)]
#[case::commands_never_read(Package::Text(r#"{"scripts": {"lint": "make all && cargo build", "start": "cmake ."}}"#), &[], vec![], false)]
fn suggest__should_read_package_json_scripts(
    #[case] package: Package,
    #[case] files: &[&str],
    #[case] expected: Vec<String>,
    #[case] refused: bool,
) {
    // Given
    let sandbox = Sandbox::must_new("suggest-package");
    let pool = must_lone_pool(&sandbox);
    sandbox.remove_machine();
    let path = pool.fg.join("package.json");
    match package {
        Package::Text(text) => fs::write(&path, text).expect("setup: write package.json"),
        Package::Sized(size) => fs::write(&path, padded(size)).expect("setup: write package.json"),
        Package::Symlink => {
            let real = sandbox.root.join("real-package.json");
            fs::write(&real, r#"{"scripts": {"build": "x"}}"#).expect("setup: write a file");
            must_symlink(&real, &path);
        }
        Package::TwentyOne => {
            let scripts: Vec<String> = (1..=21)
                .map(|n| format!("\"build-{n:02}\": \"x\""))
                .collect();
            fs::write(
                &path,
                format!("{{\"scripts\": {{{}}}}}", scripts.join(", ")),
            )
            .expect("setup: write package.json");
        }
    }
    create(&pool.fg, files);

    // When
    let document = must_job_ls_json(&sandbox, &pool.fg, &[]);
    let suggest = wtpool(&sandbox, &pool.fg, &["job", "heavy", "suggest"], &[]);

    // Then
    let found = suggested(&document);
    let expected: BTreeSet<(String, String)> = expected
        .iter()
        .map(|p| (p.clone(), "package.json".to_owned()))
        .collect();
    assert_eq!(found, expected, "the suggestions of package.json");
    if refused {
        assert!(
            suggest.status.success() && stderr_of(&suggest).contains("package.json"),
            "heavy suggest does not say on stderr why package.json gave nothing: {}",
            describe(&suggest)
        );
    }
}
