//! The background tasks of an actor: the ones its calls started, and the time each ended, by its
//! own events or by the list of work in flight another actor of its session sends. Pure functions
//! of the record and the facts the harness's adapter read from the event. The adapter names the
//! file each task's output goes to, and the hook never opens it.

use super::record::{Record, Task, push_bounded};
use crate::activity::TaskId;
use crate::clock::Timestamp;

/// Tasks kept per actor.
pub const MAX_TASKS: usize = 5;

/// The age, in seconds, a task of another record of the session must reach before a list that
/// leaves it out ends it: long enough that the list, made before the hook ran, was made after
/// the task started.
pub const LISTED_AFTER: i64 = 60;

/// A task an event started: its id, output file and command summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    pub id: TaskId,
    pub path: String,
    pub summary: String,
}

/// The facts one event gives about the actor's background tasks.
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

/// Applies `event` at `now` to `record`. A started task joins the end, dropping the oldest past
/// [`MAX_TASKS`], unless its id is known. A running task ends when the event ends it.
pub fn update(record: &mut Record, event: &TaskEvent, now: Timestamp) {
    if let Some(started) = &event.started
        && !record.tasks.iter().any(|task| task.id == started.id)
    {
        let task = Task {
            id: started.id.clone(),
            path: started.path.clone(),
            summary: started.summary.clone(),
            started: now,
            ended: None,
        };
        push_bounded(&mut record.tasks, task, MAX_TASKS);
    }
    for task in record.tasks.iter_mut().filter(|task| task.ended.is_none()) {
        let ends = event.all_ended
            || event.stopped.as_ref() == Some(&task.id)
            || event
                .running
                .as_ref()
                .is_some_and(|ids| !ids.contains(&task.id));
        if ends {
            task.ended = Some(now);
        }
    }
}

/// Applies `running`, the session's work in flight that another actor's harness lists at `now`,
/// to `record`: each running task the list does not name ends when it started at least
/// [`LISTED_AFTER`] seconds before `now`. `true` when a task ended.
pub fn end_unlisted(record: &mut Record, running: &[TaskId], now: Timestamp) -> bool {
    let mut ended = false;
    for task in record.tasks.iter_mut().filter(|task| task.ended.is_none()) {
        let old = now.seconds_since(task.started) >= LISTED_AFTER;
        if old && !running.contains(&task.id) {
            task.ended = Some(now);
            ended = true;
        }
    }
    ended
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::activity::ActorId;
    use crate::hook::fixture::{self, later};

    /// Five minutes after every task in these tests started.
    fn now() -> Timestamp {
        later(300)
    }

    fn task(id: &str, ended: Option<Timestamp>) -> Task {
        Task {
            id: task_id(id),
            path: format!("/tmp/harness/s1/tasks/{id}.output"),
            summary: "cargo build".to_owned(),
            started: fixture::NOW,
            ended,
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

    fn ends(record: &Record) -> Vec<(&str, Option<Timestamp>)> {
        record
            .tasks
            .iter()
            .map(|task| (task.id.as_str(), task.ended))
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
        let mut record = fixture::record("s1");
        record.tasks = before.iter().map(|id| task(id, None)).collect();

        // When
        update(&mut record, &started(id), now());

        // Then
        let kept: Vec<&str> = record.tasks.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(kept, after);
        let new = record
            .tasks
            .iter()
            .find(|task| task.id.as_str() == id)
            .unwrap();
        assert_eq!(new.ended, None);
        assert_eq!(new.summary, "cargo build");
    }

    #[test]
    fn update__should_start_a_task_at_the_event_time() {
        // Given
        let mut record = fixture::record("s1");

        // When
        update(&mut record, &started("b1"), now());

        // Then
        assert_eq!(
            record.tasks,
            vec![Task {
                started: now(),
                ..task("b1", None)
            }]
        );
    }

    #[rstest]
    #[case::listed_keeps_running(
        TaskEvent { running: Some(task_ids(&["b1", "b2"])), ..TaskEvent::default() },
        &[("b1", None), ("b2", None), ("b0", Some(later(60)))]
    )]
    #[case::unlisted_ends(
        TaskEvent { running: Some(task_ids(&["b2"])), ..TaskEvent::default() },
        &[("b1", Some(now())), ("b2", None), ("b0", Some(later(60)))]
    )]
    #[case::empty_list_ends_all(
        TaskEvent { running: Some(Vec::new()), ..TaskEvent::default() },
        &[("b1", Some(now())), ("b2", Some(now())), ("b0", Some(later(60)))]
    )]
    #[case::no_list_ends_nothing(
        TaskEvent::default(),
        &[("b1", None), ("b2", None), ("b0", Some(later(60)))]
    )]
    #[case::stopped_ends_one(
        TaskEvent { stopped: Some(task_id("b2")), ..TaskEvent::default() },
        &[("b1", None), ("b2", Some(now())), ("b0", Some(later(60)))]
    )]
    #[case::stopped_unknown(
        TaskEvent { stopped: Some(task_id("b9")), ..TaskEvent::default() },
        &[("b1", None), ("b2", None), ("b0", Some(later(60)))]
    )]
    #[case::session_end_ends_all(
        TaskEvent { all_ended: true, ..TaskEvent::default() },
        &[("b1", Some(now())), ("b2", Some(now())), ("b0", Some(later(60)))]
    )]
    fn update__should_end_the_tasks_the_event_ends(
        #[case] event: TaskEvent,
        #[case] expected: &[(&str, Option<Timestamp>)],
    ) {
        // Given
        let mut record = fixture::record("s1");
        record.tasks = vec![
            task("b1", None),
            task("b2", None),
            task("b0", Some(later(60))),
        ];

        // When
        update(&mut record, &event, now());

        // Then
        assert_eq!(ends(&record), expected);
    }

    #[rstest]
    #[case::unlisted_and_old(
        &["b2"],
        later(60),
        &[("b1", Some(later(60))), ("b2", None), ("b0", Some(later(30)))],
        true
    )]
    #[case::listed(
        &["b1", "b2"],
        later(60),
        &[("b1", None), ("b2", None), ("b0", Some(later(30)))],
        false
    )]
    #[case::too_young(
        &[],
        later(59),
        &[("b1", None), ("b2", None), ("b0", Some(later(30)))],
        false
    )]
    #[case::empty_list_ends_all(
        &[],
        later(3600),
        &[
            ("b1", Some(later(3600))),
            ("b2", Some(later(3600))),
            ("b0", Some(later(30))),
        ],
        true
    )]
    fn end_unlisted__should_end_old_tasks_the_list_leaves_out(
        #[case] running: &[&str],
        #[case] now: Timestamp,
        #[case] expected: &[(&str, Option<Timestamp>)],
        #[case] changed: bool,
    ) {
        // Given
        let mut record = fixture::record("s1");
        record.agent_id = Some(ActorId::try_from("a1").unwrap());
        record.tasks = vec![
            task("b1", None),
            task("b2", None),
            task("b0", Some(later(30))),
        ];
        let before = record.clone();
        let running = task_ids(running);

        // When
        let ended = end_unlisted(&mut record, &running, now);

        // Then
        assert_eq!(ended, changed);
        assert_eq!(ends(&record), expected);
        let others = Record {
            tasks: before.tasks.clone(),
            ..record.clone()
        };
        assert_eq!(others, before, "only tasks change");
    }
}
