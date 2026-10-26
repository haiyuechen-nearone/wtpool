use std::path::Path;

use rstest::rstest;

use crate::config::CopyPath;

use super::*;

/// An environment holding only `vars`.
fn env(vars: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
    let vars = vars.to_vec();
    move |name: &str| {
        vars.iter()
            .find(|(var, _)| *var == name)
            .map(|(_, value)| OsString::from(value))
    }
}

fn path(path: &str) -> AbsolutePath {
    AbsolutePath::try_new(path).unwrap()
}

fn values(foreground: Option<&str>, dir: &str, prefix: &str, count: usize) -> Values {
    Values {
        foreground: foreground.map(path),
        slot_dir: Some(path(dir)),
        slot_prefix: Some(SlotPrefix::try_new(prefix).unwrap()),
        slot_count: Some(SlotCount::try_new(count).unwrap()),
        remote: Some(RemoteName::try_new("upstream").unwrap()),
    }
}

fn repo_layer(values: Values) -> RepoLayer {
    RepoLayer {
        file: path("/code/app/.git/wtpool.toml"),
        values,
        repo_only: RepoOnly {
            heavy: Vec::new(),
            build_processes: vec!["make".to_owned()],
            slot_setup: SlotSetup {
                copy: vec![CopyPath::try_new(".envrc").unwrap()],
                on_new_slot: Some("direnv allow".to_owned()),
            },
        },
    }
}

fn main_worktree() -> Main {
    Main::Worktree(path("/code/app"))
}

fn bare() -> Main {
    Main::Bare {
        path: path("/code/app.git"),
        file: path("/code/app.git/wtpool.toml"),
    }
}

fn resolve_with(
    vars: &[(&'static str, &'static str)],
    repo: Option<&RepoLayer>,
    main: &Main,
) -> Result<Settings, Error> {
    let env = env::env_values(&env(vars)).unwrap();
    let layers = Layers { env: &env, repo };
    resolve(layers, main, None)
}

fn summary(settings: &Settings) -> [(String, String); 5] {
    [
        (
            settings.foreground.value.display().to_string(),
            settings.foreground.source.to_string(),
        ),
        (
            settings.slot_dir.value.display().to_string(),
            settings.slot_dir.source.to_string(),
        ),
        (
            settings.slot_prefix.value.as_str().to_owned(),
            settings.slot_prefix.source.to_string(),
        ),
        (
            settings.slot_count.value.to_string(),
            settings.slot_count.source.to_string(),
        ),
        (
            settings.remote.value.to_string(),
            settings.remote.source.to_string(),
        ),
    ]
}

fn sample_settings(foreground: Source, repository: Option<&str>) -> Settings {
    Settings {
        foreground: Setting::new(path("/code/app"), foreground),
        slot_dir: Setting::new(path("/code"), Source::MainWorktreeParent),
        slot_prefix: Setting::new(
            SlotPrefix::try_new("app-wt").unwrap(),
            Source::MainWorktreeName,
        ),
        slot_count: Setting::new(
            SlotCount::default(),
            Source::RepoFile(PathBuf::from("/code/app/.git/wtpool.toml")),
        ),
        remote: Setting::new(RemoteName::default(), Source::Env("WTPOOL_REMOTE")),
        build_processes: Vec::new(),
        slot_setup: SlotSetup {
            copy: vec![CopyPath::try_new(".env").unwrap()],
            on_new_slot: None,
        },
        main_worktree: Some(path("/code/app")),
        repository: repository.map(path),
    }
}

#[test]
fn env_values__should_read_every_variable() {
    // Given
    let vars = env(&[
        ("WTPOOL_FOREGROUND", "/srv/app"),
        ("WTPOOL_SLOT_DIR", "/srv"),
        ("WTPOOL_SLOT_PREFIX", "slot-"),
        ("WTPOOL_SLOT_COUNT", " 5\n"),
        ("WTPOOL_REMOTE", "upstream"),
    ]);

    // When
    let values = env::env_values(&vars).unwrap();

    // Then
    assert_eq!(values, self::values(Some("/srv/app"), "/srv", "slot-", 5));
}

#[rstest]
#[case::empty_foreground(&[("WTPOOL_FOREGROUND", "")], "invalid WTPOOL_FOREGROUND='': empty")]
#[case::empty_slot_dir(&[("WTPOOL_SLOT_DIR", "")], "invalid WTPOOL_SLOT_DIR='': empty")]
#[case::empty_prefix(&[("WTPOOL_SLOT_PREFIX", "")], "invalid WTPOOL_SLOT_PREFIX='': empty")]
#[case::empty_count(&[("WTPOOL_SLOT_COUNT", "")], "invalid WTPOOL_SLOT_COUNT='': empty")]
#[case::empty_remote(&[("WTPOOL_REMOTE", "")], "invalid WTPOOL_REMOTE='': empty")]
#[case::prefix_with_a_slash(
    &[("WTPOOL_SLOT_PREFIX", "../app-wt")],
    "invalid WTPOOL_SLOT_PREFIX='../app-wt': a slot name cannot contain '/'"
)]
#[case::count_that_is_not_a_number(
    &[("WTPOOL_SLOT_COUNT", "three")],
    "invalid WTPOOL_SLOT_COUNT='three': invalid digit found in string"
)]
#[case::zero_count(&[("WTPOOL_SLOT_COUNT", "0")], "invalid WTPOOL_SLOT_COUNT='0': must be at least 1")]
#[case::remote_that_is_an_option(
    &[("WTPOOL_REMOTE", "-v")],
    "invalid WTPOOL_REMOTE='-v': not a valid git remote name"
)]
fn env_values__should_refuse_an_invalid_variable(
    #[case] vars: &[(&'static str, &'static str)],
    #[case] expected: &str,
) {
    // When
    let values = env::env_values(&env(vars));

    // Then
    assert_eq!(values.unwrap_err().to_string(), expected);
}

#[cfg(unix)]
#[test]
fn env_values__should_refuse_a_variable_that_is_not_unicode() {
    use std::os::unix::ffi::OsStringExt;

    // Given
    let env = |name: &str| {
        (name == "WTPOOL_FOREGROUND").then(|| OsString::from_vec(b"/tmp/\xff".to_vec()))
    };

    // When
    let values = env::env_values(&env);

    // Then
    assert_eq!(
        values.unwrap_err().to_string(),
        "invalid WTPOOL_FOREGROUND='/tmp/\u{FFFD}': not valid Unicode"
    );
}

#[test]
fn env_values__should_make_relative_paths_absolute() {
    // Given
    let vars = env(&[("WTPOOL_FOREGROUND", "app"), ("WTPOOL_SLOT_DIR", "slots")]);

    // When
    let values = env::env_values(&vars).unwrap();

    // Then
    assert!(values.foreground.unwrap().is_absolute());
    assert!(values.slot_dir.unwrap().is_absolute());
}

/// A folder that is absolute on every platform: `/home/me` names no drive on Windows.
fn absolute(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

#[rstest]
#[case::home(Some(absolute("me")), Some(absolute("system")), Ok(absolute("me")))]
#[case::system_home_without_home(None, Some(absolute("system")), Ok(absolute("system")))]
#[case::empty_home(Some(PathBuf::new()), Some(absolute("system")), Ok(absolute("system")))]
#[case::relative_home(
    Some(PathBuf::from("me")),
    Some(absolute("system")),
    Ok(absolute("system"))
)]
#[case::no_home_at_all(None, None, Err("invalid HOME='': not set"))]
#[case::empty_home_without_a_system_home(
    Some(PathBuf::new()),
    None,
    Err("invalid HOME='': not set")
)]
#[case::relative_home_and_system_home(
    Some(PathBuf::from("me")),
    Some(PathBuf::from("me")),
    Err("invalid HOME='me': not absolute")
)]
#[case::relative_system_home(None, Some(PathBuf::from("home")), Err("invalid HOME='': not set"))]
fn home_dir__should_take_an_absolute_home_else_the_system_home_folder(
    #[case] home: Option<PathBuf>,
    #[case] system: Option<PathBuf>,
    #[case] expected: Result<PathBuf, &str>,
) {
    // Given
    let var = |name: &str| home.clone().filter(|_| name == "HOME").map(OsString::from);

    // When
    let home = env::home_dir(&var, || system);

    // Then
    assert_eq!(
        home.map(AbsolutePath::into_path_buf)
            .map_err(|error| error.to_string()),
        expected.map_err(str::to_owned)
    );
}

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
            config.foreground.as_path(),
            &config.foreground_source,
            config.slot_dir.as_path(),
            config.slot_prefix.as_str(),
            &config.slot_prefix_source,
            config.slot_count.get(),
            &config.slot_count_source,
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
