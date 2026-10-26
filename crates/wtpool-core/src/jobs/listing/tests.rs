use std::path::Path;

use super::*;
use crate::heavy::Pattern;
use crate::jobs::JobState;
use crate::jobs::record::{JobId, RECORD_VERSION};
use crate::processes::{RunningProcess, StartTime};

/// A process table of fixed processes.
struct Table(Vec<RunningProcess>);

impl ListProcesses for Table {
    fn list(&self, _: &ProcessQuery<'_>) -> Vec<RunningProcess> {
        self.0.clone()
    }
}

fn process(pid: u32, parent: u32, cmd: &[&str], started: &str) -> RunningProcess {
    RunningProcess {
        pid,
        parent: Some(parent),
        started: StartTime::parse(started),
        name: cmd.first().copied().unwrap_or_default().to_owned(),
        cwd: PathBuf::from(format!("/w/{pid}")),
        cmd: cmd.iter().map(|word| (*word).to_owned()).collect(),
    }
}

fn job(pid: u32, ticket: u64, state: JobState) -> JobRecord {
    JobRecord {
        version: RECORD_VERSION,
        id: JobId::new(pid),
        pid,
        started: None,
        summary: "cargo test".to_owned(),
        label: None,
        cwd: PathBuf::from("/w"),
        ticket,
        state,
        queued: Timestamp::from_seconds(10),
        since: Timestamp::from_seconds(20),
    }
}

fn heavy(patterns: &[&str]) -> HeavyList {
    let patterns: Vec<Pattern> = patterns
        .iter()
        .map(|p| Pattern::parse(p).unwrap())
        .collect();
    HeavyList::new(&patterns, &[], &[])
}

#[test]
fn outside_jobs__should_list_the_outermost_heavy_processes_no_job_holds_newest_first() {
    // Given
    let table = Table(vec![
        process(10, 1, &["make", "all"], "2026-10-20T11:00:00Z"),
        process(11, 10, &["make", "-C", "sub"], "2026-10-20T11:00:01Z"),
        process(
            20,
            1,
            &["wtpool", "job", "--", "make"],
            "2026-10-20T11:00:02Z",
        ),
        process(21, 20, &["make"], "2026-10-20T11:00:03Z"),
        process(30, 1, &["cargo", "build"], "2026-10-20T11:00:04Z"),
        process(40, 1, &["ls"], "2026-10-20T11:00:05Z"),
    ]);
    let jobs = [job(20, 1, JobState::Running)];
    let jobs: Vec<&JobRecord> = jobs.iter().collect();

    // When
    let outside = outside_jobs(
        &heavy(&["make", "cargo build"]),
        &jobs,
        &table,
        Timestamp::from_seconds(0),
    );

    // Then
    let found: Vec<(&str, &str, &Path)> = outside
        .iter()
        .map(|job| (job.id.as_str(), job.summary.as_str(), job.folder.as_path()))
        .collect();
    assert_eq!(
        found,
        vec![
            ("process-30", "cargo build", Path::new("/w/30")),
            ("process-10", "make", Path::new("/w/10")),
        ]
    );
    assert_eq!(
        outside.first().map(|job| job.since),
        Timestamp::parse("2026-10-20T11:00:04Z")
    );
}

#[test]
fn outside_jobs__should_read_no_table_without_a_pattern() {
    // Given
    let table = Table(vec![process(10, 1, &["make"], "2026-10-20T11:00:00Z")]);

    // When
    let outside = outside_jobs(&heavy(&[]), &[], &table, Timestamp::from_seconds(0));

    // Then
    assert!(outside.is_empty());
}

#[test]
fn job_listing_to_json__should_keep_the_order_of_the_listing() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let paths = JobPaths {
        config: dir.path().join("machine.toml"),
        folder: dir.path().join("jobs"),
    };
    let repository = RepositoryPatterns {
        heavy: vec![Pattern::parse("make").unwrap()],
        build_processes: vec!["protoc".to_owned()],
    };
    let suggested = vec![
        Suggestion {
            pattern: Pattern::parse("make").unwrap(),
            from: "Makefile".to_owned(),
        },
        Suggestion {
            pattern: Pattern::parse("cargo build").unwrap(),
            from: "Cargo.toml".to_owned(),
        },
    ];

    // When
    let listing = JobListing::gather(
        &paths,
        16,
        &repository,
        suggested,
        &Table(Vec::new()),
        Timestamp::from_seconds(0),
    );

    // Then
    let config = paths.config.to_string_lossy();
    assert_eq!(
        listing.to_json(),
        format!(
            "{{\"limit\":4,\"default\":4,\"min\":1,\"max\":64,\"enforce\":\"warn\",\"source\":{{\"limit\":\"default\",\"enforce\":\"default\"}},\"config\":{},\"error\":null,\"heavy\":[{{\"pattern\":\"make\",\"source\":\"repository\"}},{{\"pattern\":\"protoc\",\"source\":\"build_processes\"}}],\"suggested\":[{{\"pattern\":\"cargo build\",\"from\":\"Cargo.toml\"}}],\"running\":[],\"waiting\":[],\"outside\":[]}}\n",
            serde_json::to_string(&config).unwrap()
        )
    );
}

#[test]
fn job_listing_gather__should_report_an_invalid_config_without_a_limit() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let paths = JobPaths {
        config: dir.path().join("machine.toml"),
        folder: dir.path().join("jobs"),
    };
    std::fs::write(&paths.config, "[jobs]\nlimit = 0\n").unwrap();

    // When
    let listing = JobListing::gather(
        &paths,
        16,
        &RepositoryPatterns::default(),
        Vec::new(),
        &Table(Vec::new()),
        Timestamp::from_seconds(0),
    );

    // Then
    assert_eq!(listing.limit(), None);
    assert!(
        listing
            .error
            .as_deref()
            .is_some_and(|error| error.contains("limit = 0"))
    );
    assert!(listing.to_json().starts_with("{\"limit\":null,"));
}
