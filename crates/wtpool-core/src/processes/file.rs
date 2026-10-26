//! [`ProcessFile`]: the process table a `verify` build reads instead of the system's
//! (`WTPOOL_VERIFY_PROCS`).

use std::path::PathBuf;

use serde::Deserialize;

use super::{ListProcesses, ProcessQuery, RunningProcess, StartTime};

/// A process table read from a file of JSON lines, read again on every listing. Each line holds
/// a `pid` and an optional `ppid` and `started`, and needs a `name` and a `cwd` for a query that
/// asks for them. Lines that do not parse are left out.
#[derive(Debug, Clone)]
pub struct ProcessFile {
    path: PathBuf,
}

/// The fields of every line.
#[derive(Debug, Deserialize)]
struct Line {
    pid: u32,
    #[serde(default)]
    ppid: Option<u32>,
    #[serde(default)]
    started: Option<String>,
}

/// The fields a line needs only for the queries that ask for them, read only for those, so a
/// line without them still lists for the others.
#[derive(Debug, Default, Deserialize)]
struct Asked {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    cwd: Option<PathBuf>,
}

impl ProcessFile {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ListProcesses for ProcessFile {
    fn list(&self, query: &ProcessQuery<'_>) -> Vec<RunningProcess> {
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        text.lines()
            .filter_map(|line| process(line, query))
            .collect()
    }
}

/// The process the line `text` lists, with the facts `query` asks for.
fn process(text: &str, query: &ProcessQuery<'_>) -> Option<RunningProcess> {
    let line: Line = serde_json::from_str(text).ok()?;
    if query.pids.is_some_and(|pids| !pids.contains(&line.pid)) {
        return None;
    }
    let asked: Asked = if query.name || query.cwd {
        serde_json::from_str(text).ok()?
    } else {
        Asked::default()
    };
    Some(RunningProcess {
        pid: line.pid,
        parent: line.ppid.filter(|_| query.parent),
        started: line
            .started
            .filter(|_| query.started)
            .as_deref()
            .and_then(StartTime::parse),
        name: if query.name {
            asked.name?
        } else {
            String::new()
        },
        cwd: if query.cwd {
            asked.cwd?
        } else {
            PathBuf::new()
        },
    })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const LINES: &str = "{\"pid\":7,\"ppid\":1,\"started\":\"2026-10-20T11:00:00.5Z\",\"name\":\"cargo\",\"cwd\":\"/p/wt1\"}\n\
         not json\n\
         {\"pid\":8,\"name\":\"rustc\",\"cwd\":\"/p\"}\n\
         {\"pid\":9,\"ppid\":7,\"name\":5}\n\
         {\"pid\":10,\"ppid\":7,\"name\":\"zsh\"}\n";

    fn must_file(text: &str) -> (tempfile::TempDir, ProcessFile) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("procs.jsonl");
        std::fs::write(&path, text).unwrap();
        (dir, ProcessFile::new(path))
    }

    fn pids(processes: &[RunningProcess]) -> Vec<u32> {
        processes.iter().map(|process| process.pid).collect()
    }

    #[rstest]
    #[case::pids_alone(ProcessQuery::all(), vec![7, 8, 9, 10])]
    #[case::named(ProcessQuery::all().with_name(), vec![7, 8, 10])]
    #[case::named_with_folders(ProcessQuery::all().with_name().with_cwd(), vec![7, 8])]
    #[case::some_pids(ProcessQuery::only(&[8, 9, 11]), vec![8, 9])]
    fn process_file__should_list_the_lines_with_what_the_query_asks_for(
        #[case] query: ProcessQuery<'static>,
        #[case] expected: Vec<u32>,
    ) {
        // Given
        let (_dir, file) = must_file(LINES);

        // When
        let processes = file.list(&query);

        // Then
        assert_eq!(pids(&processes), expected);
    }

    #[test]
    fn process_file__should_give_only_the_facts_asked_for() {
        // Given
        let (_dir, file) = must_file(LINES);
        let everything = ProcessQuery::only(&[7])
            .with_parent()
            .with_started()
            .with_name()
            .with_cwd();

        // When
        let full = file.list(&everything);
        let bare = file.list(&ProcessQuery::only(&[7]));

        // Then
        let expected = RunningProcess {
            pid: 7,
            parent: Some(1),
            started: StartTime::parse("2026-10-20T11:00:00.5Z"),
            name: "cargo".to_owned(),
            cwd: PathBuf::from("/p/wt1"),
        };
        let nothing = RunningProcess {
            pid: 7,
            parent: None,
            started: None,
            name: String::new(),
            cwd: PathBuf::new(),
        };
        assert_eq!((full, bare), (vec![expected], vec![nothing]));
    }

    #[test]
    fn process_file__should_list_nothing_without_the_file() {
        // Given
        let dir = tempfile::tempdir().unwrap();

        // When
        let processes = ProcessFile::new(dir.path().join("none")).list(&ProcessQuery::all());

        // Then
        assert!(processes.is_empty());
    }

    #[test]
    fn this_process__should_read_its_last_line() {
        // Given
        let own = |started: &str| {
            format!(
                r#"{{"pid":{},"ppid":7001,"started":"{started}"}}"#,
                std::process::id()
            )
        };
        let text = [
            own("2026-10-20T12:00:00Z"),
            own("2026-10-20T12:00:00.000005Z"),
        ];
        let (_dir, file) = must_file(&text.join("\n"));

        // When
        let this = file.this_process().unwrap();

        // Then
        assert_eq!(this.parent, Some(7001));
        assert_eq!(
            this.started.map(StartTime::micros),
            Some(1_792_497_600_000_005)
        );
    }
}
