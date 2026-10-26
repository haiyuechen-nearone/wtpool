//! `wtpool setup`.

use wtpool_core::Role;

use crate::support::{Fixture, Recorder};

#[test]
fn setup__should_fill_the_pool_to_the_slot_count() {
    // Given
    let fixture = Fixture::must_new();
    let pool = fixture.pool(2);
    let mut report = Recorder::default();

    // When
    let rows = pool.setup(&mut report).unwrap();

    // Then
    assert_eq!(report.out, "created app-wt1\ncreated app-wt2\n\n");
    assert_eq!(
        rows.iter()
            .map(|r| (r.worktree.as_str(), r.role))
            .collect::<Vec<(&str, Role)>>(),
        [
            ("app", Role::Foreground),
            ("app-wt1", Role::Slot),
            ("app-wt2", Role::Slot)
        ]
    );
}

#[test]
fn setup__should_warn_once_about_a_slot_whose_directory_is_gone() {
    // Given
    let fixture = Fixture::must_new();
    let gone = fixture.must_add_worktree("app-wt1", None);
    std::fs::remove_dir_all(gone).unwrap();
    let pool = fixture.pool(1);
    let mut report = Recorder::default();

    // When
    pool.setup(&mut report).unwrap();

    // Then
    assert_eq!(
        report.warnings,
        ["app-wt1 is registered as a worktree but its directory is gone; run 'wtpool rm app-wt1'"]
    );
    assert!(fixture.slot(2).exists());
}
