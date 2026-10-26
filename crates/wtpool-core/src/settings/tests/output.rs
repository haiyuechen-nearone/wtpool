use rstest::rstest;

use super::*;

#[rstest]
#[case::environment(
    Source::Env("WTPOOL_FOREGROUND"),
    "invalid WTPOOL_FOREGROUND='/code/app': not a worktree"
)]
#[case::default(
    Source::Default,
    "invalid WTPOOL_FOREGROUND='/code/app': not a worktree"
)]
#[case::repo_file(
    Source::RepoFile(PathBuf::from("/code/app/.git/wtpool.toml")),
    "invalid foreground='/code/app' in /code/app/.git/wtpool.toml: not a worktree"
)]
#[case::main_worktree(
    Source::MainWorktree,
    "invalid foreground='/code/app' from the main worktree: not a worktree"
)]
#[case::parent_of_the_main_worktree(
    Source::MainWorktreeParent,
    "invalid foreground='/code/app' from the parent of the main worktree: not a worktree"
)]
#[case::main_worktree_name(
    Source::MainWorktreeName,
    "invalid foreground='/code/app' from the main worktree name: not a worktree"
)]
fn invalid__should_name_where_the_value_came_from(#[case] source: Source, #[case] expected: &str) {
    // When
    let error = source.invalid_value(
        Key::Foreground,
        "/code/app".to_owned(),
        "not a worktree".to_owned(),
    );

    // Then
    assert_eq!(error.to_string(), expected);
}

#[rstest]
#[case::environment(Source::Env("WTPOOL_SLOT_COUNT"), "WTPOOL_SLOT_COUNT=2")]
#[case::default(Source::Default, "the default slot_count=2")]
#[case::repo_file(
    Source::RepoFile(PathBuf::from("/code/app/.git/wtpool.toml")),
    "slot_count=2 in /code/app/.git/wtpool.toml"
)]
#[case::main_worktree_name(Source::MainWorktreeName, "slot_count=2 from the main worktree name")]
fn describe__should_name_where_the_value_came_from(#[case] source: Source, #[case] expected: &str) {
    // When
    let described = source.describe("WTPOOL_SLOT_COUNT", "slot_count", "=2");

    // Then
    assert_eq!(described, expected);
}

#[test]
fn to_text__should_align_each_setting_with_its_source() {
    // Given
    let settings = sample_settings(Source::MainWorktree, Some("/code/app/.git"));

    // When
    let text = settings.to_text();

    // Then
    assert_eq!(
        text,
        "SETTING      VALUE      SOURCE\n\
         foreground   /code/app  main worktree\n\
         slot_dir     /code      parent of the main worktree\n\
         slot_prefix  app-wt     main worktree name + -wt\n\
         slot_count   3          /code/app/.git/wtpool.toml\n\
         remote       origin     env WTPOOL_REMOTE\n"
    );
}

#[test]
fn to_json__should_give_each_setting_its_value_and_source() {
    // Given
    let settings = sample_settings(Source::Default, Some("/code/app/.git"));

    // When
    let json = settings.to_json();

    // Then
    assert_eq!(
        json,
        "{\n  \"version\": 1,\n  \
         \"foreground\": {\"value\": \"/code/app\", \"source\": \"default\"},\n  \
         \"slot_dir\": {\"value\": \"/code\", \"source\": \"parent of the main worktree\"},\n  \
         \"slot_prefix\": {\"value\": \"app-wt\", \"source\": \"main worktree name + -wt\"},\n  \
         \"slot_count\": {\"value\": 3, \"source\": \"/code/app/.git/wtpool.toml\"},\n  \
         \"remote\": {\"value\": \"origin\", \"source\": \"env WTPOOL_REMOTE\"}\n}\n"
    );
}

#[test]
fn json_string__should_escape_the_characters_json_does_not_allow_in_a_string() {
    // When
    let json = json_string("a\"b\\c\nd\te\u{1}é\u{7f}");

    // Then
    assert_eq!(json, "\"a\\\"b\\\\c\\nd\\te\\u0001é\u{7f}\"");
}

#[test]
fn require_repository__should_return_the_repository_when_one_was_found() {
    // Given
    let settings = sample_settings(Source::MainWorktree, Some("/code/app/.git"));

    // When
    let required = settings.require_repository();

    // Then
    assert_eq!(required.unwrap(), &path("/code/app/.git"));
}

#[test]
fn require_repository__should_name_the_foreground_when_none_was_found() {
    // Given
    let settings = sample_settings(Source::Env("WTPOOL_FOREGROUND"), None);

    // When
    let required = settings.require_repository();

    // Then
    assert_eq!(
        required.unwrap_err().to_string(),
        "no git repository at /code/app (foreground from env WTPOOL_FOREGROUND)"
    );
}

#[test]
fn into_config__should_carry_every_setting_over() {
    // Given
    let settings = sample_settings(Source::MainWorktree, None);

    // When
    let config = settings.into_config();

    // Then
    assert_eq!(
        (
            config.foreground.value.as_path(),
            &config.foreground.source,
            config.slot_dir.as_path(),
            config.slot_prefix.value.as_str(),
            &config.slot_prefix.source,
            config.slot_count.value.get(),
            &config.slot_count.source,
            config.remote.as_str(),
            config.main_worktree.as_deref(),
            config.slot_setup.copy.len(),
        ),
        (
            Path::new("/code/app"),
            &Source::MainWorktree,
            Path::new("/code"),
            "app-wt",
            &Source::MainWorktreeName,
            3,
            &Source::RepoFile(PathBuf::from("/code/app/.git/wtpool.toml")),
            "origin",
            Some(Path::new("/code/app")),
            1,
        )
    );
}
