use rstest::rstest;
use wtpool_core::Source;

use crate::support::Fixture;

#[rstest]
#[case::main_worktree("app")]
#[case::linked_slot("app-wt1")]
#[case::directory_inside_the_main_worktree("app/sub")]
#[case::directory_inside_a_slot("app-wt1/sub")]
fn load__should_derive_the_pool_from_the_main_worktree_of_every_worktree(#[case] cwd: &str) {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    let slot = fixture.must_slot(&repo, "app-wt1");
    std::fs::create_dir(repo.join("sub")).unwrap();
    std::fs::create_dir(slot.join("sub")).unwrap();

    // When
    let settings = fixture.load(&fixture.root.join(cwd), &[]).unwrap();

    // Then
    assert_eq!(
        (
            settings.foreground.value.as_path(),
            &settings.foreground.source,
            settings.slot_dir.value.as_path(),
            &settings.slot_dir.source,
            settings.slot_prefix.value.as_str(),
            &settings.slot_prefix.source,
            settings.slot_count.value.get(),
            settings.remote.value.as_str(),
            settings.main_worktree.as_deref(),
            settings.repository.as_deref(),
        ),
        (
            repo.as_path(),
            &Source::MainWorktree,
            fixture.root.as_path(),
            &Source::MainWorktreeParent,
            "app-wt",
            &Source::MainWorktreeName,
            3,
            "origin",
            Some(repo.as_path()),
            Some(repo.join(".git").as_path()),
        )
    );
}

#[rstest]
#[case::foreground("app")]
#[case::slot("app-wt1")]
fn load__should_find_the_repo_file_from_every_worktree(#[case] cwd: &str) {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    fixture.must_slot(&repo, "app-wt1");
    let file = fixture.must_write_repo_file(
        &repo,
        "slot_prefix = \"box\"\nremote = \"upstream\"\ncopy = [\".env\"]\non_new_slot = \"direnv allow\"\n",
    );

    // When
    let settings = fixture.load(&fixture.root.join(cwd), &[]).unwrap();

    // Then
    assert_eq!(settings.slot_prefix.value.as_str(), "box");
    assert_eq!(settings.slot_prefix.source, Source::RepoFile(file.clone()));
    assert_eq!(settings.remote.value.as_str(), "upstream");
    assert_eq!(settings.remote.source, Source::RepoFile(file));
    assert_eq!(settings.slot_setup.copy.len(), 1);
    assert_eq!(
        settings.slot_setup.on_new_slot.as_deref(),
        Some("direnv allow")
    );
    assert_eq!(settings.foreground.value.as_path(), repo);
    assert_eq!(settings.foreground.source, Source::MainWorktree);
}

#[test]
fn load__should_take_the_folder_names_of_a_pool_that_lives_elsewhere() {
    // Given
    let fixture = Fixture::must_new();
    std::fs::create_dir(fixture.root.join("code")).unwrap();
    let repo = fixture.must_repo("code/web-shop");
    let slot = fixture.must_slot(&repo, "code/web-shop-wt2");

    // When
    let settings = fixture.load(&slot, &[]).unwrap();

    // Then
    assert_eq!(
        (
            settings.foreground.value.as_path(),
            settings.slot_dir.value.as_path(),
            settings.slot_prefix.value.as_str(),
        ),
        (
            repo.as_path(),
            fixture.root.join("code").as_path(),
            "web-shop-wt"
        )
    );
}

#[test]
fn load__should_resolve_relative_paths_against_the_directory_holding_the_main_worktree() {
    // Given
    let fixture = Fixture::must_new();
    let repo = fixture.must_repo("app");
    let slot = fixture.must_slot(&repo, "app-wt1");
    fixture.must_write_repo_file(&repo, "foreground = \"app\"\nslot_dir = \"slots\"\n");

    // When
    let settings = fixture.load(&slot, &[]).unwrap();

    // Then
    assert_eq!(settings.foreground.value.as_path(), repo);
    assert_eq!(
        settings.slot_dir.value.as_path(),
        fixture.root.join("slots")
    );
}
