//! The machine config of heavy jobs: where `machine.toml` lives, that no repository or variable
//! sets the limit or the policy, the refusal of an invalid file, the default limit and a symlinked
//! file, and the same machine config in the int and JS sandboxes.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::Output;

use rstest::rstest;
use serde_json::{Value, json};

use crate::support::jobs::settings::{
    must_lone_pool, names_path, same_path, set_mode, stderr_of, stdout_of, tokens,
};
use crate::support::jobs::{
    EARLIER_BUILDS, JobSpec, SETTLE, describe, must_job_ls_json, wait_state, wtpool,
};
use crate::support::{Sandbox, must_symlink, must_wtpool_bin};

fn cores(count: u32) -> String {
    count.to_string()
}

#[test]
fn machine_config__should_follow_xdg_and_home() {
    // Given
    let sandbox = Sandbox::must_new("machine-xdg");
    let pool = must_lone_pool(&sandbox);
    sandbox.write_machine("[jobs]\nlimit = 2\n");
    let home_config = sandbox.root.join("home/.config/wtpool/machine.toml");
    fs::create_dir_all(home_config.parent().expect("setup: a parent"))
        .expect("setup: create ~/.config/wtpool");
    fs::write(&home_config, "[jobs]\nlimit = 3\n").expect("setup: write ~/.config machine.toml");
    let home_state = sandbox.root.join("home/.local/state/wtpool/jobs");
    let ls = |unset: &[&str], empty: &[&str]| -> Value {
        let mut command = sandbox.command(&must_wtpool_bin(), &pool.fg);
        command.args(["job", "ls", "--json"]);
        for name in unset {
            command.env_remove(name);
        }
        for name in empty {
            command.env(name, "");
        }
        let output = command.output().expect("setup: start wtpool");
        assert!(
            output.status.success(),
            "wtpool job ls --json without {unset:?}, empty {empty:?}: {}",
            describe(&output)
        );
        serde_json::from_str(stdout_of(&output).trim_end())
            .unwrap_or_else(|e| panic!("wtpool job ls --json printed no JSON ({e}): {output:?}"))
    };

    // When
    let with_xdg = ls(&[], &[]);
    let without_xdg = ls(&["XDG_CONFIG_HOME"], &[]);
    let empty_xdg = ls(&[], &["XDG_CONFIG_HOME"]);
    let mut state_set = sandbox.must_start_job(&JobSpec::held("probe-tool", &[]).in_dir(&pool.fg));
    let set_record = wait_state(&sandbox, &mut state_set, "running");
    state_set.finish(SETTLE);
    let mut unset_spec = JobSpec::held("probe-tool", &[]).in_dir(&pool.fg);
    unset_spec.remove.push("XDG_STATE_HOME".to_owned());
    let mut state_unset = sandbox.must_start_job(&unset_spec);
    let pid = state_unset.pid();
    let home_record = crate::support::jobs::wait_until(SETTLE, || {
        if state_unset.try_status().is_some() {
            return Some(None);
        }
        fs::read_dir(&home_state)
            .ok()?
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| fs::read_to_string(e.path()).ok())
            .filter_map(|text| serde_json::from_str::<Value>(&text).ok())
            .find(|record| record["pid"].as_u64() == Some(u64::from(pid)))
            .map(Some)
    })
    .flatten();
    state_unset.finish(SETTLE);

    // Then
    let mut wrong = Vec::new();
    for (case, document, limit, path) in [
        (
            "XDG_CONFIG_HOME set",
            &with_xdg,
            2,
            sandbox.machine_config(),
        ),
        (
            "XDG_CONFIG_HOME unset",
            &without_xdg,
            3,
            home_config.clone(),
        ),
        ("XDG_CONFIG_HOME empty", &empty_xdg, 3, home_config.clone()),
    ] {
        if document["limit"] != json!(limit) || document["source"]["limit"] != json!("machine") {
            wrong.push(format!(
                "{case}: limit {} from {}, not {limit} from machine",
                document["limit"], document["source"]["limit"]
            ));
        }
        let shown = document["config"].as_str().unwrap_or_default();
        if !same_path(shown, &path) {
            wrong.push(format!("{case}: config {shown:?}, not {}", path.display()));
        }
    }
    if set_record.is_none() {
        wrong.push(format!(
            "with XDG_STATE_HOME set, no record of the job in {}",
            sandbox.jobs_dir().display()
        ));
    }
    if home_record.is_none() {
        wrong.push(format!(
            "with XDG_STATE_HOME unset, no record of the job in {}",
            home_state.display()
        ));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn machine_config__should_ignore_the_repository() {
    // Given
    let sandbox = Sandbox::must_new("machine-repository");
    let pool = must_lone_pool(&sandbox);
    sandbox.remove_machine();
    let other = sandbox.root.join("other-machine.toml");
    fs::write(&other, "[jobs]\nlimit = 2\nenforce = \"off\"\n").expect("setup: write a file");
    let config = pool.common.join("wtpool.toml");
    let original = fs::read_to_string(&config).expect("setup: read wtpool.toml");
    let cores = cores(16);
    let base = [("WTPOOL_VERIFY_CORES", OsStr::new(&cores))];
    let variables = [
        ("WTPOOL_JOB_LIMIT", OsStr::new("1")),
        ("WTPOOL_JOBS_LIMIT", OsStr::new("1")),
        ("WTPOOL_LIMIT", OsStr::new("1")),
        ("WTPOOL_ENFORCE", OsStr::new("off")),
        ("WTPOOL_JOB_ENFORCE", OsStr::new("off")),
        ("WTPOOL_MACHINE_CONFIG", other.as_os_str()),
        ("WTPOOL_MACHINE_TOML", other.as_os_str()),
        ("WTPOOL_CONFIG", other.as_os_str()),
    ];

    // When
    let plain = must_job_ls_json(&sandbox, &pool.fg, &base);
    let varied: Vec<(&str, Value)> = variables
        .iter()
        .map(|(name, value)| {
            let envs = [base[0], (*name, *value)];
            (*name, must_job_ls_json(&sandbox, &pool.fg, &envs))
        })
        .collect();
    let refused: Vec<(&str, Output)> = ["limit = 2\n", "enforce = \"off\"\n"]
        .iter()
        .map(|line| {
            fs::write(&config, format!("{original}{line}")).expect("setup: write wtpool.toml");
            (
                *line,
                wtpool(&sandbox, &pool.fg, &["status", "--json"], &base),
            )
        })
        .collect();

    // Then
    let mut wrong = Vec::new();
    let settings = |document: &Value| {
        (
            document["limit"].clone(),
            document["enforce"].clone(),
            document["source"].clone(),
            document["config"].clone(),
        )
    };
    if settings(&plain).0 != json!(4) || settings(&plain).1 != json!("warn") {
        wrong.push(format!(
            "without machine.toml on 16 cores: limit {} and enforce {}, not 4 and warn",
            plain["limit"], plain["enforce"]
        ));
    }
    for (name, document) in &varied {
        if settings(document) != settings(&plain) {
            wrong.push(format!(
                "{name} changed the settings: {:?}, not {:?}",
                settings(document),
                settings(&plain)
            ));
        }
    }
    for (line, output) in &refused {
        let stderr = stderr_of(output);
        if output.status.success() || !stderr.contains("wtpool job") {
            wrong.push(format!(
                "{} in wtpool.toml must be an unknown key whose error names wtpool job: {}",
                line.trim(),
                describe(output)
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The fault that makes a machine config invalid.
enum Fault {
    Text(&'static str),
    Unreadable,
    FileMode(u32),
    FolderMode(u32),
}

#[rstest]
#[case::unknown_key(Fault::Text("[jobs]\nlimits = 2\n"))]
#[case::unknown_top_level_key(Fault::Text("limit = 2\n"))]
#[case::unknown_table(Fault::Text("[jobs]\nlimit = 2\n\n[other]\nx = 1\n"))]
#[case::limit_a_string(Fault::Text("[jobs]\nlimit = \"4\"\n"))]
#[case::limit_a_fraction(Fault::Text("[jobs]\nlimit = 2.5\n"))]
#[case::heavy_a_string(Fault::Text("[jobs]\nheavy = \"make\"\n"))]
#[case::limit_zero(Fault::Text("[jobs]\nlimit = 0\n"))]
#[case::limit_65(Fault::Text("[jobs]\nlimit = 65\n"))]
#[case::limit_negative(Fault::Text("[jobs]\nlimit = -1\n"))]
#[case::unknown_policy(Fault::Text("[jobs]\nenforce = \"strict\"\n"))]
#[case::bad_pattern(Fault::Text("[jobs]\nheavy = [\"make *\"]\n"))]
#[case::not_toml(Fault::Text("[jobs\nlimit = 2\n"))]
#[case::unreadable(Fault::Unreadable)]
#[case::file_writable_by_group(Fault::FileMode(0o664))]
#[case::file_writable_by_others(Fault::FileMode(0o646))]
#[case::folder_writable_by_group(Fault::FolderMode(0o775))]
#[case::folder_writable_by_others(Fault::FolderMode(0o757))]
fn machine_config__should_reject_invalid_files(#[case] fault: Fault) {
    // Given
    let sandbox = Sandbox::must_new("machine-invalid");
    let pool = must_lone_pool(&sandbox);
    let path = sandbox.machine_config();
    match &fault {
        Fault::Text(text) => sandbox.write_machine(text),
        Fault::Unreadable => {
            sandbox.write_machine("[jobs]\nlimit = 2\n");
            set_mode(&path, 0o000);
        }
        Fault::FileMode(mode) => {
            sandbox.write_machine("[jobs]\nlimit = 2\n");
            set_mode(&path, *mode);
        }
        Fault::FolderMode(mode) => {
            sandbox.write_machine("[jobs]\nlimit = 2\n");
            set_mode(path.parent().expect("setup: a parent"), *mode);
        }
    }
    let spec = JobSpec {
        program: "probe-tool".to_owned(),
        cwd: Some(pool.fg.clone()),
        ..JobSpec::default()
    };

    // When
    let mut job = sandbox.must_start_job(&spec);
    let status = job.wait_exit(SETTLE);
    let stderr = job.stderr();
    let started = job.start_mark().is_some();

    // Then
    assert_eq!(
        status.and_then(|s| s.code()),
        Some(125),
        "wtpool job with an invalid machine.toml: exit {status:?}, stderr {stderr:?}"
    );
    assert!(
        names_path(&stderr, &path),
        "the error does not name {}: {stderr:?}",
        path.display()
    );
    assert!(!started, "the command ran although machine.toml is invalid");
}

#[rstest]
#[case::one_core(1, 1)]
#[case::four_cores(4, 1)]
#[case::seven_cores(7, 1)]
#[case::sixteen_cores(16, 4)]
#[case::sixty_four_cores(64, 8)]
fn machine_config__should_default_to_a_quarter_of_the_cores(
    #[case] count: u32,
    #[case] expected: u64,
) {
    // Given
    let sandbox = Sandbox::must_new("machine-default");
    let pool = must_lone_pool(&sandbox);
    sandbox.remove_machine();
    let count = cores(count);
    let envs = [("WTPOOL_VERIFY_CORES", OsStr::new(&count))];

    // When
    let absent = must_job_ls_json(&sandbox, &pool.fg, &envs);
    let printed = wtpool(&sandbox, &pool.fg, &["job", "limit"], &envs);
    sandbox.write_machine("[jobs]\nenforce = \"off\"\n");
    let partial = must_job_ls_json(&sandbox, &pool.fg, &envs);

    // Then
    let mut wrong = Vec::new();
    let want = json!({
        "limit": expected, "default": expected, "min": 1, "max": 64, "enforce": "warn",
        "source": {"limit": "default", "enforce": "default"}, "error": null, "heavy": [],
    });
    for field in [
        "limit", "default", "min", "max", "enforce", "source", "error", "heavy",
    ] {
        if absent[field] != want[field] {
            wrong.push(format!(
                "no machine.toml, {count} cores: {field} {}, not {}",
                absent[field], want[field]
            ));
        }
    }
    let words = tokens(&stdout_of(&printed));
    if !printed.status.success()
        || !words.contains(&expected.to_string())
        || !words.contains("default")
    {
        wrong.push(format!(
            "wtpool job limit does not print {expected} from default: {}",
            describe(&printed)
        ));
    }
    let partial_want = (
        json!(expected),
        json!("off"),
        json!({"limit": "default", "enforce": "machine"}),
    );
    let partial_got = (
        partial["limit"].clone(),
        partial["enforce"].clone(),
        partial["source"].clone(),
    );
    if partial_got != partial_want {
        wrong.push(format!(
            "machine.toml with enforce alone: {partial_got:?}, not {partial_want:?}"
        ));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn machine_config__should_read_through_a_symlink() {
    // Given
    let sandbox = Sandbox::must_new("machine-symlink");
    let pool = must_lone_pool(&sandbox);
    let elsewhere = sandbox.root.join("elsewhere");
    fs::create_dir_all(elsewhere.join("a-folder")).expect("setup: create a folder");
    let target = elsewhere.join("machine-real.toml");
    fs::write(&target, "[jobs]\nlimit = 5\nenforce = \"require\"\n").expect("setup: write");
    let path = sandbox.machine_config();
    sandbox.remove_machine();
    must_symlink(&target, &path);

    // When
    let through = must_job_ls_json(&sandbox, &pool.fg, &[]);
    fs::remove_file(&path).expect("setup: remove the symlink");
    must_symlink(&elsewhere.join("a-folder"), &path);
    let spec = JobSpec {
        program: "probe-tool".to_owned(),
        cwd: Some(pool.fg.clone()),
        ..JobSpec::default()
    };
    let mut job = sandbox.must_start_job(&spec);
    let to_folder = job.wait_exit(SETTLE).and_then(|s| s.code());

    // Then
    let got = (
        through["limit"].clone(),
        through["enforce"].clone(),
        through["source"].clone(),
    );
    let want = (
        json!(5),
        json!("require"),
        json!({"limit": "machine", "enforce": "machine"}),
    );
    assert_eq!(got, want, "machine.toml as a symlink to a regular file");
    assert_eq!(
        to_folder,
        Some(125),
        "machine.toml as a symlink to a folder must be an unreadable file: stderr {:?}",
        job.stderr()
    );
    assert!(
        job.start_mark().is_none(),
        "the command ran with machine.toml a symlink to a folder"
    );
}

#[test]
fn earlier_builds__should_be_the_machine_config_of_the_js_sandboxes() {
    // Given
    let code = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vscode/test/support/code.mjs");

    // When
    let text = fs::read_to_string(&code).expect("setup: read vscode/test/support/code.mjs");

    // Then
    let line = format!(
        "export const EARLIER_BUILDS = '{}';",
        EARLIER_BUILDS.replace('\n', "\\n")
    );
    assert!(text.contains(&line), "code.mjs no longer has {line}");
}
