use rstest::rstest;

use super::*;

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
#[case::found(Some(absolute("me")), Some(absolute("me")), Ok(absolute("me")))]
#[case::found_without_home(None, Some(absolute("system")), Ok(absolute("system")))]
#[case::not_set(None, None, Err("invalid HOME='': not set"))]
#[case::empty(Some(PathBuf::new()), None, Err("invalid HOME='': not set"))]
#[case::relative(
    Some(PathBuf::from("me")),
    None,
    Err("invalid HOME='me': not absolute")
)]
fn checked_home__should_refuse_naming_home_when_no_folder_was_found(
    #[case] home: Option<PathBuf>,
    #[case] found: Option<PathBuf>,
    #[case] expected: Result<PathBuf, &str>,
) {
    // When
    let checked = env::checked_home(home.map(OsString::from), found);

    // Then
    assert_eq!(
        checked
            .map(AbsolutePath::into_path_buf)
            .map_err(|error| error.to_string()),
        expected.map_err(str::to_owned)
    );
}
