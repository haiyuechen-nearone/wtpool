use rstest::rstest;

use crate::support::Fixture;

#[rstest]
#[case::with_a_repo_file(true)]
#[case::without_a_repo_file(false)]
fn load__should_refuse_a_slot_git_cannot_read(#[case] repo_file: bool) {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    let slot = fixture.must_slot(&repo, "app-wt1");
    std::fs::remove_dir_all(repo.join(".git/worktrees/app-wt1")).unwrap();
    if repo_file {
        fixture.must_write_repo_file(&repo, "slot_count = 2\n");
    }

    // When
    let settings = fixture.load(&slot, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "could not tell which git repository {} belongs to, so wtpool cannot find its pool. Run wtpool from another directory, or set WTPOOL_FOREGROUND to the foreground worktree",
            slot.display()
        )
    );
}

#[test]
fn load__should_refuse_a_symlinked_repo_file() {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    let target = fixture.root.join("target.toml");
    std::fs::write(&target, "slot_count = 2\n").unwrap();
    let link = repo.join(".git/wtpool.toml");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    // When
    let settings = fixture.load(&repo, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!("refusing to read {}: it is a symlink", link.display())
    );
}

#[rstest]
#[case::real_path("app")]
#[case::symlink("link")]
fn load__should_not_look_above_git_ceiling_directories(#[case] ceiling: &str) {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    std::os::unix::fs::symlink(&repo, fixture.root.join("link")).unwrap();
    std::fs::create_dir(repo.join("sub")).unwrap();

    // When
    let settings = fixture.load(
        &repo.join("sub"),
        &[("GIT_CEILING_DIRECTORIES", &fixture.root.join(ceiling))],
    );

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        "not inside a git repository: run wtpool in your repository or set WTPOOL_FOREGROUND"
    );
}

#[test]
fn load__should_refuse_an_invalid_repo_file() {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    let file = fixture.must_write_repo_file(&repo, "slot_count = 0\n");

    // When
    let settings = fixture.load(&repo, &[]);

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "invalid slot_count='0' in {}: must be at least 1",
            file.display()
        )
    );
}

#[test]
fn load__should_expand_home_in_the_repo_file() {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    fixture.must_write_repo_file(&repo, "slot_dir = \"~/slots\"\n");

    // When
    let settings = fixture.load(&repo, &[]).unwrap();

    // Then
    assert_eq!(
        settings.slot_dir.value.as_path(),
        fixture.home.join("slots")
    );
}
