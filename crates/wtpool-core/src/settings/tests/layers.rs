use rstest::rstest;

use super::*;

#[rstest]
#[case::environment_over_the_repo_file(
    true,
    true,
    [
        ("/e/app", "env WTPOOL_FOREGROUND"),
        ("/e", "env WTPOOL_SLOT_DIR"),
        ("e-", "env WTPOOL_SLOT_PREFIX"),
        ("1", "env WTPOOL_SLOT_COUNT"),
        ("fork", "env WTPOOL_REMOTE"),
    ]
)]
#[case::repo_file_over_the_defaults(
    false,
    true,
    [
        ("/r/app", "/code/app/.git/wtpool.toml"),
        ("/r", "/code/app/.git/wtpool.toml"),
        ("r-", "/code/app/.git/wtpool.toml"),
        ("2", "/code/app/.git/wtpool.toml"),
        ("upstream", "/code/app/.git/wtpool.toml"),
    ]
)]
#[case::defaults_from_the_main_worktree(
    false,
    false,
    [
        ("/code/app", "main worktree"),
        ("/code", "parent of the main worktree"),
        ("app-wt", "main worktree name + -wt"),
        ("3", "default"),
        ("origin", "default"),
    ]
)]
fn resolve__should_take_each_setting_from_the_first_source_that_sets_it(
    #[case] with_env: bool,
    #[case] with_repo: bool,
    #[case] expected: [(&str, &str); 5],
) {
    // Given
    let env_vars: &[(&'static str, &'static str)] = &[
        ("WTPOOL_FOREGROUND", "/e/app"),
        ("WTPOOL_SLOT_DIR", "/e"),
        ("WTPOOL_SLOT_PREFIX", "e-"),
        ("WTPOOL_SLOT_COUNT", "1"),
        ("WTPOOL_REMOTE", "fork"),
    ];
    let vars = if with_env { env_vars } else { &[] };
    let repo = repo_layer(values(Some("/r/app"), "/r", "r-", 2));

    // When
    let settings = resolve_with(vars, with_repo.then_some(&repo), &main_worktree()).unwrap();

    // Then
    assert_eq!(
        summary(&settings),
        expected.map(|(v, s)| (v.to_owned(), s.to_owned()))
    );
    assert_eq!(settings.main_worktree, Some(path("/code/app")));
}

#[rstest]
#[case::nothing_set(&[], "foreground", "WTPOOL_FOREGROUND")]
#[case::foreground_set(&[("WTPOOL_FOREGROUND", "/code/app")], "slot_dir", "WTPOOL_SLOT_DIR")]
#[case::foreground_and_slot_dir_set(
    &[("WTPOOL_FOREGROUND", "/code/app"), ("WTPOOL_SLOT_DIR", "/code")],
    "slot_prefix",
    "WTPOOL_SLOT_PREFIX"
)]
fn resolve__should_refuse_a_default_from_the_main_worktree_of_a_bare_repository(
    #[case] vars: &[(&'static str, &'static str)],
    #[case] key: &str,
    #[case] var: &str,
) {
    // When
    let settings = resolve_with(vars, None, &bare());

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        format!(
            "/code/app.git is a bare repository, so it has no main worktree to derive {key} from; set {var}, or {key} in /code/app.git/wtpool.toml"
        )
    );
}

#[rstest]
#[case::bare(bare())]
#[case::unknown(Main::Unknown(path("/code/app")))]
fn resolve__should_need_no_main_worktree_when_every_setting_is_set(#[case] main: Main) {
    // Given
    let vars = [
        ("WTPOOL_FOREGROUND", "/code/app"),
        ("WTPOOL_SLOT_DIR", "/code"),
        ("WTPOOL_SLOT_PREFIX", "app-wt"),
    ];

    // When
    let settings = resolve_with(&vars, None, &main).unwrap();

    // Then
    assert_eq!(
        (settings.slot_prefix.value.as_str(), &settings.main_worktree),
        ("app-wt", &None)
    );
}

#[test]
fn resolve__should_refuse_a_default_when_wtpool_foreground_names_no_repository() {
    // When
    let settings = resolve_with(
        &[("WTPOOL_FOREGROUND", "/code/app")],
        None,
        &Main::Unknown(path("/code/app")),
    );

    // Then
    assert_eq!(
        settings.unwrap_err().to_string(),
        "no git repository at /code/app (foreground from env WTPOOL_FOREGROUND)"
    );
}

#[rstest]
#[case::from_the_repository_file(true, &["make"], true)]
#[case::without_a_repository_file(false, &[], false)]
fn resolve__should_take_build_processes_and_slot_setup_from_the_repository_file(
    #[case] with_file: bool,
    #[case] expected: &[&str],
    #[case] expected_setup: bool,
) {
    // Given
    let layer = repo_layer(values(None, "/w", "box", 2));
    let repo = with_file.then_some(&layer);

    // When
    let settings = resolve_with(&[], repo, &main_worktree()).unwrap();

    // Then
    let setup = if expected_setup {
        layer.repo_only.slot_setup.clone()
    } else {
        SlotSetup::default()
    };
    assert_eq!(settings.build_processes, expected);
    assert_eq!(settings.slot_setup, setup);
    let config = settings.into_config();
    assert_eq!(config.slot_setup, setup);
}

#[rstest]
#[case::folder("/code/app", Ok("app-wt"))]
#[case::folder_with_a_dot("/code/app.v2", Ok("app.v2-wt"))]
#[case::root(
    "/",
    Err(
        "invalid slot_prefix='-wt' from the main worktree name: a slot name cannot start with '-'"
    )
)]
#[case::folder_with_a_leading_dash(
    "/code/-app",
    Err(
        "invalid slot_prefix='-app-wt' from the main worktree name: a slot name cannot start with '-'"
    )
)]
fn prefix_of__should_append_wtpool_to_the_folder_name(
    #[case] main: &str,
    #[case] expected: Result<&str, &str>,
) {
    // When
    let prefix = discover::prefix_of(&path(main));

    // Then
    assert_eq!(
        prefix
            .map(|prefix| prefix.as_str().to_owned())
            .map_err(|error| error.to_string()),
        expected.map(str::to_owned).map_err(str::to_owned)
    );
}

#[test]
fn enclosing__should_refuse_without_a_current_directory() {
    // When
    let repository = discover::enclosing(None, None, GitMode::Writing);

    // Then
    assert_eq!(
        repository.unwrap_err().to_string(),
        "not inside a git repository: run wtpool in your repository or set WTPOOL_FOREGROUND"
    );
}
