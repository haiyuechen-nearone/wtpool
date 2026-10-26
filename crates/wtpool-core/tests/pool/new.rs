//! `wtpool new`, and the slots every command creates.

use std::path::PathBuf;

use rstest::rstest;
use wtpool_core::{AbsolutePath, Config, Pool, SlotName, Source, SyncMode};

use crate::support::{FakePr, Fixture, PoolCommand, PrAnswer, Recorder};

#[rstest]
#[case::next_free_number_above_the_count(
    None,
    1,
    "app-wt2",
    &["the pool now has 2 slots, above slot_count=1 in /r/.git/wtpool.toml"]
)]
#[case::next_free_number_at_the_count(None, 2, "app-wt2", &[])]
#[case::given_name_below_the_count(Some("app-wt7"), 3, "app-wt7", &[])]
fn new_slot__should_create_a_detached_slot(
    #[case] name: Option<&str>,
    #[case] slot_count: usize,
    #[case] created: &str,
    #[case] warnings: &[&str],
) {
    // Given
    let fixture = Fixture::must_new();
    std::fs::create_dir(fixture.slot(1)).unwrap();
    fixture.must_add_worktree("app-wt3", None);
    let pool = Pool::with_view_pr(
        Config {
            slot_count_source: Source::RepoFile(PathBuf::from("/r/.git/wtpool.toml")),
            ..fixture.config(slot_count)
        },
        FakePr(PrAnswer::NoLookup),
    );
    let mut report = Recorder::default();

    // When
    let path = pool
        .new_slot(name.map(|n| SlotName::try_new(n).unwrap()), &mut report)
        .unwrap();

    // Then
    assert_eq!(path, fixture.root.join(created));
    assert_eq!(fixture.must_head_branch(&path), None);
    assert_eq!(report.out, format!("created {}\n", path.display()));
    assert_eq!(report.warnings, warnings);
}

#[rstest]
#[case::default(Source::Default, "the default slot_count=1")]
#[case::variable(Source::Env("WTPOOL_SLOT_COUNT"), "WTPOOL_SLOT_COUNT=1")]
#[case::file(
    Source::RepoFile(PathBuf::from("/r/.git/wtpool.toml")),
    "slot_count=1 in /r/.git/wtpool.toml"
)]
fn new_slot__should_name_where_the_slot_count_came_from(
    #[case] source: Source,
    #[case] limit: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_add_worktree("app-wt1", None);
    let pool = Pool::with_view_pr(
        Config {
            slot_count_source: source,
            ..fixture.config(1)
        },
        FakePr(PrAnswer::NoLookup),
    );
    let mut report = Recorder::default();

    // When
    pool.new_slot(None, &mut report).unwrap();

    // Then
    assert_eq!(
        report.warnings,
        [format!("the pool now has 2 slots, above {limit}")]
    );
}

#[test]
fn new_slot__should_refuse_a_name_without_the_slot_prefix() {
    // Given
    let fixture = Fixture::must_new();
    let pool = Pool::with_view_pr(
        Config {
            slot_prefix_source: Source::RepoFile(PathBuf::from("/r/.git/wtpool.toml")),
            ..fixture.config(3)
        },
        FakePr(PrAnswer::NoLookup),
    );
    let name = SlotName::try_new("review").unwrap();

    // When
    let result = pool.new_slot(Some(name), &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            "invalid slot name 'review': a slot name must start with slot_prefix 'app-wt' in /r/.git/wtpool.toml"
        ]
    );
    assert!(!fixture.root.join("review").exists());
    assert!(!fixture.foreground.join(".git/wtpool.lock").exists());
}

#[test]
fn new_slot__should_refuse_a_path_that_exists() {
    // Given
    let fixture = Fixture::must_new();
    std::fs::create_dir(fixture.slot(1)).unwrap();
    let pool = fixture.pool(3);

    // When
    let result = pool.new_slot(
        Some(SlotName::try_new("app-wt1").unwrap()),
        &mut Recorder::default(),
    );

    // Then
    assert_eq!(
        result.unwrap_err().to_string(),
        format!("{} already exists", fixture.slot(1).display())
    );
    assert!(!fixture.must_registered(&fixture.slot(1)));
}

#[rstest]
#[case::bg(
    |pool: &Pool<FakePr>,
    report: &mut Recorder| pool.background("feature", SyncMode::No, report).map(drop)
)]
#[case::new(|pool: &Pool<FakePr>, report: &mut Recorder| pool.new_slot(None, report).map(drop))]
#[case::setup(|pool: &Pool<FakePr>, report: &mut Recorder| pool.setup(report).map(drop))]
fn slot_creation__should_fail_when_git_cannot_add_the_worktree(#[case] command: PoolCommand) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_branch("feature");
    let blocker = fixture.root.join("file");
    std::fs::write(&blocker, "").unwrap();
    let pool = Pool::with_view_pr(
        Config {
            slot_dir: AbsolutePath::try_new(blocker.join("slots")).unwrap(),
            ..fixture.config(1)
        },
        FakePr(PrAnswer::NoLookup),
    );
    let mut report = Recorder::default();

    // When
    let result = command(&pool, &mut report);

    // Then
    assert_eq!(
        result.unwrap_err().to_string(),
        format!(
            "could not create worktree at {}",
            blocker.join("slots/app-wt1").display()
        )
    );
    assert!(!report.passed_on.is_empty(), "{}", report.err);
}
