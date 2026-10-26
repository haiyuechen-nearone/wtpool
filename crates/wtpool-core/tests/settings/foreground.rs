use std::path::Path;

use rstest::rstest;
use wtpool_core::{AbsolutePath, Setting, Source};

use crate::support::Fixture;

#[rstest]
#[case::wtpool_foreground_before_the_current_repository(
    &["here", "one"],
    Some("one"),
    "one",
    Source::Env("WTPOOL_FOREGROUND")
)]
#[case::wtpool_foreground_without_a_repo_file_before_the_current_repository(
    &["here"],
    Some("one"),
    "one-wt",
    Source::Env("WTPOOL_FOREGROUND")
)]
#[case::current_repository_with_a_repo_file(&["here"], None, "here", Source::MainWorktree)]
#[case::current_repository_without_a_repo_file(&[], None, "here-wt", Source::MainWorktree)]
fn load__should_take_wtpool_foreground_before_the_current_repository(
    #[case] repo_files: &[&str],
    #[case] wtpool_foreground: Option<&str>,
    #[case] expected_prefix: &str,
    #[case] expected_foreground_source: Source,
) {
    // Given
    let fixture = Fixture::must_new();
    for name in ["here", "one"] {
        let repo = fixture.must_repo(name);
        if repo_files.contains(&name) {
            fixture.must_write_repo_file(&repo, &format!("slot_prefix = \"{name}\"\n"));
        }
    }
    let wtpool_foreground = wtpool_foreground.map(|name| fixture.root.join(name));
    let vars: Vec<(&str, &Path)> = wtpool_foreground
        .iter()
        .map(|path| ("WTPOOL_FOREGROUND", path.as_path()))
        .collect();
    let expected_repository = wtpool_foreground
        .as_deref()
        .unwrap_or(&fixture.root.join("here"))
        .join(".git");

    // When
    let settings = fixture.load(&fixture.root.join("here"), &vars).unwrap();

    // Then
    assert_eq!(
        (
            settings.repository.as_deref(),
            settings.slot_prefix.value.as_str(),
            &settings.foreground.source,
        ),
        (
            Some(expected_repository.as_path()),
            expected_prefix,
            &expected_foreground_source
        )
    );
}

#[test]
fn load__should_refuse_to_run_outside_a_repository() {
    // Given
    let fixture = Fixture::must_new();
    let plain = fixture.root.join("plain");
    std::fs::create_dir(&plain).unwrap();
    fixture.must_repo("app");

    // When
    let settings = fixture.load(&plain, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        "not inside a git repository: run wtpool in your repository or set WTPOOL_FOREGROUND"
    );
}

#[test]
fn load__should_refuse_a_derived_setting_when_wtpool_foreground_names_no_repository() {
    // Given
    let fixture = Fixture::must_new();
    let plain = fixture.root.join("plain");
    std::fs::create_dir(&plain).unwrap();
    let repo = fixture.must_repo("app");

    // When
    let settings = fixture.load(&repo, &[("WTPOOL_FOREGROUND", &plain)]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "no git repository at {} (foreground from env WTPOOL_FOREGROUND)",
            plain.display()
        )
    );
}

#[test]
fn load__should_leave_a_foreground_that_is_no_repository_to_the_pool_when_all_is_set() {
    // Given
    let fixture = Fixture::must_new();
    let plain = fixture.root.join("plain");

    // When
    let settings = fixture
        .load(
            &fixture.root,
            &[
                ("WTPOOL_FOREGROUND", &plain),
                ("WTPOOL_SLOT_DIR", &fixture.root),
                ("WTPOOL_SLOT_PREFIX", Path::new("app-wt")),
            ],
        )
        .unwrap();

    // Then
    assert_eq!(
        (&settings.repository, &settings.main_worktree),
        (&None, &None)
    );
    assert_eq!(
        settings.require_repository().unwrap_err().to_string(),
        format!(
            "no git repository at {} (foreground from env WTPOOL_FOREGROUND)",
            plain.display()
        )
    );
}

#[test]
fn load__should_accept_a_foreground_in_a_slot_of_the_repository_of_the_repo_file() {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    let slot = fixture.must_slot(&repo, "app-wt1");
    let file = fixture.must_write_repo_file(&repo, "foreground = \"app-wt1\"\n");

    // When
    let settings = fixture.load(&repo, &[]).unwrap();

    // Then
    assert_eq!(
        settings.foreground,
        Setting {
            value: AbsolutePath::try_new(&slot).unwrap(),
            source: Source::RepoFile(file),
        }
    );
    assert_eq!(settings.main_worktree.as_deref(), Some(repo.as_path()));
}

#[rstest]
#[case::worktree_of_another_repository("other")]
#[case::directory_that_is_no_worktree("plain")]
fn load__should_refuse_a_foreground_outside_the_repository_of_the_repo_file(
    #[case] foreground: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    fixture.must_repo("other");
    std::fs::create_dir(fixture.root.join("plain")).unwrap();
    let file = fixture.must_write_repo_file(&repo, &format!("foreground = \"{foreground}\"\n"));

    // When
    let settings = fixture.load(&repo, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "invalid foreground='{}' in {}: not a worktree of {}",
            fixture.root.join(foreground).display(),
            file.display(),
            repo.join(".git").display()
        )
    );
}
