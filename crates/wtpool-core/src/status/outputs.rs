//! The background tasks of a record (H27) and the outputs its actor lists (S13): the running
//! tasks, newest first, then the one that ended last. wtpool lists where each task's output goes,
//! never the output; a consumer reads the file on its own machine.

use std::path::{Component, Path};

use serde::Deserialize;

use super::document::Output;
use super::record_time::RecordTime;

/// A task as a record holds it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TaskFields {
    id: String,
    path: String,
    summary: String,
    started: String,
    ended: Option<String>,
}

/// A background task of a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub path: String,
    pub summary: String,
    pub started: RecordTime,
    /// `None` while it runs.
    pub ended: Option<RecordTime>,
}

/// The tasks of a record with an id, an absolute path of only normal components and times that
/// parse; the others are skipped.
#[must_use]
pub fn tasks(fields: Vec<TaskFields>) -> Vec<Task> {
    fields
        .into_iter()
        .filter(|task| !task.id.is_empty() && is_plain_absolute(Path::new(&task.path)))
        .filter_map(|task| {
            let ended = match task.ended.as_deref() {
                Some(ended) => Some(RecordTime::parse(ended)?),
                None => None,
            };
            Some(Task {
                started: RecordTime::parse(&task.started)?,
                ended,
                id: task.id,
                path: task.path,
                summary: task.summary,
            })
        })
        .collect()
}

/// What an actor lists (S13): its running tasks, newest start first, then the task that ended
/// last. Among equal times the later task of the record comes first.
#[must_use]
pub fn outputs(tasks: &[Task]) -> Vec<Output> {
    let mut running: Vec<&Task> = tasks.iter().rev().filter(|t| t.ended.is_none()).collect();
    running.sort_by_key(|task| std::cmp::Reverse(task.started.at()));
    let last_ended = tasks
        .iter()
        .filter_map(|task| Some((task.ended.as_ref()?.at(), task)))
        .max_by_key(|(ended, _)| *ended)
        .map(|(_, task)| task);
    running.into_iter().chain(last_ended).map(output).collect()
}

fn output(task: &Task) -> Output {
    Output {
        task: task.id.clone(),
        path: task.path.clone(),
        summary: task.summary.clone(),
        running: task.ended.is_none(),
        started: task.started.clone(),
        ended: task.ended.clone(),
    }
}

/// Whether `path` is absolute with only normal components after its root: no `.` or `..`.
fn is_plain_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn fields(id: &str, path: &str, started: &str, ended: Option<&str>) -> TaskFields {
        TaskFields {
            id: id.to_owned(),
            path: path.to_owned(),
            summary: "cargo build".to_owned(),
            started: started.to_owned(),
            ended: ended.map(str::to_owned),
        }
    }

    fn at(minute: u32) -> String {
        format!("2026-10-20T12:{minute:02}:00Z")
    }

    fn task(id: &str, started: u32, ended: Option<u32>) -> Task {
        tasks(vec![fields(
            id,
            &format!("/t/claude-501/p/s1/tasks/{id}.output"),
            &at(started),
            ended.map(at).as_deref(),
        )])
        .remove(0)
    }

    #[rstest]
    #[case::kept(fields("b1", "/t/b1.output", "2026-10-20T12:00:00Z", None), true)]
    #[case::ended(
        fields(
            "b1",
            "/t/b1.output",
            "2026-10-20T12:00:00Z",
            Some("2026-10-20T12:01:00Z")
        ),
        true
    )]
    #[case::no_id(fields("", "/t/b1.output", "2026-10-20T12:00:00Z", None), false)]
    #[case::relative(fields("b1", "t/b1.output", "2026-10-20T12:00:00Z", None), false)]
    #[case::dot_dot(fields("b1", "/t/../etc/passwd", "2026-10-20T12:00:00Z", None), false)]
    #[case::empty_path(fields("b1", "", "2026-10-20T12:00:00Z", None), false)]
    #[case::bad_start(fields("b1", "/t/b1.output", "noon", None), false)]
    #[case::bad_end(
        fields("b1", "/t/b1.output", "2026-10-20T12:00:00Z", Some("later")),
        false
    )]
    fn tasks__should_skip_bad_paths_and_times(#[case] task: TaskFields, #[case] kept: bool) {
        // When
        let found = tasks(vec![task]);

        // Then
        assert_eq!(found.len(), usize::from(kept));
    }

    #[test]
    fn tasks__should_read_a_record_without_some_fields() {
        // When
        let parsed: TaskFields = serde_json::from_str(r#"{"id":"b1"}"#).unwrap();

        // Then
        assert!(tasks(vec![parsed]).is_empty());
    }

    #[rstest]
    #[case::none(vec![], &[])]
    #[case::running_newest_first(
        vec![task("b1", 0, None), task("b2", 5, None), task("b3", 3, None)],
        &[("b2", true), ("b3", true), ("b1", true)]
    )]
    #[case::then_the_last_ended(
        vec![task("b1", 0, Some(9)), task("b2", 1, Some(4)), task("b3", 2, None)],
        &[("b3", true), ("b1", false)]
    )]
    #[case::only_ended(
        vec![task("b1", 0, Some(2)), task("b2", 1, Some(3))],
        &[("b2", false)]
    )]
    #[case::equal_times_later_first(
        vec![task("b1", 0, Some(5)), task("b2", 0, Some(5)), task("b3", 1, None), task("b4", 1, None)],
        &[("b4", true), ("b3", true), ("b2", false)]
    )]
    fn outputs__should_list_running_newest_first_then_the_last_ended(
        #[case] tasks: Vec<Task>,
        #[case] expected: &[(&str, bool)],
    ) {
        // When
        let listed = outputs(&tasks);

        // Then
        let found: Vec<(&str, bool)> = listed
            .iter()
            .map(|o| (o.task.as_str(), o.running))
            .collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn outputs__should_carry_the_task_facts() {
        // Given
        let ended = task("b1", 0, Some(2));

        // When
        let listed = outputs(std::slice::from_ref(&ended));

        // Then
        assert_eq!(
            listed,
            vec![Output {
                task: "b1".to_owned(),
                path: "/t/claude-501/p/s1/tasks/b1.output".to_owned(),
                summary: "cargo build".to_owned(),
                running: false,
                started: ended.started.clone(),
                ended: ended.ended.clone(),
            }]
        );
    }
}
