//! A job's folder, record and pass-through (J7, J8, J13 to J17): an unsafe job folder refuses
//! the job, a record keeps only the summary, the command gets exactly its arguments, streams,
//! exit status and signals, and `wtpool job` says only that it waits.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rstest::rstest;
use serde_json::{Value, json};

use super::jobs_queue::{HANDOVER, limit_config, must_start, must_wait};
use crate::support::jobs::{JobSpec, SETTLE, describe, job_records, must_job_ls_json, wtpool};
use crate::support::{Sandbox, must_wtpool_bin};

/// The mode bits of `path`, `None` where the system has none.
fn mode(path: &Path) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::symlink_metadata(path)
            .ok()
            .map(|meta| meta.permissions().mode() & 0o777)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("setup: chmod");
}

/// Whether the process `pid` still lives.
pub(super) fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-s", "0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[derive(Debug, Clone, Copy)]
enum Folder {
    Symlink,
    GroupWritable,
    OthersWritable,
    New,
}

#[rstest]
#[case::a_symlink(Folder::Symlink)]
#[case::group_writable(Folder::GroupWritable)]
#[case::others_writable(Folder::OthersWritable)]
#[case::made_by_the_first_job(Folder::New)]
fn job__should_refuse_an_unsafe_folder(#[case] folder: Folder) {
    #[cfg(not(unix))]
    panic!(
        "skipped on this machine: {folder:?} needs Unix modes; Windows relies on the profile folder's access list (J17)"
    );
    #[cfg(unix)]
    {
        // Given
        let sandbox = Sandbox::must_new("job-folder");
        sandbox.write_machine(&limit_config(2));
        let jobs = sandbox.jobs_dir();
        let parent = jobs.parent().expect("setup: a parent").to_path_buf();
        fs::create_dir_all(&parent).expect("setup: create <state>/wtpool");
        let target = sandbox.root.join("elsewhere-jobs");
        match folder {
            Folder::Symlink => {
                fs::create_dir_all(&target).expect("setup: create the link target");
                set_mode(&target, 0o700);
                crate::support::must_symlink(&target, &jobs);
            }
            Folder::GroupWritable | Folder::OthersWritable => {
                fs::create_dir_all(&jobs).expect("setup: create the job folder");
                let bits = if matches!(folder, Folder::GroupWritable) {
                    0o770
                } else {
                    0o707
                };
                set_mode(&jobs, bits);
            }
            Folder::New => {
                let _ = fs::remove_dir_all(&jobs);
            }
        }

        // When
        let mut job = sandbox.must_start_job(&JobSpec {
            program: "probe".into(),
            ..JobSpec::default()
        });
        let status = job.wait_exit(SETTLE);

        // Then
        let code = status.and_then(|s| s.code());
        let stderr = job.stderr();
        if matches!(folder, Folder::New) {
            assert_eq!(code, Some(0), "a job with no job folder yet: {stderr:?}");
            assert!(
                job.start_mark().is_some(),
                "a job with no job folder yet never ran"
            );
            assert_eq!(
                mode(&jobs),
                Some(0o700),
                "the mode of the job folder the first job made"
            );
            return;
        }
        assert_eq!(code, Some(125), "{folder:?}: wtpool job: {stderr:?}");
        assert!(
            job.start_mark().is_none(),
            "{folder:?}: the command ran in spite of the unsafe job folder"
        );
        assert!(
            stderr.contains("wtpool/jobs"),
            "{folder:?}: the error does not name the job folder: {stderr:?}"
        );
        let written: Vec<PathBuf> = fs::read_dir(if matches!(folder, Folder::Symlink) {
            &target
        } else {
            &jobs
        })
        .map(|entries| entries.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
        assert!(
            written.is_empty(),
            "{folder:?}: wtpool job wrote into the unsafe folder: {written:?}"
        );
    }
}

/// `^<pid>[-_.]?[0-9a-f]{8}$`: J8 names the pid and 8 random hex digits, no separator.
fn is_job_id(id: &str, pid: u32) -> bool {
    let Some(rest) = id.strip_prefix(&pid.to_string()) else {
        return false;
    };
    let rest = rest.strip_prefix(['-', '_', '.']).unwrap_or(rest);
    rest.len() == 8 && rest.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

fn is_time(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        text.len() >= 20 && text.ends_with('Z') && text.get(10..11) == Some("T")
    })
}

/// Every byte of every file below `dir`, with its path.
fn contents(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_file())
                .map(|e| (e.path(), fs::read(e.path()).unwrap_or_default()))
                .collect()
        })
        .unwrap_or_default()
}

fn holds(bytes: &[u8], secret: &str) -> bool {
    bytes
        .windows(secret.len())
        .any(|window| window == secret.as_bytes())
}

const SECRETS: [&str; 4] = [
    "wtpool-verify-token-argument",
    "wtpool-verify-bare-argument",
    "wtpool-verify-environment-value",
    "wtpool-verify-standard-input",
];

#[test]
fn job__should_record_only_its_summary() {
    // Given
    let sandbox = Sandbox::must_new("job-record");
    sandbox.write_machine("[jobs]\nlimit = 2\nheavy = [\"cargo test\"]\n");
    let cwd = sandbox.root.join("work");
    fs::create_dir_all(&cwd).expect("setup: create the job's folder");
    let [token, bare, env_value, input] = SECRETS;
    let mut spec = JobSpec::held("cargo", &["test", &format!("--token={token}"), bare])
        .in_dir(&cwd)
        .with_env("WTPOOL_VERIFY_SECRET", env_value);
    spec.copy_stdin = true;
    spec.stdin = Some(input.as_bytes().to_vec());

    // When
    let mut job = sandbox.must_start_job(&spec);
    must_start(&job, "the job");
    let record = job.wait_record(&sandbox, "running", SETTLE);
    let files = contents(&sandbox.jobs_dir());
    let modes: Vec<(PathBuf, Option<u32>)> =
        files.iter().map(|(p, _)| (p.clone(), mode(p))).collect();
    let folder_mode = mode(&sandbox.jobs_dir());
    let code = job.finish(SETTLE).and_then(|s| s.code());
    let after = contents(&sandbox.jobs_dir());

    // Then
    let record = record.unwrap_or_else(|| {
        panic!(
            "no running record of pid {}: {:?}",
            job.pid(),
            job_records(&sandbox)
        )
    });
    let keys: Vec<&str> = record
        .as_object()
        .map(|map| map.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let mut expected_keys = vec![
        "cwd", "id", "label", "pid", "queued", "since", "started", "state", "summary", "ticket",
        "version",
    ];
    expected_keys.sort_unstable();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, expected_keys, "the record's keys: {record}");
    let id = record["id"].as_str().unwrap_or_default();
    assert!(
        is_job_id(id, job.pid()),
        "the id {id:?} is not the pid {} and 8 hex digits",
        job.pid()
    );
    let canonical = fs::canonicalize(&cwd).unwrap_or_else(|_| cwd.clone());
    let cwd_ok = [cwd.to_string_lossy(), canonical.to_string_lossy()]
        .iter()
        .any(|path| record["cwd"] == json!(path));
    assert!(
        cwd_ok,
        "the record's cwd is not the job's folder {}: {record}",
        cwd.display()
    );
    assert_eq!(
        (
            &record["version"],
            &record["pid"],
            &record["summary"],
            &record["label"],
            &record["state"]
        ),
        (
            &json!(1),
            &json!(job.pid()),
            &json!("cargo test"),
            &Value::Null,
            &json!("running")
        ),
        "the record's version, pid, summary, label and state: {record}"
    );
    assert!(
        record["ticket"].is_u64(),
        "the ticket is no counter value: {record}"
    );
    assert!(
        !record["started"].is_null(),
        "the record has no process start time: {record}"
    );
    assert!(
        is_time(&record["queued"]) && is_time(&record["since"]),
        "queued and since are not RFC 3339 times: {record}"
    );
    let names: Vec<String> = files
        .iter()
        .filter_map(|(p, _)| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    for suffix in ["json", "lock"] {
        let name = format!("{id}.{suffix}");
        assert!(
            names.contains(&name),
            "the job folder lacks {name}: {names:?}"
        );
    }
    let wide: Vec<&(PathBuf, Option<u32>)> = modes
        .iter()
        .filter(|(path, bits)| {
            let own = path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(id));
            bits.is_some_and(|bits| {
                if own {
                    bits != 0o600
                } else {
                    bits & 0o077 != 0
                }
            })
        })
        .collect();
    assert!(
        wide.is_empty(),
        "job files readable by others, or a record or lock not 0600: {wide:?}"
    );
    if cfg!(unix) {
        assert_eq!(folder_mode, Some(0o700), "the job folder's mode");
    }
    for (path, bytes) in files.iter().chain(&after) {
        for secret in SECRETS {
            assert!(
                !holds(bytes, secret),
                "{} holds {secret:?}, an argument, a variable or the stream of the command",
                path.display()
            );
        }
    }
    assert_eq!(code, Some(0), "the job's exit code");
    let left: Vec<&PathBuf> = after
        .iter()
        .map(|(p, _)| p)
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(id))
        })
        .collect();
    assert!(left.is_empty(), "the ended job left {left:?}");
}

#[test]
fn job__should_not_pass_its_lock_to_the_command() {
    // Given: limit 1, one job running and one waiting.
    let sandbox = Sandbox::must_new("job-lock");
    sandbox.write_machine(&limit_config(1));
    let mut first = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_start(&first, "the running job");
    let second = sandbox.must_start_job(&JobSpec::held("probe", &[]));
    must_wait(&second, &sandbox, "the waiting job");
    let probe = first.probe_pid().unwrap_or_default();

    // When: wtpool job alone is killed; its command lives on.
    first.signal("KILL");
    let reaped = first.wait_exit(SETTLE);
    let started = second.wait_start(HANDOVER);
    let probe_lives = alive(probe);

    // Then
    assert!(reaped.is_some(), "wtpool job outlived SIGKILL");
    assert!(
        probe_lives,
        "setup: the command of the killed job ended with it, so nothing tells whether it held the lock"
    );
    assert!(
        started.is_some(),
        "the waiting job did not start within {HANDOVER:?} while the killed job's command lives: the command holds the job's lock"
    );
}

/// 64 rounds of every byte value.
fn binary_bytes() -> Vec<u8> {
    (0..64).flat_map(|_| 0..=255_u8).collect()
}

#[test]
fn job__should_pass_streams_and_exit_codes_through() {
    // Given
    let sandbox = Sandbox::must_new("job-streams");
    sandbox.write_machine(&limit_config(4));
    let home = sandbox.root.join("home");
    let args = ["two words", "", "--flag=x", "ü", "-", "--", "+x"];
    let bytes = binary_bytes();
    let copy = JobSpec {
        program: "probe".into(),
        args: args.iter().map(|a| (*a).to_owned()).collect(),
        copy_stdin: true,
        stdin: Some(bytes.clone()),
        ..JobSpec::default()
    };
    let plain = sandbox.root.join("plain");
    fs::create_dir_all(&plain).expect("setup: create a folder");
    let not_executable = plain.join("not-executable");
    fs::write(&not_executable, "#!/bin/sh\nexit 0\n").expect("setup: write a file");
    #[cfg(unix)]
    set_mode(&not_executable, 0o644);

    // When
    let mut streams = sandbox.must_start_job(&copy);
    let streamed = streams.wait_exit(SETTLE).and_then(|s| s.code());
    let mut codes = Vec::new();
    for code in [0_u8, 3, 255] {
        let mut job = sandbox.must_start_job(&JobSpec {
            program: "probe".into(),
            exit: Some(code),
            ..JobSpec::default()
        });
        let status = job.wait_exit(SETTLE).and_then(|s| s.code());
        codes.push((code, status, job.stdout(), job.start_mark().is_some()));
    }
    let refused = wtpool(
        &sandbox,
        &home,
        &["job", "--", &not_executable.to_string_lossy()],
        &[],
    );
    let missing = wtpool(
        &sandbox,
        &home,
        &["job", "--", "wtpool-verify-no-such-program"],
        &[],
    );

    // Then
    let start = streams.start_mark().unwrap_or(Value::Null);
    assert_eq!(
        start["args"],
        json!(args),
        "the command's arguments, which must pass exactly: {start}; stderr {:?}",
        streams.stderr()
    );
    assert_eq!(streamed, Some(0), "the copying job's exit code");
    assert!(
        streams.stdout() == bytes,
        "stdout is not the {} bytes the command wrote ({} bytes)",
        bytes.len(),
        streams.stdout().len()
    );
    assert!(
        streams.stderr_bytes() == bytes,
        "stderr is not the {} bytes the command wrote, with nothing of wtpool job's own ({} bytes)",
        bytes.len(),
        streams.stderr_bytes().len()
    );
    for (code, status, stdout, ran) in &codes {
        assert!(*ran, "a job meant to exit {code} never ran its command");
        assert_eq!(
            *status,
            Some(i32::from(*code)),
            "the exit code of a command that exits {code}"
        );
        assert!(stdout.is_empty(), "wtpool job wrote to stdout: {stdout:?}");
    }
    assert_eq!(
        refused.status.code(),
        Some(126),
        "a command that is not executable: {}",
        describe(&refused)
    );
    assert_eq!(
        missing.status.code(),
        Some(127),
        "a command that is not found: {}",
        describe(&missing)
    );
    assert!(
        refused.stdout.is_empty() && missing.stdout.is_empty(),
        "wtpool job wrote to stdout for a command that cannot start"
    );
}

#[test]
fn job__should_keep_a_terminal_a_terminal() {
    #[cfg(not(unix))]
    panic!("skipped on this machine: the check opens a terminal through script, a Unix tool");
    #[cfg(unix)]
    {
        // Given
        let sandbox = Sandbox::must_new("job-tty");
        sandbox.write_machine(&limit_config(2));
        let probe = sandbox.must_install_probe("probe");
        let marks = sandbox.marks_dir();
        let home = sandbox.root.join("home");
        let wtpool = must_wtpool_bin();
        let mut command = sandbox.command(Path::new("script"), &home);
        if cfg!(target_os = "macos") {
            command
                .args(["-q", "/dev/null"])
                .arg(&wtpool)
                .args(["job", "--"])
                .arg(&probe);
        } else {
            let line = format!("'{}' job -- '{}'", wtpool.display(), probe.display());
            command.args(["-qec", &line, "/dev/null"]);
        }
        command
            .env("WTPOOL_PROBE_MARKS", &marks)
            .env("WTPOOL_PROBE_LABEL", "tty")
            .env("WTPOOL_PROBE_EXIT", "3")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // When
        let child = command.spawn().expect("setup: start script");
        let output = crate::support::hook::wait_output(child, SETTLE);
        let start = fs::read_to_string(marks.join("tty.start"))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());

        // Then
        let start = start.unwrap_or_else(|| {
            panic!(
                "the command of wtpool job never ran in the terminal: {}",
                describe(&output)
            )
        });
        assert_eq!(
            start["tty"],
            json!([true, true, true]),
            "stdin, stdout and stderr of the command in a terminal: {start}"
        );
        assert_eq!(
            output.status.code(),
            Some(3),
            "the exit code through the terminal"
        );
    }
}

#[derive(Debug, Clone, Copy)]
struct Summary {
    heavy: &'static [&'static str],
    program: &'static str,
    args: &'static [&'static str],
    label: Option<&'static str>,
    summary: &'static str,
}

/// A label of 60 characters with a control, a bidi and a zero width character.
const BIDI_LABEL: &str =
    "\u{7}\u{202e}\u{200b}unit tests of the parser, the planner and the whole graph";

#[rstest]
#[case::go_test_by_its_pattern(Summary { heavy: &["go test"], program: "go", args: &["test", "./..."], label: None, summary: "go test" })]
#[case::go_test_without_a_pattern(Summary { heavy: &[], program: "go", args: &["test", "./..."], label: None, summary: "go" })]
#[case::nix_develop_cargo_test(Summary { heavy: &["cargo test"], program: "nix", args: &["develop", "-c", "cargo", "test"], label: None, summary: "cargo test" })]
#[case::a_bidi_label(Summary { heavy: &[], program: "make", args: &["-j8"], label: Some(BIDI_LABEL), summary: "make" })]
fn job__should_summarise_by_pattern_else_h14(#[case] case: Summary) {
    // Given
    let sandbox = Sandbox::must_new("job-summary");
    let heavy: Vec<String> = case.heavy.iter().map(|p| format!("\"{p}\"")).collect();
    sandbox.write_machine(&format!(
        "[jobs]\nlimit = 2\nheavy = [{}]\n",
        heavy.join(", ")
    ));
    let home = sandbox.root.join("home");
    let mut spec = JobSpec::held(case.program, case.args);
    spec.label = case.label.map(str::to_owned);

    // When
    let job = sandbox.must_start_job(&spec);
    must_start(&job, &format!("{case:?}"));
    let record = job
        .wait_record(&sandbox, "running", SETTLE)
        .unwrap_or(Value::Null);
    let listing = must_job_ls_json(&sandbox, &home, &[]);

    // Then
    let label: Option<String> = case.label.map(|label| {
        label
            .chars()
            .filter(|c| !matches!(c, '\u{7}' | '\u{202e}' | '\u{200b}'))
            .take(40)
            .collect()
    });
    assert_eq!(
        (&record["summary"], &record["label"]),
        (&json!(case.summary), &json!(label)),
        "{case:?}: the record's summary and label: {record}"
    );
    let entry = listing["running"]
        .as_array()
        .and_then(|running| running.first())
        .cloned()
        .unwrap_or(Value::Null);
    assert_eq!(
        (&entry["summary"], &entry["label"]),
        (&json!(case.summary), &json!(label)),
        "{case:?}: the summary and label of wtpool job ls --json: {listing}"
    );
    drop(job);
}

#[test]
fn job__should_run_on_windows() {
    #[cfg(not(windows))]
    panic!("skipped on this machine: J17 holds for Windows only, and this is not Windows");
    #[cfg(windows)]
    {
        // Given
        let sandbox = Sandbox::must_new("job-windows");
        sandbox.write_machine(&limit_config(2));
        let mut spec = JobSpec::held("probe", &[]);
        spec.exit = Some(3);

        // When
        let mut job = sandbox.must_start_job(&spec);
        must_start(&job, "a job on Windows");
        let record = job.wait_record(&sandbox, "running", SETTLE);
        let id = record
            .as_ref()
            .and_then(|r| r["id"].as_str().map(str::to_owned))
            .unwrap_or_default();
        let lock = sandbox.jobs_dir().join(format!("{id}.lock"));
        let locked = lock.exists();
        let code = job.finish(SETTLE).and_then(|s| s.code());

        // Then
        assert!(
            record.is_some(),
            "no running record on Windows: {:?}",
            job_records(&sandbox)
        );
        assert!(locked, "no lock file {} on Windows", lock.display());
        assert_eq!(code, Some(3), "the exit code on Windows");
        let _ = crate::support::jobs::wait_until(SETTLE, || (!lock.exists()).then_some(()));
    }
}
