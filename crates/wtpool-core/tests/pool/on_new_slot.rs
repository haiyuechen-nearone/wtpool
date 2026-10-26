//! The `on_new_slot` command `wtpool` runs in the slots it creates.

use wtpool_core::SyncMode;

use crate::support::{Fixture, Recorder};

#[test]
fn new_slot__should_run_on_new_slot_once_after_the_copies_with_its_output_on_stderr() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_ignore(".envrc\n");
    fixture.must_write(".envrc", "use flake\n");
    let pool = fixture.pool_with_setup(
        3,
        &[".envrc"],
        Some("pwd >> ../ran; cat .envrc; echo to-stderr >&2"),
    );
    let mut report = Recorder::default();

    // When
    let slot = pool.new_slot(None, &mut report).unwrap();

    // Then
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("ran")).unwrap(),
        format!("{}\n", slot.display())
    );
    assert_eq!(report.out, format!("created {}\n", slot.display()));
    assert!(
        report.err.ends_with("use flake\nto-stderr\n"),
        "{}",
        report.err
    );
}

#[test]
fn background__should_park_the_branch_then_fail_when_on_new_slot_fails() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    let pool = fixture.pool_with_setup(3, &[], Some("echo no >&2; exit 3"));
    let mut report = Recorder::default();

    // When
    let result = pool.background("feature", SyncMode::No, &mut report);

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            format!(
                "on_new_slot 'echo no >&2; exit 3' failed in {}: exit status 3",
                fixture.slot(1).display()
            ),
            "copy or on_new_slot failed as shown above; the slots were kept".to_owned()
        ]
    );
    assert_eq!(
        fixture.must_head_branch(&fixture.slot(1)).as_deref(),
        Some("feature")
    );
    assert!(report.err.contains("no\n"), "{}", report.err);
}

#[test]
fn setup__should_create_every_slot_and_report_each_failure_of_on_new_slot() {
    // Given
    let fixture = Fixture::must_new();
    let pool = fixture.pool_with_setup(2, &[], Some("kill -9 $$"));
    let mut report = Recorder::default();

    // When
    let result = pool.setup(&mut report);

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            format!(
                "on_new_slot 'kill -9 $$' failed in {}: killed by a signal",
                fixture.slot(1).display()
            ),
            format!(
                "on_new_slot 'kill -9 $$' failed in {}: killed by a signal",
                fixture.slot(2).display()
            ),
            "copy or on_new_slot failed as shown above; the slots were kept".to_owned()
        ]
    );
    assert!(fixture.must_registered(&fixture.slot(1)));
    assert!(fixture.must_registered(&fixture.slot(2)));
    assert_eq!(report.out, "created app-wt1\ncreated app-wt2\n\n");
}

#[test]
fn background__should_not_run_on_new_slot_in_a_slot_it_reuses() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    fixture.must_add_worktree("app-wt1", None);
    let pool = fixture.pool_with_setup(3, &[], Some("exit 1"));

    // When
    let slot = pool
        .background("feature", SyncMode::No, &mut Recorder::default())
        .unwrap();

    // Then
    assert_eq!(slot, fixture.slot(1));
}
