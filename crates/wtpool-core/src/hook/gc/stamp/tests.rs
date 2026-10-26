use std::sync::{Arc, Barrier};

use rstest::rstest;

use super::*;
use crate::hook::fixture::{NOW, eventually, later, must_activity};

fn stamp_text(activity: &Activity) -> Option<String> {
    std::fs::read_to_string(activity.file_path(STAMP)).ok()
}

/// The stamp a claim at [`NOW`] writes.
fn claimed_now() -> Option<String> {
    Some(format!("{}\n", NOW.to_rfc3339()))
}

#[rstest]
#[case::missing(None, true, claimed_now())]
#[case::an_hour_ago(
    Some(later(-3600).to_rfc3339()),
    false,
    Some(later(-3600).to_rfc3339())
)]
#[case::a_second_short_of_a_day(
    Some(later(1 - DAY).to_rfc3339()),
    false,
    Some(later(1 - DAY).to_rfc3339())
)]
#[case::a_day_ago(Some(later(-DAY).to_rfc3339()), true, claimed_now())]
#[case::a_week_ago(Some(later(-(7 * DAY)).to_rfc3339()), true, claimed_now())]
#[case::now(Some(NOW.to_rfc3339()), false, Some(NOW.to_rfc3339()))]
#[case::later_than_now(Some(later(60).to_rfc3339()), true, claimed_now())]
#[case::garbage(Some("soon".to_owned()), true, claimed_now())]
#[case::too_long(
    Some(format!("{}{}", NOW.to_rfc3339(), " ".repeat(64))),
    true,
    claimed_now()
)]
fn claim__should_run_once_a_day(
    #[case] stamp: Option<String>,
    #[case] due: bool,
    #[case] expected: Option<String>,
) {
    // Given
    let (_dir, activity) = must_activity();
    if let Some(text) = &stamp {
        std::fs::write(activity.file_path(STAMP), text).unwrap();
    }

    // When
    let claimed = claim(&activity, NOW);

    // Then
    assert_eq!(claimed, due);
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
    assert!(!claim(&activity, later(DAY - 1)));
}

#[test]
fn mark_and_clear__should_set_and_remove_the_stamp() {
    // Given
    let (_dir, activity) = must_activity();
    std::fs::write(activity.file_path(STAMP), later(-(2 * DAY)).to_rfc3339()).unwrap();

    // When
    mark(&activity, NOW);
    let marked = stamp_text(&activity);
    let fresh = claim(&activity, later(60));
    clear(&activity);
    let cleared = stamp_text(&activity);

    // Then
    assert_eq!(marked, Some(format!("{}\n", NOW.to_rfc3339())));
    assert!(!fresh);
    assert_eq!(cleared, None);
    assert!(claim(&activity, later(60)));
}
