use rstest::rstest;

use crate::support::{Fixture, must_git};

#[test]
fn load__should_refuse_a_bare_repository_whose_repo_file_names_no_foreground() {
    // Given
    let fixture = Fixture::must_new();
    must_git(&fixture.root, &["init", "--quiet", "--bare", "app.git"]);
    let bare = fixture.root.join("app.git");
    let file = bare.join("wtpool.toml");
    std::fs::write(&file, "slot_count = 2\n").unwrap();

    // When
    let settings = fixture.load(&bare, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "{} is a bare repository, so it has no main worktree to derive foreground from; set WTPOOL_FOREGROUND, or foreground in {}",
            bare.display(),
            file.display()
        )
    );
}

#[test]
fn load__should_accept_a_bare_repository_whose_repo_file_names_the_pool() {
    // Given
    let fixture = Fixture::must_new();
    fixture.must_repo("source");
    must_git(
        &fixture.root,
        &["clone", "--quiet", "--bare", "source", "app.git"],
    );
    let bare = fixture.root.join("app.git");
    let slot = fixture.must_slot(&bare, "app-wt1");
    std::fs::write(
        bare.join("wtpool.toml"),
        "foreground = \"app-wt1\"\nslot_dir = \".\"\nslot_prefix = \"app-wt\"\n",
    )
    .unwrap();

    // When
    let settings = fixture.load(&slot, &[]).unwrap();

    // Then
    assert_eq!(
        (
            settings.foreground.value.as_path(),
            settings.slot_dir.value.as_path(),
            &settings.main_worktree,
        ),
        (slot.as_path(), fixture.root.as_path(), &None)
    );
}

#[rstest]
#[case::bare(&[], "embedded.git")]
#[case::bare_from_a_folder_inside(&[], "embedded.git/refs")]
#[case::config_that_says_it_is_not_bare(&["config", "core.bare", "false"], "embedded.git")]
fn load__should_refuse_a_bare_repository_inside_a_worktree_of_another(
    #[case] config: &[&str],
    #[case] cwd: &str,
) {
    // Given
    let fixture = Fixture::must_new();
    let outer = fixture.must_repo("outer");
    must_git(&outer, &["init", "--quiet", "--bare", "embedded.git"]);
    let embedded = outer.join("embedded.git");
    if !config.is_empty() {
        must_git(&embedded, config);
    }
    std::fs::write(embedded.join("wtpool.toml"), "slot_count = 0\n").unwrap();
    let cwd = outer.join(cwd);

    // When
    let settings = fixture.load(&cwd, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "could not tell which git repository {} belongs to, so wtpool cannot find its pool. Run wtpool from another directory, or set WTPOOL_FOREGROUND to the foreground worktree",
            cwd.display()
        )
    );
}

#[rstest]
#[case::outside_any_worktree("app.git", false)]
#[case::next_to_the_git_file_that_names_it("app/.bare", true)]
fn load__should_read_the_repo_file_of_the_bare_repository_it_runs_in(
    #[case] bare: &str,
    #[case] git_file: bool,
) {
    // Given
    let fixture = Fixture::must_new();
    must_git(&fixture.root, &["init", "--quiet", "--bare", bare]);
    let bare = fixture.root.join(bare);
    if git_file {
        std::fs::write(fixture.root.join("app/.git"), "gitdir: ./.bare\n").unwrap();
    }
    let file = bare.join("wtpool.toml");
    std::fs::write(&file, "slot_count = 0\n").unwrap();

    // When
    let settings = fixture.load(&bare, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "invalid slot_count='0' in {}: must be at least 1",
            file.display()
        )
    );
}
