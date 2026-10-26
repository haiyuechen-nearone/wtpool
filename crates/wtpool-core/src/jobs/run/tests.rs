use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};

use rstest::rstest;

use super::*;
use crate::clock::{SystemClock, Timestamp};
use crate::heavy::Pattern;
use crate::jobs::{JobRecord, JobState};
use crate::processes::{RunningProcess, StartTime};
use crate::test_support::eventually;

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

/// A process table of this process, started at noon, and of its parents, nearest first.
struct Lineage(Vec<u32>);

impl ListProcesses for Lineage {
    fn list(&self, query: &ProcessQuery<'_>) -> Vec<RunningProcess> {
        let pids = std::iter::once(std::process::id()).chain(self.0.iter().copied());
        let parents = self
            .0
            .iter()
            .copied()
            .map(Some)
            .chain(std::iter::once(None));
        pids.zip(parents)
            .filter(|(pid, _)| query.pids.is_none_or(|pids| pids.contains(pid)))
            .map(|(pid, parent)| RunningProcess {
                pid,
                parent: parent.filter(|_| query.parent),
                started: StartTime::parse("2026-10-20T12:00:00Z").filter(|_| query.started),
                name: String::new(),
                cwd: PathBuf::new(),
                cmd: Vec::new(),
            })
            .collect()
    }
}

/// A clock whose system time moves on by 192 s at each read.
#[derive(Default)]
struct Steps(AtomicI64);

impl ReadClock for Steps {
    fn now(&self) -> Timestamp {
        Timestamp::from_seconds(0)
    }

    fn system_micros(&self) -> i64 {
        self.0.fetch_add(192_000_000, Ordering::SeqCst)
    }
}

static NO_PATTERNS: RepositoryPatterns = RepositoryPatterns {
    heavy: Vec::new(),
    build_processes: Vec::new(),
};

const OUTER: u32 = 4242;

fn context<'a, C, P>(dir: &Path, clock: &'a C, processes: &'a P) -> JobContext<'a, C, P> {
    JobContext {
        paths: JobPaths {
            config: dir.join("config/wtpool/machine.toml"),
            folder: dir.join("state/wtpool/jobs"),
        },
        cores: 4,
        parent: None,
        cwd: dir.to_path_buf(),
        repository: &NO_PATTERNS,
        clock,
        processes,
    }
}

fn write_limit<C, P>(context: &JobContext<'_, C, P>, limit: u32) {
    std::fs::create_dir_all(context.paths.config.parent().unwrap()).unwrap();
    std::fs::write(&context.paths.config, format!("[jobs]\nlimit = {limit}\n")).unwrap();
}

/// A running job of the process `pid` in `folder`, with the lock it holds.
fn running_job(folder: &JobFolder, pid: u32) -> (JobRecord, std::fs::File) {
    let record = JobRecord {
        state: JobState::Running,
        ..JobRecord::sample(pid)
    };
    let held = folder.create_lock(&record.id).unwrap();
    folder.write_record(&record).unwrap();
    (record, held)
}

#[test]
fn draft__should_name_the_job_by_the_repository_patterns_too() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let repository = RepositoryPatterns {
        heavy: vec![Pattern::parse("cargo xtask verify").unwrap()],
        build_processes: vec!["protoc".to_owned()],
    };
    let processes = Lineage(Vec::new());
    let mut context = context(dir.path(), &SystemClock, &processes);
    context.repository = &repository;
    let config = MachineConfig::defaults(dir.path().join("machine.toml"), 4);

    // When
    let verify = draft(
        &command("cargo", &["xtask", "verify", "--phase", "4"]),
        &context,
        &config,
    );
    let protoc = draft(&command("protoc", &["--rust_out=x"]), &context, &config);

    // Then
    assert_eq!(
        (verify.summary.as_str(), protoc.summary.as_str()),
        ("cargo xtask verify", "protoc")
    );
}

#[test]
fn draft__should_hold_the_summary_label_and_start_of_this_job() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let mut job = command("cargo", &["test", "--token=secret"]);
    job.label = Some("\u{202e}unit tests".to_owned());
    let config = MachineConfig::defaults(dir.path().join("machine.toml"), 4);
    let processes = Lineage(Vec::new());

    // When
    let draft = draft(
        &job,
        &context(dir.path(), &SystemClock, &processes),
        &config,
    );

    // Then
    assert_eq!(
        (
            draft.summary.as_str(),
            draft.label.as_deref(),
            draft.started,
            draft.pid
        ),
        (
            "cargo test",
            Some("unit tests"),
            Some(Timestamp::from_seconds(1_792_497_600)),
            std::process::id()
        )
    );
    let record = draft.queued(1, JobState::Waiting, Timestamp::from_seconds(0));
    assert!(!serde_json::to_string(&record).unwrap().contains("secret"));
}

#[cfg(unix)]
#[test]
fn run_job__should_pass_the_exit_code_and_leave_no_record() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let processes = Lineage(Vec::new());
    let context = context(dir.path(), &SystemClock, &processes);
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
    let processes = Lineage(vec![OUTER, 1]);
    let mut context = context(dir.path(), &SystemClock, &processes);
    write_limit(&context, 1);
    let folder = JobFolder::create(context.paths.folder.clone()).unwrap();
    let (running, _held) = running_job(&folder, OUTER);
    context.parent = Some(OsString::from(running.id.as_str()));

    // When
    let code = run_job(&command("sh", &["-c", "exit 0"]), &context, &mut Vec::new()).unwrap();

    // Then
    assert_eq!(code, 0);
    assert_eq!(folder.records(), vec![running]);
}

#[rstest]
#[case::the_parent(&[OUTER, 1], true)]
#[case::a_grandparent(&[7, OUTER, 1], true)]
#[case::no_ancestor(&[7, 1], false)]
#[case::a_loop_of_parents(&[7, 8, 7, 8], false)]
fn inside_live_job__should_need_the_job_process_among_the_ancestors(
    #[case] parents: &[u32],
    #[case] expected: bool,
) {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let folder = JobFolder::create(dir.path().join("jobs")).unwrap();
    let (running, _held) = running_job(&folder, OUTER);

    // When
    let inside = inside_live_job(
        &folder,
        Some(OsStr::new(running.id.as_str())),
        &Lineage(parents.to_vec()),
    );

    // Then
    assert_eq!(inside, expected);
}

#[test]
fn inside_live_job__should_need_a_live_job() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let folder = JobFolder::create(dir.path().join("jobs")).unwrap();
    let (dead, held) = running_job(&folder, OUTER);
    drop(held);
    let processes = Lineage(vec![OUTER, 1]);
    let jobs = [
        Some(OsStr::new(dead.id.as_str())),
        Some(OsStr::new("4242-0badc0de")),
        Some(OsStr::new("4242")),
        None,
    ];

    // When
    let outside = eventually(|| {
        jobs.iter()
            .all(|job| !inside_live_job(&folder, *job, &processes))
    });

    // Then
    assert!(outside);
}

#[test]
fn wait_for_turn__should_run_once_the_limit_rises_and_time_the_wait_by_the_clock() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let processes = Lineage(Vec::new());
    let clock = Steps::default();
    let context = context(dir.path(), &clock, &processes);
    let folder = JobFolder::create(context.paths.folder.clone()).unwrap();
    let (_running, _held) = running_job(&folder, OUTER);
    let one = Limit::new(1).unwrap();
    let config = MachineConfig::defaults(PathBuf::new(), 4);
    let job = command("make", &[]);
    let (place, turn) =
        Place::join(folder, draft(&job, &context, &config), one, clock.now()).unwrap();
    let Turn::Wait(standing) = turn else {
        panic!("the job ran over its limit");
    };
    write_limit(&context, 2);
    let signals = JobSignals::catch().unwrap();
    let mut notes = Vec::new();

    // When
    let place = wait_for_turn(place, standing, &job, &context, one, &signals, &mut notes);

    // Then
    assert!(place.is_ok());
    assert_eq!(
        String::from_utf8_lossy(&notes),
        "wtpool job: 1 of 1 jobs run; waiting, 0 ahead\nwtpool job: started after 3 m 12 s\n"
    );
}
