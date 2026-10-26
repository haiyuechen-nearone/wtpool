use std::sync::{Arc, Barrier};

use rstest::rstest;

use super::super::super::store::tests::eventually;
use super::*;

const NOW: i64 = 1_792_497_600;

fn must_activity() -> (tempfile::TempDir, Activity) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("wtpool-activity")).unwrap();
    let activity = Activity::open(dir.path()).unwrap();
    (dir, activity)
}

fn stamp_text(activity: &Activity) -> Option<String> {
    std::fs::read_to_string(activity.file_path(STAMP)).ok()
}

#[rstest]
#[case::missing(None, true)]
#[case::an_hour_ago(Some(clock::rfc3339(NOW - 3600)), false)]
#[case::a_second_short_of_a_day(Some(clock::rfc3339(NOW - DAY + 1)), false)]
#[case::a_day_ago(Some(clock::rfc3339(NOW - DAY)), true)]
#[case::a_week_ago(Some(clock::rfc3339(NOW - 7 * DAY)), true)]
#[case::now(Some(clock::rfc3339(NOW)), false)]
#[case::later_than_now(Some(clock::rfc3339(NOW + 60)), true)]
#[case::garbage(Some("soon".to_owned()), true)]
#[case::too_long(Some(format!("{}{}", clock::rfc3339(NOW), " ".repeat(64))), true)]
fn claim__should_run_once_a_day(#[case] stamp: Option<String>, #[case] due: bool) {
    // Given
    let (_dir, activity) = must_activity();
    if let Some(text) = &stamp {
        std::fs::write(activity.file_path(STAMP), text).unwrap();
    }

    // When
    let claimed = claim(&activity, NOW);

    // Then
    assert_eq!(claimed, due);
    let expected = if due {
        Some(format!("{}\n", clock::rfc3339(NOW)))
    } else {
        stamp
    };
    assert_eq!(stamp_text(&activity), expected);
}

#[test]
fn claim__should_not_wait_for_a_held_lock() {
    // Given
    let (_dir, activity) = must_activity();
    let holder = activity.open_file(STAMP).unwrap();
    holder.lock().unwrap();

    // When
    let while_held = claim(&activity, NOW);
    mark(&activity, NOW);
    let marked = stamp_text(&activity);
    drop(holder);
    let after = eventually(|| claim(&activity, NOW));

    // Then
    assert!(!while_held, "a held stamp gave the gc to a second process");
    assert_eq!(marked.as_deref(), Some(""), "mark wrote a held stamp");
    assert!(after, "the stamp stayed claimed after its holder let go");
}

#[test]
fn claim__should_give_the_day_to_one_claimant_of_many() {
    // Given
    let (_dir, activity) = must_activity();
    let barrier = Arc::new(Barrier::new(8));

    // When
    let claimants: Vec<_> = (0..8)
        .map(|_| {
            let (activity, barrier) = (activity.clone(), Arc::clone(&barrier));
            std::thread::spawn(move || {
                barrier.wait();
                claim(&activity, NOW)
            })
        })
        .collect();
    let won = claimants
        .into_iter()
        .map(|claimant| claimant.join().unwrap())
        .filter(|claimed| *claimed)
        .count();

    // Then
    assert_eq!(won, 1);
    assert!(!claim(&activity, NOW + DAY - 1));
}

#[test]
fn mark_and_clear__should_set_and_remove_the_stamp() {
    // Given
    let (_dir, activity) = must_activity();
    std::fs::write(activity.file_path(STAMP), clock::rfc3339(NOW - 2 * DAY)).unwrap();

    // When
    mark(&activity, NOW);
    let marked = stamp_text(&activity);
    let fresh = claim(&activity, NOW + 60);
    clear(&activity);
    let cleared = stamp_text(&activity);

    // Then
    assert_eq!(marked, Some(format!("{}\n", clock::rfc3339(NOW))));
    assert!(!fresh);
    assert_eq!(cleared, None);
    assert!(claim(&activity, NOW + 60));
}
