//! The settings commands of heavy jobs: `wtpool job limit`, `enforce` and `heavy` print a setting
//! with its source, set it or reset it, and refuse bad values. [`super::jobs_settings_files`]
//! checks the files they write.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use rstest::rstest;

use crate::support::Sandbox;
use crate::support::jobs::describe;
use crate::support::jobs::settings::{
    file, job, jobs_table, must_lone_pool, refused, snapshot, stdout_of, strings, succeeded, table,
    tokens,
};

fn set_of(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

/// True when a line of `text` holds `value`, as a word when it is one, and the source word
/// `source`.
fn printed_with(text: &str, value: &str, source: &str) -> bool {
    text.lines().any(|line| {
        let words = tokens(line);
        let holds = if value.contains(' ') {
            line.contains(value)
        } else {
            words.contains(value)
        };
        holds && words.contains(source)
    })
}

enum Setting {
    Limit,
    Enforce,
    MachineHeavy,
    RepositoryHeavy,
    Suggested,
}

fn limit_and_enforce(
    sandbox: &Sandbox,
    cwd: &Path,
    key: &str,
    (default, set): (&str, &str),
    bad: &[&str],
    wrong: &mut Vec<String>,
) {
    let path = sandbox.machine_config();
    let shown = job(sandbox, cwd, &[key]);
    if succeeded(wrong, &format!("wtpool job {key}"), &shown)
        && !printed_with(&stdout_of(&shown), default, "default")
    {
        wrong.push(format!(
            "wtpool job {key} without machine.toml does not print {default} from default: {}",
            describe(&shown)
        ));
    }
    let changed = job(sandbox, cwd, &[key, set]);
    if succeeded(wrong, &format!("wtpool job {key} {set}"), &changed)
        && !tokens(&stdout_of(&changed)).contains(set)
    {
        wrong.push(format!(
            "wtpool job {key} {set} does not print the new value: {}",
            describe(&changed)
        ));
    }
    let stored = jobs_table(&path).and_then(|t| t.get(key).cloned());
    let wanted = set
        .parse::<i64>()
        .map_or_else(|_| toml::Value::from(set), toml::Value::from);
    if stored.as_ref() != Some(&wanted) {
        wrong.push(format!(
            "after {key} {set}, machine.toml holds {key} {stored:?}"
        ));
    }
    let again = job(sandbox, cwd, &[key]);
    if !printed_with(&stdout_of(&again), set, "machine") {
        wrong.push(format!(
            "wtpool job {key} does not print {set} from machine: {}",
            describe(&again)
        ));
    }
    for value in bad {
        let before = file(&path);
        let output = job(sandbox, cwd, &[key, value]);
        refused(
            wrong,
            &format!("wtpool job {key} {value:?}"),
            &output,
            &path,
            &before,
        );
    }
    let reset = job(sandbox, cwd, &[key, "default"]);
    if succeeded(wrong, &format!("wtpool job {key} default"), &reset)
        && !tokens(&stdout_of(&reset)).contains(default)
    {
        wrong.push(format!(
            "wtpool job {key} default does not print {default}: {}",
            describe(&reset)
        ));
    }
    if jobs_table(&path).is_some_and(|t| t.contains_key(key)) {
        wrong.push(format!(
            "wtpool job {key} default left the key in machine.toml"
        ));
    }
}

fn machine_heavy(sandbox: &Sandbox, cwd: &Path, wrong: &mut Vec<String>) {
    let path = sandbox.machine_config();
    let heavy = || strings(jobs_table(&path).as_ref(), "heavy");
    let added = job(sandbox, cwd, &["heavy", "add", "--", "make"]);
    succeeded(wrong, "heavy add -- make", &added);
    if heavy() != set_of(&["make"]) {
        wrong.push(format!("after heavy add -- make, heavy is {:?}", heavy()));
    }
    let listed = job(sandbox, cwd, &["heavy"]);
    if !printed_with(&stdout_of(&listed), "make", "machine") {
        wrong.push(format!(
            "wtpool job heavy does not print make from machine: {}",
            describe(&listed)
        ));
    }
    for (what, args) in [
        ("a pattern added twice", vec!["heavy", "add", "--", "make"]),
        (
            "an unlisted pattern removed",
            vec!["heavy", "rm", "--", "just"],
        ),
        (
            "a bad pattern among good ones",
            vec!["heavy", "add", "--", "ninja", "make *"],
        ),
    ] {
        let before = file(&path);
        let output = job(sandbox, cwd, &args);
        refused(wrong, what, &output, &path, &before);
    }
    let two = job(sandbox, cwd, &["heavy", "add", "--", "cargo build", "just"]);
    succeeded(wrong, "heavy add of two patterns", &two);
    if heavy() != set_of(&["make", "cargo build", "just"]) {
        wrong.push(format!("after adding two patterns, heavy is {:?}", heavy()));
    }
    let removed = job(sandbox, cwd, &["heavy", "rm", "--", "make"]);
    succeeded(wrong, "heavy rm -- make", &removed);
    if heavy() != set_of(&["cargo build", "just"]) {
        wrong.push(format!("after heavy rm -- make, heavy is {:?}", heavy()));
    }
}

fn repository_heavy(sandbox: &Sandbox, cwd: &Path, config: &Path, wrong: &mut Vec<String>) {
    let machine = sandbox.machine_config();
    let machine_before = file(&machine);
    let mut text = fs::read_to_string(config).expect("setup: read wtpool.toml");
    text.push_str("build_processes = [\"protoc\"]\n");
    fs::write(config, text).expect("setup: write wtpool.toml");
    let read = |key: &str| strings(table(config).as_ref(), key);
    let added = job(
        sandbox,
        cwd,
        &["heavy", "add", "--repo", "--", "cargo xtask verify"],
    );
    succeeded(wrong, "heavy add --repo", &added);
    if read("heavy") != set_of(&["cargo xtask verify"])
        || read("build_processes") != set_of(&["protoc"])
    {
        wrong.push(format!(
            "after heavy add --repo, heavy {:?} and build_processes {:?}",
            read("heavy"),
            read("build_processes")
        ));
    }
    let listed = job(sandbox, cwd, &["heavy"]);
    let out = stdout_of(&listed);
    if !printed_with(&out, "cargo xtask verify", "repository")
        || !printed_with(&out, "protoc", "build_processes")
    {
        wrong.push(format!(
            "wtpool job heavy does not print each repository pattern with its source: {}",
            describe(&listed)
        ));
    }
    let before = file(config);
    let twice = job(
        sandbox,
        cwd,
        &["heavy", "add", "--repo", "--", "cargo xtask verify"],
    );
    refused(
        wrong,
        "a repository pattern added twice",
        &twice,
        config,
        &before,
    );
    let removed = job(sandbox, cwd, &["heavy", "rm", "--repo", "--", "protoc"]);
    succeeded(wrong, "heavy rm --repo -- protoc", &removed);
    if read("build_processes").contains("protoc") {
        wrong.push("heavy rm --repo -- protoc left protoc in build_processes".to_owned());
    }
    let before = file(config);
    let gone = job(sandbox, cwd, &["heavy", "rm", "--repo", "--", "protoc"]);
    refused(
        wrong,
        "a removed build_processes name removed again",
        &gone,
        config,
        &before,
    );
    let last = job(
        sandbox,
        cwd,
        &["heavy", "rm", "--repo", "--", "cargo xtask verify"],
    );
    succeeded(wrong, "heavy rm --repo of a heavy pattern", &last);
    if read("heavy").contains("cargo xtask verify") {
        wrong.push("heavy rm --repo left cargo xtask verify".to_owned());
    }
    if file(&machine) != machine_before {
        wrong.push("a --repo edit changed machine.toml".to_owned());
    }
}

fn suggested(sandbox: &Sandbox, cwd: &Path, config: &Path, wrong: &mut Vec<String>) {
    for marker in ["Makefile", "Cargo.toml"] {
        fs::write(cwd.join(marker), "").expect("setup: write a marker");
    }
    sandbox.write_machine("[jobs]\nheavy = [\"cargo test\"]\n");
    let before = snapshot(&sandbox.root, &[]);
    let shown = job(sandbox, cwd, &["heavy", "suggest"]);
    let after = snapshot(&sandbox.root, &[]);
    let out = stdout_of(&shown);
    succeeded(wrong, "heavy suggest", &shown);
    for (pattern, marker) in [
        ("make", "Makefile"),
        ("cargo build", "Cargo.toml"),
        ("cargo nextest", "Cargo.toml"),
        ("cargo clippy", "Cargo.toml"),
    ] {
        if !out
            .lines()
            .any(|line| line.contains(pattern) && line.contains(marker))
        {
            wrong.push(format!(
                "heavy suggest does not print {pattern} from {marker}: {out:?}"
            ));
        }
    }
    if out.contains("cargo test") {
        wrong.push(format!(
            "heavy suggest prints cargo test, which the machine lists: {out:?}"
        ));
    }
    if before != after {
        wrong.push("heavy suggest wrote to the sandbox".to_owned());
    }
    let added = job(sandbox, cwd, &["heavy", "add", "--repo", "--suggested"]);
    succeeded(wrong, "heavy add --repo --suggested", &added);
    let heavy = strings(table(config).as_ref(), "heavy");
    let expected = set_of(&["make", "cargo build", "cargo nextest", "cargo clippy"]);
    if heavy != expected {
        wrong.push(format!(
            "after add --repo --suggested, the repository lists {heavy:?}, not {expected:?}"
        ));
    }
}

#[rstest]
#[case::limit(Setting::Limit)]
#[case::enforce(Setting::Enforce)]
#[case::machine_heavy(Setting::MachineHeavy)]
#[case::repository_heavy(Setting::RepositoryHeavy)]
#[case::suggested(Setting::Suggested)]
fn job_settings__should_print_set_and_reset(#[case] setting: Setting) {
    // Given
    let sandbox = Sandbox::must_new("job-settings");
    let pool = must_lone_pool(&sandbox);
    sandbox.remove_machine();
    let config = pool.common.join("wtpool.toml");
    let mut wrong = Vec::new();

    // When
    match setting {
        Setting::Limit => limit_and_enforce(
            &sandbox,
            &pool.fg,
            "limit",
            ("4", "6"),
            &["0", "65", "2.5", "four", "-1", ""],
            &mut wrong,
        ),
        Setting::Enforce => limit_and_enforce(
            &sandbox,
            &pool.fg,
            "enforce",
            ("warn", "require"),
            &["strict", "WARN", "on", ""],
            &mut wrong,
        ),
        Setting::MachineHeavy => machine_heavy(&sandbox, &pool.fg, &mut wrong),
        Setting::RepositoryHeavy => repository_heavy(&sandbox, &pool.fg, &config, &mut wrong),
        Setting::Suggested => suggested(&sandbox, &pool.fg, &config, &mut wrong),
    }

    // Then
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
