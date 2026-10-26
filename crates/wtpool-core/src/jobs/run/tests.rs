use std::path::Path;

use rstest::rstest;

use super::*;
use crate::clock::{SystemClock, Timestamp};
use crate::heavy::Pattern;
use crate::processes::RunningProcess;

fn command(program: &str, args: &[&str]) -> JobCommand {
    JobCommand {
        program: OsString::from(program),
        args: args.iter().map(OsString::from).collect(),
        label: None,
        quiet: false,
    }
}

#[rstest]
#[case::by_its_pattern(&["go test"], "go", &["test", "./..."], "go test")]
#[case::without_a_pattern(&[], "go", &["test", "./..."], "go")]
#[case::through_nix_develop(&["cargo test"], "nix", &["develop", "-c", "cargo", "test"], "cargo test")]
#[case::a_listed_program_without_a_pattern(&[], "cargo", &["test"], "cargo test")]
#[case::options_after_the_program(&[], "make", &["-j8"], "make")]
#[case::a_star(&["cargo-*"], "cargo-nextest", &["run"], "cargo-nextest")]
#[case::no_program_after_the_prefixes(&[], "env", &["A=1"], "env")]
fn summary__should_name_the_pattern_else_the_program(
    #[case] heavy: &[&str],
    #[case] program: &str,
    #[case] args: &[&str],
    #[case] expected: &str,
) {
    // Given
    let patterns: Vec<Pattern> = heavy.iter().map(|p| Pattern::parse(p).unwrap()).collect();
    let heavy = HeavyList::new(&patterns, &[], &[]);

    // When
    let summary = summary(&command(program, args), &heavy);

    // Then
    assert_eq!(summary, expected);
}

#[rstest]
#[case::under_a_minute(Duration::from_millis(12_900), "12 s")]
#[case::minutes(Duration::from_secs(192), "3 m 12 s")]
#[case::an_hour(Duration::from_secs(3600), "60 m 0 s")]
fn waited_text__should_count_minutes_and_seconds(#[case] waited: Duration, #[case] expected: &str) {
    // When
    let text = waited_text(waited);

    // Then
    assert_eq!(text, expected);
}

/// A process table that lists this process as started at noon.
struct Noon;

impl ListProcesses for Noon {
    fn list(&self, query: &ProcessQuery<'_>) -> Vec<RunningProcess> {
        vec![RunningProcess {
            pid: std::process::id(),
            parent: None,
            started: crate::processes::StartTime::parse("2026-10-20T12:00:00Z")
                .filter(|_| query.started),
            name: String::new(),
            cwd: PathBuf::new(),
            cmd: Vec::new(),
        }]
    }
}

static NO_PATTERNS: RepositoryPatterns = RepositoryPatterns {
    heavy: Vec::new(),
    build_processes: Vec::new(),
};

fn context(dir: &Path) -> JobContext<'static> {
    JobContext {
        paths: JobPaths {
            config: dir.join("config/wtpool/machine.toml"),
            folder: dir.join("state/wtpool/jobs"),
        },
        cores: 4,
        parent: None,
        cwd: dir.to_path_buf(),
        repository: &NO_PATTERNS,
        clock: &SystemClock,
        processes: &Noon,
    }
}

#[test]
fn new_record__should_name_the_job_by_the_repository_patterns_too() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let repository = RepositoryPatterns {
        heavy: vec![Pattern::parse("cargo xtask verify").unwrap()],
        build_processes: vec!["protoc".to_owned()],
    };
    let mut context = context(dir.path());
    context.repository = &repository;
    let config = MachineConfig::defaults(dir.path().join("machine.toml"), 4);

    // When
    let verify = new_record(
        &command("cargo", &["xtask", "verify", "--phase", "4"]),
        &context,
        &config,
    );
    let protoc = new_record(&command("protoc", &["--rust_out=x"]), &context, &config);

    // Then
    assert_eq!(
        (verify.summary.as_str(), protoc.summary.as_str()),
        ("cargo xtask verify", "protoc")
    );
}

#[test]
fn new_record__should_hold_the_summary_label_and_start_of_this_job() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let mut job = command("cargo", &["test", "--token=secret"]);
    job.label = Some("\u{202e}unit tests".to_owned());
    let config = MachineConfig::defaults(dir.path().join("machine.toml"), 4);

    // When
    let record = new_record(&job, &context(dir.path()), &config);

    // Then
    assert_eq!(
        (
            record.summary.as_str(),
            record.label.as_deref(),
            record.started,
            record.pid
        ),
        (
            "cargo test",
            Some("unit tests"),
            Some(Timestamp::from_seconds(1_792_497_600)),
            std::process::id()
        )
    );
    assert!(!serde_json::to_string(&record).unwrap().contains("secret"));
}

#[cfg(unix)]
#[test]
fn run_job__should_pass_the_exit_code_and_leave_no_record() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let context = context(dir.path());
    let mut notes = Vec::new();

    // When
    let failed = run_job(&command("sh", &["-c", "exit 3"]), &context, &mut notes).unwrap();
    let missing = run_job(
        &command("wtpool-no-such-program", &[]),
        &context,
        &mut notes,
    )
    .unwrap();

    // Then
    let left: Vec<_> = std::fs::read_dir(&context.paths.folder)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name())
        .filter(|name| name.to_string_lossy().ends_with(".json"))
        .collect();
    assert_eq!((failed, missing), (3, 127));
    assert!(left.is_empty(), "{left:?}");
    assert!(
        String::from_utf8_lossy(&notes)
            .starts_with("wtpool job: cannot run wtpool-no-such-program")
    );
}

#[cfg(unix)]
#[test]
fn run_job__should_run_inside_a_live_job_at_once_and_record_nothing() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let mut context = context(dir.path());
    std::fs::create_dir_all(context.paths.config.parent().unwrap()).unwrap();
    std::fs::write(&context.paths.config, "[jobs]\nlimit = 1\n").unwrap();
    let folder = JobFolder::create(context.paths.folder.clone()).unwrap();
    let outer = JobId::new(1);
    let _held = folder.create_lock(&outer).unwrap();
    let mut running = new_record(
        &command("make", &[]),
        &context,
        &MachineConfig::defaults(PathBuf::new(), 4),
    );
    running.id = outer.clone();
    running.state = JobState::Running;
    folder.write_record(&running).unwrap();
    context.parent = Some(OsString::from(outer.as_str()));

    // When
    let code = run_job(&command("sh", &["-c", "exit 0"]), &context, &mut Vec::new()).unwrap();

    // Then
    assert_eq!(code, 0);
    assert_eq!(folder.records(), vec![running]);
}
