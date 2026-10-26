//! The titles of a session and its subagents, and its label.

use rstest::rstest;

use super::*;

fn titles(user: Option<&str>, generated: Option<&str>) -> Titles {
    Titles {
        user: user.map(str::to_owned),
        generated: generated.map(str::to_owned),
    }
}

#[rstest]
#[case::none(&[], "session 12:00", None)]
#[case::generated(&[titles(None, Some("Bind"))], "Bind", Some(TitleBy::Harness))]
#[case::generated_again(&[titles(None, Some("Bind")), titles(None, Some("Bound"))], "Bound", Some(TitleBy::Harness))]
#[case::given(&[titles(None, Some("Bind")), titles(Some("Mine"), None)], "Mine", Some(TitleBy::User))]
#[case::given_over_generated_at_once(&[titles(Some("Mine"), Some("Bind"))], "Mine", Some(TitleBy::User))]
#[case::generated_after_given(&[titles(Some("Mine"), None), titles(None, Some("Bind"))], "Mine", Some(TitleBy::User))]
#[case::given_again(&[titles(Some("Mine"), None), titles(Some("Ours"), None)], "Ours", Some(TitleBy::User))]
#[case::nothing_found_keeps(&[titles(None, Some("Bind")), titles(None, None)], "Bind", Some(TitleBy::Harness))]
fn apply__should_label_a_session_with_its_title(
    #[case] found: &[Titles],
    #[case] label: &str,
    #[case] by: Option<TitleBy>,
) {
    // Given
    let mut record = apply(None, &Event::TurnStart, &change("2026-10-20T12:00:00Z"));

    // When
    for titles in found {
        let mut next = change("2026-10-20T12:00:01Z");
        next.found.titles = titles.clone();
        record = apply(Some(&record), &Event::TurnEnd(None), &next);
    }

    // Then
    assert_eq!(record.label, label);
    assert_eq!(record.title.map(|title| title.by), by);
}

#[test]
fn apply__should_keep_the_transcript_offset_until_a_read_moves_it() {
    // Given
    let mut read = change("2026-10-20T12:00:00Z");
    read.found.transcript_offset = Some(4096);
    let first = apply(None, &Event::TurnStart, &read);

    // When
    let unread = apply(
        Some(&first),
        &Event::TurnEnd(None),
        &change("2026-10-20T12:00:01Z"),
    );

    // Then
    assert_eq!(
        (first.transcript_offset, unread.transcript_offset),
        (4096, 4096)
    );
}

#[test]
fn apply__should_keep_the_newest_16_subagent_titles() {
    // Given
    let mut record = apply(None, &Event::TurnStart, &change("2026-10-20T12:00:00Z"));

    // When
    for n in 0..20 {
        let mut next = change("2026-10-20T12:00:01Z");
        next.found.subagent = Some(subagent(&format!("a{n}"), &format!("Task {n}")));
        record = apply(
            Some(&record),
            &Event::CallEnd(call("c", "spawn", "h")),
            &next,
        );
    }
    let mut again = change("2026-10-20T12:00:02Z");
    again.found.subagent = Some(subagent("a10", "Task ten"));
    record = apply(
        Some(&record),
        &Event::CallEnd(call("c", "spawn", "h")),
        &again,
    );

    // Then
    let ids: Vec<&str> = record
        .subagents
        .iter()
        .map(|known| known.agent_id.as_str())
        .collect();
    assert_eq!(ids.len(), MAX_SUBAGENTS);
    assert_eq!(ids.first(), Some(&"a4"));
    assert_eq!(record.subagents.last(), Some(&subagent("a10", "Task ten")));
}
