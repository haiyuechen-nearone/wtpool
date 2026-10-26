//! The background tasks of an actor (H27): which ones its calls started, and when each ended,
//! by its own events or by the list of work in flight another actor of its session sends. Pure
//! functions of the record and what the harness's adapter read from the event. The adapter
//! names the file each task's output goes to; the hook never opens it.

use super::clock;
use super::record::{Record, Task};
use crate::activity::TaskId;

/// Tasks kept per actor (H27).
pub const MAX_TASKS: usize = 5;

/// How long, in seconds, a task of another record of the session must have run before a list
/// that leaves it out ends it (H27): long enough that the list, made before the hook ran, was
/// made after the task started.
pub const LISTED_AFTER: i64 = 60;

/// A task an event started: its id, output file and command summary (H14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    pub id: TaskId,
    pub path: String,
    pub summary: String,
}

/// What one event says about the actor's background tasks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskEvent {
    pub started: Option<Started>,
    /// The ids the harness lists as still in flight: every other running task ended.
    pub running: Option<Vec<TaskId>>,
    /// A task a call stopped.
    pub stopped: Option<TaskId>,
    /// The session ended, and its tasks with it.
    pub all_ended: bool,
}

/// `record` after `event` at `now`: a started task joins the end, dropping the oldest past
/// [`MAX_TASKS`], unless its id is known; a running task ends when the event ends it.
pub fn update(record: &mut Record, event: &TaskEvent, now: &str) {
    if let Some(started) = &event.started
        && !record
            .tasks
            .iter()
            .any(|task| task.id == started.id.as_str())
    {
        record.tasks.push(Task {
            id: started.id.as_str().to_owned(),
            path: started.path.clone(),
            summary: started.summary.clone(),
            started: now.to_owned(),
            ended: None,
        });
        while record.tasks.len() > MAX_TASKS {
            record.tasks.remove(0);
        }
    }
    for task in record.tasks.iter_mut().filter(|task| task.ended.is_none()) {
        let ends = event.all_ended
            || event
                .stopped
                .as_ref()
                .is_some_and(|id| id.as_str() == task.id)
            || event
                .running
                .as_ref()
                .is_some_and(|ids| !lists(ids, &task.id));
        if ends {
            task.ended = Some(now.to_owned());
        }
    }
}

/// `record`, another record of the session whose harness lists `running` as its work in flight,
/// after that list at `now` (H27): each running task the list does not name ends when it started
/// at least [`LISTED_AFTER`] seconds before `now`. Whether a task ended. A task or a time that
/// does not parse never ends.
pub fn end_unlisted(record: &mut Record, running: &[TaskId], now: &str) -> bool {
    let Some(now_seconds) = clock::parse_seconds(now) else {
        return false;
    };
    let mut ended = false;
    for task in record.tasks.iter_mut().filter(|task| task.ended.is_none()) {
        let old = clock::parse_seconds(&task.started)
            .is_some_and(|started| now_seconds - started >= LISTED_AFTER);
        if old && !lists(running, &task.id) {
            task.ended = Some(now.to_owned());
            ended = true;
        }
    }
    ended
}

/// `true` when `ids` names the task `id`.
fn lists(ids: &[TaskId], id: &str) -> bool {
    ids.iter().any(|listed| listed.as_str() == id)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::super::store::tests::must_record;
    use super::*;

    const NOW: &str = "2026-10-20T12:05:00Z";

    fn task(id: &str, ended: Option<&str>) -> Task {
        Task {
            id: id.to_owned(),
            path: format!("/tmp/harness/s1/tasks/{id}.output"),
            summary: "cargo build".to_owned(),
            started: "2026-10-20T12:00:00Z".to_owned(),
            ended: ended.map(str::to_owned),
        }
    }

    fn task_id(id: &str) -> TaskId {
        TaskId::try_from(id).unwrap()
    }

    fn task_ids(ids: &[&str]) -> Vec<TaskId> {
        ids.iter().map(|id| task_id(id)).collect()
    }

    fn started(id: &str) -> TaskEvent {
        TaskEvent {
            started: Some(Started {
                id: task_id(id),
                path: format!("/tmp/harness/s1/tasks/{id}.output"),
                summary: "cargo build".to_owned(),
            }),
            ..TaskEvent::default()
        }
    }

    fn ids(record: &Record) -> Vec<(String, bool)> {
        record
            .tasks
            .iter()
            .map(|task| (task.id.clone(), task.ended.is_some()))
            .collect()
    }

    #[rstest]
    #[case::first(&[], "b1", &["b1"])]
    #[case::second(&["b1"], "b2", &["b1", "b2"])]
    #[case::known_id(&["b1", "b2"], "b1", &["b1", "b2"])]
    #[case::fifth(&["b1", "b2", "b3", "b4"], "b5", &["b1", "b2", "b3", "b4", "b5"])]
    #[case::sixth_drops_the_oldest(
        &["b1", "b2", "b3", "b4", "b5"],
        "b6",
        &["b2", "b3", "b4", "b5", "b6"]
    )]
    fn update__should_keep_the_last_five_tasks(
        #[case] before: &[&str],
        #[case] id: &str,
        #[case] after: &[&str],
    ) {
        // Given
        let mut record = must_record("s1");
        record.tasks = before.iter().map(|id| task(id, None)).collect();

        // When
        update(&mut record, &started(id), NOW);

        // Then
        let kept: Vec<&str> = record.tasks.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(kept, after);
        let new = record.tasks.iter().find(|task| task.id == id).unwrap();
        assert_eq!(new.ended, None);
        assert_eq!(new.summary, "cargo build");
    }

    #[test]
    fn update__should_start_a_task_at_the_event_time() {
        // Given
        let mut record = must_record("s1");

        // When
        update(&mut record, &started("b1"), NOW);

        // Then
        assert_eq!(
            record.tasks,
            vec![Task {
                started: NOW.to_owned(),
                ..task("b1", None)
            }]
        );
    }

    #[rstest]
    #[case::listed_keeps_running(
        TaskEvent { running: Some(task_ids(&["b1", "b2"])), ..TaskEvent::default() },
        &[("b1", false), ("b2", false), ("b0", true)]
    )]
    #[case::unlisted_ends(
        TaskEvent { running: Some(task_ids(&["b2"])), ..TaskEvent::default() },
        &[("b1", true), ("b2", false), ("b0", true)]
    )]
    #[case::empty_list_ends_all(
        TaskEvent { running: Some(Vec::new()), ..TaskEvent::default() },
        &[("b1", true), ("b2", true), ("b0", true)]
    )]
    #[case::no_list_ends_nothing(
        TaskEvent::default(),
        &[("b1", false), ("b2", false), ("b0", true)]
    )]
    #[case::stopped_ends_one(
        TaskEvent { stopped: Some(task_id("b2")), ..TaskEvent::default() },
        &[("b1", false), ("b2", true), ("b0", true)]
    )]
    #[case::stopped_unknown(
        TaskEvent { stopped: Some(task_id("b9")), ..TaskEvent::default() },
        &[("b1", false), ("b2", false), ("b0", true)]
    )]
    #[case::session_end_ends_all(
        TaskEvent { all_ended: true, ..TaskEvent::default() },
        &[("b1", true), ("b2", true), ("b0", true)]
    )]
    fn update__should_end_the_tasks_the_event_ends(
        #[case] event: TaskEvent,
        #[case] expected: &[(&str, bool)],
    ) {
        // Given
        let mut record = must_record("s1");
        record.tasks = vec![
            task("b1", None),
            task("b2", None),
            task("b0", Some("2026-10-20T12:01:00Z")),
        ];

        // When
        update(&mut record, &event, NOW);

        // Then
        let expected: Vec<(String, bool)> = expected
            .iter()
            .map(|(id, ended)| ((*id).to_owned(), *ended))
            .collect();
        assert_eq!(ids(&record), expected);
        for task in &record.tasks {
            match task.id.as_str() {
                "b0" => assert_eq!(task.ended.as_deref(), Some("2026-10-20T12:01:00Z")),
                _ if task.ended.is_some() => assert_eq!(task.ended.as_deref(), Some(NOW)),
                _ => {}
            }
        }
    }

    #[rstest]
    #[case::unlisted_and_old(&["b2"], "2026-10-20T12:01:00Z", &[("b1", true), ("b2", false), ("b0", true)], true)]
    #[case::listed(&["b1", "b2"], "2026-10-20T12:01:00Z", &[("b1", false), ("b2", false), ("b0", true)], false)]
    #[case::too_young(&[], "2026-10-20T12:00:59Z", &[("b1", false), ("b2", false), ("b0", true)], false)]
    #[case::empty_list_ends_all(&[], "2026-10-20T13:00:00Z", &[("b1", true), ("b2", true), ("b0", true)], true)]
    #[case::time_that_does_not_parse(&[], "noon", &[("b1", false), ("b2", false), ("b0", true)], false)]
    fn end_unlisted__should_end_old_tasks_the_list_leaves_out(
        #[case] running: &[&str],
        #[case] now: &str,
        #[case] expected: &[(&str, bool)],
        #[case] changed: bool,
    ) {
        // Given
        let mut record = must_record("s1.a1");
        record.tasks = vec![
            task("b1", None),
            task("b2", None),
            task("b0", Some("2026-10-20T12:00:30Z")),
        ];
        let before = record.clone();
        let running = task_ids(running);

        // When
        let ended = end_unlisted(&mut record, &running, now);

        // Then
        assert_eq!(ended, changed);
        let expected: Vec<(String, bool)> = expected
            .iter()
            .map(|(id, ended)| ((*id).to_owned(), *ended))
            .collect();
        assert_eq!(ids(&record), expected);
        for task in &record.tasks {
            match task.id.as_str() {
                "b0" => assert_eq!(task.ended.as_deref(), Some("2026-10-20T12:00:30Z")),
                _ if task.ended.is_some() => assert_eq!(task.ended.as_deref(), Some(now)),
                _ => {}
            }
        }
        let others = Record {
            tasks: before.tasks.clone(),
            ..record.clone()
        };
        assert_eq!(others, before, "only tasks change");
    }

    #[test]
    fn end_unlisted__should_keep_a_task_whose_start_does_not_parse() {
        // Given
        let mut record = must_record("s1.a1");
        record.tasks = vec![Task {
            started: "noon".to_owned(),
            ..task("b1", None)
        }];

        // When
        let ended = end_unlisted(&mut record, &[], "2026-10-20T13:00:00Z");

        // Then
        assert!(!ended);
        assert_eq!(record.tasks[0].ended, None);
    }
}
