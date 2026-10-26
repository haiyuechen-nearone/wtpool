//! `wtpool job ls` (J2): the rules first, then the running jobs, the waiting ones in queue order
//! and the heavy processes outside any job; `--json` as the object of S14 with folders. And the
//! grammar of a heavy pattern (J18), through `wtpool job heavy add` and `machine.toml`.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use rstest::rstest;
use serde_json::{Value, json};

use super::jobs_config::{jobs_table, must_lone_pool, same_path, stdout_of, wait_state};
use crate::support::jobs::{
    JobSpec, ProcLine, SETTLE, describe, must_job_ls_json, schema_file_errors, wtpool,
};
use crate::support::{Sandbox, must_repo};

/// The alphanumeric words of `line`.
fn words(line: &str) -> BTreeSet<&str> {
    line.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect()
}

/// Whether `line` holds an age: digits, an optional space, then `s`, `m`, `h` or `d` alone.
fn has_age(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let unit_at = |at: usize| {
        chars
            .get(at)
            .is_some_and(|c| matches!(c, 's' | 'm' | 'h' | 'd'))
            && chars.get(at + 1).is_none_or(|c| !c.is_alphanumeric())
    };
    chars.iter().enumerate().any(|(at, c)| {
        c.is_ascii_digit()
            && chars.get(at + 1).is_none_or(|n| !n.is_ascii_digit())
            && (unit_at(at + 1) || (chars.get(at + 1) == Some(&' ') && unit_at(at + 2)))
    })
}

/// The line index of the first line of `lines` that names `folder`.
fn line_of(lines: &[&str], folder: &Path) -> Option<usize> {
    let shown = folder.to_string_lossy();
    let resolved = fs::canonicalize(folder).ok();
    lines.iter().position(|line| {
        line.contains(&*shown)
            || resolved
                .as_ref()
                .is_some_and(|r| line.contains(&*r.to_string_lossy()))
    })
}

/// `schema` with every object that lists properties closed.
fn closed(schema: &Value) -> Value {
    match schema {
        Value::Array(items) => Value::Array(items.iter().map(closed).collect()),
        Value::Object(map) => {
            let mut out: serde_json::Map<String, Value> =
                map.iter().map(|(k, v)| (k.clone(), closed(v))).collect();
            if out.contains_key("properties") {
                out.insert("additionalProperties".to_owned(), json!(false));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn closed_errors(document: &Value) -> Vec<String> {
    let path = must_repo().join("verify/schema/job-ls.schema.json");
    let text = fs::read_to_string(&path).expect("setup: read the schema");
    let schema: Value = serde_json::from_str(&text).expect("setup: parse the schema");
    let validator = jsonschema::validator_for(&closed(&schema)).expect("setup: compile");
    validator
        .iter_errors(document)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect()
}

/// The (pattern, source or from) pairs of a list of `machine.jobs`.
fn pairs(list: &Value, second: &str) -> BTreeSet<(String, String)> {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|item| {
            let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
            (text(&item["pattern"]), text(&item[second]))
        })
        .collect()
}

fn owned(items: &[(&str, &str)]) -> BTreeSet<(String, String)> {
    items
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

#[test]
fn job_ls__should_state_the_rules_then_list_jobs() {
    // Given
    let sandbox = Sandbox::must_new("job-ls");
    let pool = must_lone_pool(&sandbox);
    sandbox.write_machine("[jobs]\nlimit = 1\nenforce = \"warn\"\nheavy = [\"cargo test\"]\n");
    let config = pool.common.join("wtpool.toml");
    let mut text = fs::read_to_string(&config).expect("setup: read wtpool.toml");
    text.push_str("heavy = [\"make\"]\nbuild_processes = [\"protoc\"]\n");
    fs::write(&config, text).expect("setup: write wtpool.toml");
    fs::write(pool.fg.join("Cargo.toml"), "").expect("setup: write a marker");
    let [a, b, c, d] = ["a", "b", "c", "d"].map(|n| pool.fg.join(n));
    for folder in [&a, &b, &c, &d] {
        fs::create_dir_all(folder).expect("setup: create a folder");
    }
    let (a, b, c, d) = (a.as_path(), b.as_path(), c.as_path(), d.as_path());
    let procs = sandbox.must_write_proc_lines(&[ProcLine {
        pid: 4400,
        ppid: 1,
        name: "make",
        cmd: Some(&["make", "all"]),
        cwd: d,
    }]);
    let envs = [("WTPOOL_VERIFY_PROCS", procs.as_os_str())];
    let mut running = sandbox.must_start_job(&JobSpec::held("cargo", &["test"]).in_dir(a));
    let first = wait_state(&sandbox, &mut running, "running");
    let mut labelled =
        sandbox.must_start_job(&JobSpec::held("make", &[]).in_dir(b).labelled("nightly"));
    let second = wait_state(&sandbox, &mut labelled, "waiting");
    let mut plain = sandbox.must_start_job(&JobSpec::held("make", &["-j2"]).in_dir(c));
    let third = wait_state(&sandbox, &mut plain, "waiting");

    // When
    let listed = wtpool(&sandbox, &pool.fg, &["job", "ls"], &envs);
    let document = must_job_ls_json(&sandbox, &pool.fg, &envs);
    fs::remove_file(pool.fg.join("Cargo.toml")).expect("setup: remove the marker");
    let without = wtpool(&sandbox, &pool.fg, &["job", "ls"], &envs);
    for job in [&mut running, &mut labelled, &mut plain] {
        job.finish(SETTLE);
    }

    // Then
    let mut wrong = Vec::new();
    if first.is_none() || second.is_none() || third.is_none() {
        wrong.push(format!(
            "the jobs did not reach running, waiting, waiting under limit 1: {first:?} {second:?} {third:?}"
        ));
    }
    let out = stdout_of(&listed);
    let lines: Vec<&str> = out.lines().collect();
    let wrapper = lines
        .iter()
        .position(|l| l.trim() == "run heavy commands as: wtpool job -- <command>");
    let spots = [a, b, c, d].map(|folder| line_of(&lines, folder));
    let first_job = spots.iter().flatten().min().copied().unwrap_or(lines.len());
    let rules = lines.get(..first_job).unwrap_or_default();
    let ruled = |test: &dyn Fn(&str) -> bool| rules.iter().any(|line| test(line));
    if !listed.status.success() || wrapper.is_none_or(|at| at >= first_job) {
        wrong.push(format!(
            "no wrapper line before the jobs: {}",
            describe(&listed)
        ));
    }
    if !ruled(&|l| words(l).contains("limit") && words(l).contains("1"))
        || !ruled(&|l| words(l).contains("warn"))
    {
        wrong.push(format!(
            "the limit 1 and the policy warn are not stated first: {out:?}"
        ));
    }
    for (pattern, source) in [
        ("cargo test", "machine"),
        ("make", "repository"),
        ("protoc", "build_processes"),
    ] {
        if !ruled(&|l| l.contains(pattern) && words(l).contains(source)) {
            wrong.push(format!(
                "{pattern} with its source {source} is not stated first: {out:?}"
            ));
        }
    }
    let hint = "add them with: wtpool job heavy add --repo --suggested";
    if !ruled(&|l| l.contains(hint))
        || !["cargo build", "cargo nextest", "cargo clippy"]
            .iter()
            .all(|s| ruled(&|l| l.contains(s)))
    {
        wrong.push(format!(
            "the suggestions and {hint:?} are not stated first: {out:?}"
        ));
    }
    if stdout_of(&without).contains(hint) {
        wrong.push(format!(
            "{hint:?} without any suggestion: {}",
            describe(&without)
        ));
    }
    let order: Vec<Option<usize>> = spots.to_vec();
    let sorted = order.iter().all(Option::is_some)
        && order.windows(2).all(|pair| matches!(pair, [x, y] if x < y));
    if !sorted {
        wrong.push(format!("the running, the two waiting and the outside lines are not in order a, b, c, d: {order:?} in {out:?}"));
    }
    for ((folder, what), spot) in [(a, "cargo test"), (b, "nightly"), (c, "make"), (d, "make")]
        .iter()
        .zip(&spots)
    {
        let line = spot
            .and_then(|at| lines.get(at))
            .copied()
            .unwrap_or_default();
        if !line.contains(what) || !has_age(line) {
            wrong.push(format!(
                "the line of {} lacks {what:?} or an age: {line:?}",
                folder.display()
            ));
        }
    }
    let mut problems = schema_file_errors("job-ls.schema.json", &document);
    problems.extend(
        closed_errors(&document)
            .into_iter()
            .map(|p| format!("closed: {p}")),
    );
    if !problems.is_empty() {
        wrong.push(format!(
            "wtpool job ls --json against verify/schema/job-ls.schema.json: {problems:?}"
        ));
    }
    let settings = (
        &document["limit"],
        &document["enforce"],
        &document["source"],
    );
    if settings
        != (
            &json!(1),
            &json!("warn"),
            &json!({"limit": "machine", "enforce": "machine"}),
        )
    {
        wrong.push(format!("--json settings: {settings:?}"));
    }
    let heavy = pairs(&document["heavy"], "source");
    let want = owned(&[
        ("cargo test", "machine"),
        ("make", "repository"),
        ("protoc", "build_processes"),
    ]);
    if heavy != want {
        wrong.push(format!("--json heavy {heavy:?}, not {want:?}"));
    }
    let suggested = pairs(&document["suggested"], "from");
    let want = owned(&[
        ("cargo build", "Cargo.toml"),
        ("cargo nextest", "Cargo.toml"),
        ("cargo clippy", "Cargo.toml"),
    ]);
    if suggested != want {
        wrong.push(format!("--json suggested {suggested:?}, not {want:?}"));
    }
    let entry =
        |list: &str, at: usize, id: Option<&Value>, summary: &str, label: Value, folder: &Path| {
            let item = &document[list][at];
            let ok = id.is_none_or(|id| item["id"] == id["id"])
                && item["id"].is_string()
                && item["summary"] == json!(summary)
                && item["label"] == label
                && same_path(item["folder"].as_str().unwrap_or_default(), folder);
            (!ok).then(|| {
                format!(
                    "--json {list}[{at}] is {item}, not {summary} {label} in {}",
                    folder.display()
                )
            })
        };
    let lengths =
        ["running", "waiting", "outside"].map(|l| document[l].as_array().map_or(0, Vec::len));
    if lengths != [1, 2, 1] {
        wrong.push(format!(
            "--json lists {lengths:?} running, waiting and outside, not [1, 2, 1]: {document}"
        ));
    }
    wrong.extend(entry(
        "running",
        0,
        first.as_ref(),
        "cargo test",
        Value::Null,
        a,
    ));
    wrong.extend(entry(
        "waiting",
        0,
        second.as_ref(),
        "make",
        json!("nightly"),
        b,
    ));
    wrong.extend(entry("waiting", 1, third.as_ref(), "make", Value::Null, c));
    wrong.extend(entry("outside", 0, None, "make", Value::Null, d));
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[rstest]
#[case::make("make", true)]
#[case::cargo_build("cargo build", true)]
#[case::cargo_xtask_verify("cargo xtask verify", true)]
#[case::cargo_star("cargo-*", true)]
#[case::npm_run_build("npm run build", true)]
#[case::pnpm_run_test_e2e("pnpm run test:e2e", true)]
#[case::word_of_32(&"a".repeat(32), true)]
#[case::every_character("A.b_c+d-e9", true)]
#[case::colons_in_a_later_word("go test:unit:fast", true)]
#[case::star_alone("*", false)]
#[case::star_as_a_later_word("make *", false)]
#[case::an_option("-rf", false)]
#[case::two_spaces("cargo  build", false)]
#[case::word_of_33(&"a".repeat(33), false)]
#[case::later_word_with_a_dash("cargo -x", false)]
#[case::later_word_with_a_plus("cargo +nightly", false)]
#[case::star_on_a_later_word("cargo build*", false)]
#[case::star_inside("ma*ke", false)]
#[case::star_first("*cargo", false)]
#[case::colon_in_the_first_word("a:b", false)]
#[case::first_word_with_a_plus("+make", false)]
#[case::tab("cargo\tbuild", false)]
#[case::unicode("cärgo", false)]
#[case::a_slash("make/x", false)]
#[case::four_words("a b c d", false)]
#[case::empty("", false)]
#[case::leading_space(" make", false)]
#[case::trailing_space("make ", false)]
fn pattern__should_follow_the_grammar(#[case] pattern: &str, #[case] valid: bool) {
    // Given
    let sandbox = Sandbox::must_new("pattern-grammar");
    let pool = must_lone_pool(&sandbox);
    let path = sandbox.machine_config();
    sandbox.write_machine("[jobs]\n");
    let before = fs::read(&path).expect("setup: read machine.toml");

    // When
    let added = wtpool(
        &sandbox,
        &pool.fg,
        &["job", "heavy", "add", "--", pattern],
        &[],
    );
    let after = fs::read(&path).unwrap_or_default();
    let listed: Vec<String> = jobs_table(&path)
        .and_then(|t| t.get("heavy").cloned())
        .and_then(|v| v.as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    sandbox.write_machine(&format!(
        "[jobs]\nheavy = [{}]\n",
        toml::Value::from(pattern)
    ));
    let spec = JobSpec {
        program: "probe-tool".to_owned(),
        cwd: Some(pool.fg.clone()),
        ..JobSpec::default()
    };
    let mut job = sandbox.must_start_job(&spec);
    let code = job.wait_exit(SETTLE).and_then(|s| s.code());
    let ran = job.start_mark().is_some();

    // Then
    if valid {
        assert!(
            added.status.success() && listed == [pattern],
            "{pattern:?} is a pattern: heavy add gave {}, heavy {listed:?}",
            describe(&added)
        );
        assert!(
            code == Some(0) && ran,
            "with {pattern:?} in machine.toml, wtpool job exited {code:?}, ran {ran}: {:?}",
            job.stderr()
        );
    } else {
        assert!(
            added.status.code() == Some(125) && after == before,
            "{pattern:?} is no pattern: heavy add gave {} and machine.toml {}",
            describe(&added),
            if after == before {
                "is unchanged"
            } else {
                "changed"
            }
        );
        assert!(
            code == Some(125) && !ran,
            "with {pattern:?} in machine.toml, wtpool job exited {code:?}, ran {ran}: {:?}",
            job.stderr()
        );
    }
}
