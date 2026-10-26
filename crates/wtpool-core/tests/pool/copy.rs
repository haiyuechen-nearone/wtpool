//! The `copy` files `wtpool` puts into the slots it creates or fills in.

use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use rstest::rstest;
use wtpool_core::{AbsolutePath, Config, Pool, Uncommitted};

use crate::support::{FakePr, Fixture, PrAnswer, Recorder, Setup, must_git};

fn must_mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn new_slot__should_copy_the_files_git_ignores_in_the_main_worktree() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_ignore(".envrc\n.env\n");
    fixture.must_write(".envrc", "use flake\n");
    fixture.must_write("config/.env", "TOKEN=1\n");
    std::fs::set_permissions(
        fixture.foreground.join("config/.env"),
        Permissions::from_mode(0o640),
    )
    .unwrap();
    let pool = fixture.pool_with_setup(3, &[".envrc", "config/.env"], None);
    let mut report = Recorder::default();

    // When
    let slot = pool.new_slot(None, &mut report).unwrap();

    // Then
    assert_eq!(
        std::fs::read_to_string(slot.join(".envrc")).unwrap(),
        "use flake\n"
    );
    assert_eq!(
        std::fs::read_to_string(slot.join("config/.env")).unwrap(),
        "TOKEN=1\n"
    );
    assert_eq!(must_mode(&slot.join("config/.env")), 0o640);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let rows = pool.list(&mut Recorder::default()).unwrap();
    assert_eq!(rows[1].state.dirty, Uncommitted::Files(0));
}

#[test]
fn new_slot__should_skip_a_file_the_main_worktree_lacks_quietly() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_ignore(".env\n");
    fixture.must_write("README.d", "a file where a folder would be\n");
    let pool = fixture.pool_with_setup(3, &[".env", "README.d/.env"], None);
    let mut report = Recorder::default();

    // When
    let slot = pool.new_slot(None, &mut report).unwrap();

    // Then
    assert!(!slot.join(".env").exists());
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

fn untracked_file(fixture: &Fixture) {
    fixture.must_write(".env", "TOKEN=1\n");
}

fn ignored_file_in_the_index(fixture: &Fixture) {
    fixture.must_ignore(".envrc\n");
    fixture.must_write(".envrc", "use flake\n");
    must_git(&fixture.foreground, &["add", "--force", ".envrc"]);
}

#[rstest]
#[case::file_git_does_not_ignore(untracked_file as Setup, ".env")]
#[case::ignored_file_git_tracks(ignored_file_in_the_index as Setup, ".envrc")]
fn new_slot__should_warn_and_skip_a_file_git_does_not_ignore(
    #[case] setup: Setup,
    #[case] path: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    setup(&fixture);
    let pool = fixture.pool_with_setup(3, &[path], None);
    let mut report = Recorder::default();

    // When
    let slot = pool.new_slot(None, &mut report).unwrap();

    // Then
    assert!(!slot.join(path).exists());
    assert_eq!(
        report.warnings,
        [format!(
            "not copying {path}: git does not ignore it in {}, so the slot would not stay clean",
            fixture.foreground.display()
        )]
    );
}

fn symlinked_file(fixture: &Fixture) {
    std::os::unix::fs::symlink(
        fixture.root.join("victim"),
        fixture.foreground.join(".envrc"),
    )
    .unwrap();
}

fn symlinked_folder(fixture: &Fixture) {
    std::fs::create_dir(fixture.root.join("elsewhere")).unwrap();
    std::fs::write(fixture.root.join("elsewhere/.envrc"), "precious\n").unwrap();
    std::os::unix::fs::symlink(
        fixture.root.join("elsewhere"),
        fixture.foreground.join("config"),
    )
    .unwrap();
}

fn folder(fixture: &Fixture) {
    std::fs::create_dir_all(fixture.foreground.join(".envrc")).unwrap();
}

#[rstest]
#[case::symlinked_file(
    ".envrc",
    symlinked_file as Setup,
    "not copying .envrc: {foreground}/.envrc is a symlink"
)]
#[case::file_in_a_symlinked_folder(
    "config/.envrc",
    symlinked_folder as Setup,
    "not copying config/.envrc: {foreground}/config is a symlink"
)]
#[case::folder(
    ".envrc",
    folder as Setup,
    "not copying .envrc: {foreground}/.envrc is not a regular file"
)]
fn new_slot__should_warn_and_skip_what_is_not_a_regular_file_in_the_main_worktree(
    #[case] path: &str,
    #[case] setup: Setup,
    #[case] warning: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_ignore(".envrc\nconfig\n");
    std::fs::write(fixture.root.join("victim"), "precious\n").unwrap();
    setup(&fixture);
    let pool = fixture.pool_with_setup(3, &[path], None);
    let mut report = Recorder::default();

    // When
    let slot = pool.new_slot(None, &mut report).unwrap();

    // Then
    assert!(slot.join(path).symlink_metadata().is_err());
    assert_eq!(report.warnings, [fixture.expand(warning)]);
}

#[test]
fn setup__should_not_write_through_a_symlinked_folder_of_the_branch() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_ignore(".env\n");
    fixture.must_write("config/.env", "TOKEN=1\n");
    std::fs::create_dir(fixture.root.join("elsewhere")).unwrap();
    must_branch_with_symlink(&fixture, "evil", "config", &fixture.root.join("elsewhere"));
    let slot = fixture.must_add_worktree("app-wt1", Some("evil"));
    let pool = fixture.pool_with_setup(1, &["config/.env"], None);
    let mut report = Recorder::default();

    // When
    pool.setup(&mut report).unwrap();

    // Then
    assert!(!fixture.root.join("elsewhere/.env").exists());
    assert_eq!(
        report.warnings,
        [format!(
            "not copying config/.env into {}: {}/config is not a folder",
            slot.display(),
            slot.display()
        )]
    );
}

fn slot_with_its_own_file(fixture: &Fixture) {
    let slot = fixture.must_add_worktree("app-wt1", None);
    std::fs::write(slot.join(".envrc"), "mine\n").unwrap();
}

fn slot_whose_branch_brought_a_symlink(fixture: &Fixture) {
    must_branch_with_symlink(fixture, "evil", ".envrc", &fixture.root.join("victim"));
    fixture.must_add_worktree("app-wt1", Some("evil"));
}

fn slot_without_the_file(fixture: &Fixture) {
    fixture.must_add_worktree("app-wt1", None);
}

#[rstest]
#[case::missing_file_filled_in(slot_without_the_file as Setup, "use flake\n")]
#[case::own_file_kept(slot_with_its_own_file as Setup, "mine\n")]
#[case::symlink_of_the_branch_kept(slot_whose_branch_brought_a_symlink as Setup, "precious\n")]
fn setup__should_fill_in_only_the_files_an_existing_slot_lacks(
    #[case] setup: Setup,
    #[case] expected: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_ignore(".envrc\n");
    fixture.must_write(".envrc", "use flake\n");
    std::fs::write(fixture.root.join("victim"), "precious\n").unwrap();
    setup(&fixture);
    let ran = fixture.root.join("ran");
    let pool =
        fixture.pool_with_setup(2, &[".envrc"], Some(&format!("pwd >> '{}'", ran.display())));
    let mut report = Recorder::default();

    // When
    pool.setup(&mut report).unwrap();

    // Then
    assert_eq!(
        std::fs::read_to_string(fixture.slot(1).join(".envrc")).unwrap(),
        expected
    );
    assert_eq!(
        std::fs::read_to_string(fixture.slot(2).join(".envrc")).unwrap(),
        "use flake\n"
    );
    assert_eq!(
        std::fs::read_to_string(&ran).unwrap(),
        format!("{}\n", fixture.slot(2).display())
    );
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn setup__should_copy_into_a_folder_the_slot_has() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_ignore(".env\n");
    fixture.must_write("config/.env", "TOKEN=1\n");
    let slot = fixture.must_add_worktree("app-wt1", None);
    std::fs::create_dir(slot.join("config")).unwrap();
    let pool = fixture.pool_with_setup(1, &["config/.env"], None);

    // When
    pool.setup(&mut Recorder::default()).unwrap();

    // Then
    assert_eq!(
        std::fs::read_to_string(slot.join("config/.env")).unwrap(),
        "TOKEN=1\n"
    );
}

fn unreadable_file(fixture: &Fixture) {
    fixture.must_write(".envrc", "use flake\n");
    std::fs::set_permissions(
        fixture.foreground.join(".envrc"),
        Permissions::from_mode(0o000),
    )
    .unwrap();
}

fn unreadable_folder_of_the_main_worktree(fixture: &Fixture) {
    fixture.must_write("config/.envrc", "use flake\n");
    std::fs::set_permissions(
        fixture.foreground.join("config"),
        Permissions::from_mode(0o000),
    )
    .unwrap();
}

fn read_only_slot_for_a_file(fixture: &Fixture) {
    fixture.must_write(".envrc", "use flake\n");
    std::fs::set_permissions(fixture.slot(1), Permissions::from_mode(0o555)).unwrap();
}

fn read_only_slot_for_a_folder(fixture: &Fixture) {
    fixture.must_write("config/.envrc", "use flake\n");
    std::fs::set_permissions(fixture.slot(1), Permissions::from_mode(0o555)).unwrap();
}

/// A problem in the main worktree meets every slot: first the slot `setup` creates, then the
/// existing one. A problem in the existing slot meets that slot only.
#[rstest]
#[case::unreadable_file(
    ".envrc",
    unreadable_file as Setup,
    &[
        "could not copy .envrc into {slot2}: Permission denied (os error 13)",
        "could not copy .envrc into {slot1}: Permission denied (os error 13)",
    ]
)]
#[case::unreadable_folder(
    "config/.envrc",
    unreadable_folder_of_the_main_worktree as Setup,
    &[
        "could not copy config/.envrc into {slot2}: Permission denied (os error 13)",
        "could not copy config/.envrc into {slot1}: Permission denied (os error 13)",
    ]
)]
#[case::read_only_slot_for_a_file(
    ".envrc",
    read_only_slot_for_a_file as Setup,
    &["could not copy .envrc into {slot1}: Permission denied (os error 13)"]
)]
#[case::read_only_slot_for_a_folder(
    "config/.envrc",
    read_only_slot_for_a_folder as Setup,
    &["could not copy config/.envrc into {slot1}: Permission denied (os error 13)"]
)]
fn setup__should_fail_but_go_on_when_a_file_cannot_be_copied(
    #[case] path: &str,
    #[case] setup: Setup,
    #[case] failures: &[&str],
) {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_ignore(".envrc\n");
    fixture.must_add_worktree("app-wt1", None);
    setup(&fixture);
    let pool = fixture.pool_with_setup(2, &[path], None);
    let mut report = Recorder::default();

    // When
    let result = pool.setup(&mut report);

    // Then
    let mut expected: Vec<String> = failures
        .iter()
        .map(|failure| fixture.expand(failure))
        .collect();
    expected.push("copy or on_new_slot failed as shown above; the slots were kept".to_owned());
    assert_eq!(result.unwrap_err().messages(), expected);
    assert!(fixture.slot(2).exists());
}

#[test]
fn new_slot__should_fail_but_keep_the_slot_when_git_cannot_tell_what_it_ignores() {
    // Given
    let fixture = Fixture::must_new();
    let plain = fixture.root.join("plain");
    std::fs::create_dir(&plain).unwrap();
    std::fs::write(plain.join(".envrc"), "use flake\n").unwrap();
    let pool = Pool::new(
        Config {
            main_worktree: Some(AbsolutePath::try_new(&plain).unwrap()),
            ..fixture.config_with_setup(3, &[".envrc"], None)
        },
        FakePr(PrAnswer::NoLookup),
    );
    let mut report = Recorder::default();

    // When
    let result = pool.new_slot(None, &mut report);

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            format!(
                "could not tell whether git ignores .envrc in {}; not copied",
                plain.display()
            ),
            "copy or on_new_slot failed as shown above; the slots were kept".to_owned()
        ]
    );
    assert!(!report.passed_on.is_empty(), "{}", report.err);
    assert!(fixture.must_registered(&fixture.slot(1)));
}

#[test]
fn new_slot__should_fail_but_keep_the_slot_without_a_main_worktree_to_copy_from() {
    // Given
    let fixture = Fixture::must_new();
    let pool = Pool::new(
        Config {
            main_worktree: None,
            ..fixture.config_with_setup(3, &[".envrc"], None)
        },
        FakePr(PrAnswer::NoLookup),
    );

    // When
    let result = pool.new_slot(None, &mut Recorder::default());

    // Then
    assert_eq!(
        result.unwrap_err().messages(),
        [
            format!(
                "nothing copied into {}: the repository has no main worktree to copy from",
                fixture.slot(1).display()
            ),
            "copy or on_new_slot failed as shown above; the slots were kept".to_owned()
        ]
    );
    assert!(fixture.must_registered(&fixture.slot(1)));
}

/// A commit on a new branch `name` that tracks `path` as a symlink to `target`.
fn must_branch_with_symlink(fixture: &Fixture, name: &str, path: &str, target: &Path) {
    let scratch = fixture.root.join("scratch-symlink");
    must_git(
        &fixture.foreground,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            name,
            scratch.to_str().unwrap(),
        ],
    );
    std::os::unix::fs::symlink(target, scratch.join(path)).unwrap();
    must_git(&scratch, &["add", "--force", path]);
    must_git(&scratch, &["commit", "--quiet", "-m", "symlink"]);
    must_git(
        &fixture.foreground,
        &["worktree", "remove", "--force", scratch.to_str().unwrap()],
    );
}
