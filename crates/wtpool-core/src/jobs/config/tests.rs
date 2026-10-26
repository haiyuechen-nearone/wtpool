use std::path::{Path, PathBuf};

use rstest::rstest;

use super::*;

fn write(dir: &Path, text: &str) -> PathBuf {
    let path = dir.join("wtpool/machine.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

fn reason(text: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), text);
    let error = MachineConfig::read(&path, 16).unwrap_err();
    assert_eq!(error.path, path);
    error.fault.to_string()
}

#[rstest]
#[case::one_core(1, 1)]
#[case::four_cores(4, 1)]
#[case::seven_cores(7, 1)]
#[case::sixteen_cores(16, 4)]
#[case::thirty_cores(30, 7)]
#[case::sixty_four_cores(64, 8)]
#[case::no_core(0, 1)]
fn limit_default_for__should_take_a_quarter_of_the_cores_from_1_to_8(
    #[case] cores: usize,
    #[case] expected: u32,
) {
    // When
    let limit = Limit::default_for(cores);

    // Then
    assert_eq!(limit.get(), expected);
}

#[rstest]
#[case::lowest(1, Some(1))]
#[case::highest(64, Some(64))]
#[case::zero(0, None)]
#[case::past_the_highest(65, None)]
#[case::negative(-1, None)]
fn limit_new__should_keep_within_the_bounds(#[case] count: i64, #[case] expected: Option<u32>) {
    // When
    let limit = Limit::new(count);

    // Then
    assert_eq!(limit.map(Limit::get), expected);
}

#[rstest]
#[case::lowest("1", Ok(1))]
#[case::highest("64", Ok(64))]
#[case::leading_zero("08", Ok(8))]
#[case::zero("0", Err(InvalidLimit))]
#[case::past_the_highest("65", Err(InvalidLimit))]
#[case::past_any_integer("99999999999999999999", Err(InvalidLimit))]
#[case::signed("+6", Err(InvalidLimit))]
#[case::negative("-1", Err(InvalidLimit))]
#[case::blank(" 6", Err(InvalidLimit))]
#[case::empty("", Err(InvalidLimit))]
#[case::word("default", Err(InvalidLimit))]
fn limit_from_str__should_take_only_decimal_digits_within_the_bounds(
    #[case] text: &str,
    #[case] expected: Result<u32, InvalidLimit>,
) {
    // When
    let limit = text.parse::<Limit>();

    // Then
    assert_eq!(limit.map(Limit::get), expected);
}

#[test]
fn invalid_limit__should_name_the_bounds() {
    // When
    let message = InvalidLimit.to_string();

    // Then
    assert_eq!(message, "the limit is a whole number from 1 to 64");
}

#[rstest]
#[case::off("off", Ok(Enforce::Off))]
#[case::warn("warn", Ok(Enforce::Warn))]
#[case::require("require", Ok(Enforce::Require))]
#[case::capitalized("Warn", Err(InvalidEnforce))]
#[case::unknown("strict", Err(InvalidEnforce))]
#[case::empty("", Err(InvalidEnforce))]
fn enforce_from_str__should_take_the_name_the_config_writes(
    #[case] name: &str,
    #[case] expected: Result<Enforce, InvalidEnforce>,
) {
    // When
    let enforce = name.parse::<Enforce>();

    // Then
    assert_eq!(enforce, expected);
    assert_eq!(Enforce::from_name(name), expected.ok());
}

#[test]
fn invalid_enforce__should_name_every_policy() {
    // When
    let message = InvalidEnforce.to_string();

    // Then
    assert_eq!(message, "the policy is off, warn or require");
}

#[test]
fn machine_config_read__should_give_the_defaults_without_a_file() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wtpool/machine.toml");

    // When
    let config = MachineConfig::read(&path, 16).unwrap();

    // Then
    assert_eq!(config, MachineConfig::defaults(path, 16));
    assert_eq!(
        (config.limit.get(), config.enforce, config.limit_source),
        (4, Enforce::Warn, ConfigSource::Default)
    );
}

#[test]
fn machine_config_read__should_take_each_key_the_file_sets() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        dir.path(),
        "# mine\n[jobs]\nlimit = 6\nenforce = \"require\"\nheavy = [\"make\", \"cargo-*\"]\n",
    );

    // When
    let config = MachineConfig::read(&path, 16).unwrap();

    // Then
    let heavy: Vec<&str> = config.heavy.iter().map(Pattern::as_str).collect();
    assert_eq!(
        (
            config.limit.get(),
            config.limit_source,
            config.default_limit.get()
        ),
        (6, ConfigSource::Machine, 4)
    );
    assert_eq!(
        (config.enforce, config.enforce_source),
        (Enforce::Require, ConfigSource::Machine)
    );
    assert_eq!(heavy, ["make", "cargo-*"]);
}

#[test]
fn machine_config_read__should_keep_the_defaults_of_keys_the_file_omits() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "[jobs]\nenforce = \"off\"\n");

    // When
    let config = MachineConfig::read(&path, 64).unwrap();

    // Then
    assert_eq!(
        (
            config.limit.get(),
            config.limit_source,
            config.enforce_source
        ),
        (8, ConfigSource::Default, ConfigSource::Machine)
    );
}

#[rstest]
#[case::unknown_key("[jobs]\nlimits = 2\n", "line 2: unknown field `limits`")]
#[case::top_level_key("limit = 2\n", "line 1: unknown field `limit`")]
#[case::unknown_table("[jobs]\n\n[other]\nx = 1\n", "line 3: unknown field `other`")]
#[case::limit_a_string("[jobs]\nlimit = \"4\"\n", "line 2: invalid type: string \"4\"")]
#[case::limit_a_fraction("[jobs]\nlimit = 2.5\n", "line 2: invalid type: floating point")]
#[case::heavy_a_string("[jobs]\nheavy = \"make\"\n", "line 2: invalid type: string \"make\"")]
#[case::limit_zero(
    "[jobs]\nlimit = 0\n",
    "limit = 0: the limit is a whole number from 1 to 64"
)]
#[case::limit_65(
    "[jobs]\nlimit = 65\n",
    "limit = 65: the limit is a whole number from 1 to 64"
)]
#[case::unknown_policy("[jobs]\nenforce = \"strict\"\n", "line 2: unknown variant `strict`")]
#[case::bad_pattern("[jobs]\nheavy = [\"make *\"]\n", "\"make *\" is not a heavy pattern")]
#[case::not_toml("[jobs\nlimit = 2\n", "line 1: ")]
fn machine_config_read__should_refuse_an_invalid_file(#[case] text: &str, #[case] start: &str) {
    // When
    let reason = reason(text);

    // Then
    assert!(
        reason.starts_with(start) || reason.contains(start),
        "{reason:?} does not start with {start:?}"
    );
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::*;

    fn chmod(path: &Path, mode: u32) {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[rstest]
    #[case::file_writable_by_the_group(true, 0o664, "its group or other users may write to it")]
    #[case::file_writable_by_others(true, 0o646, "its group or other users may write to it")]
    #[case::folder_writable_by_the_group(false, 0o775, "its folder ")]
    fn machine_config_read__should_refuse_a_file_others_may_change(
        #[case] file: bool,
        #[case] mode: u32,
        #[case] part: &str,
    ) {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "[jobs]\nlimit = 2\n");
        chmod(if file { &path } else { path.parent().unwrap() }, mode);

        // When
        let error = MachineConfig::read(&path, 16).unwrap_err();

        // Then
        let fault = error.fault.to_string();
        assert!(fault.contains(part), "{fault:?}");
        assert!(error.to_string().contains(&path.display().to_string()));
    }

    #[test]
    fn machine_config_read__should_refuse_a_file_this_user_may_not_read() {
        // Root reads a file of mode 000 all the same.
        if rustix::process::geteuid().is_root() {
            return;
        }
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "[jobs]\nlimit = 2\n");
        chmod(&path, 0o000);

        // When
        let error = MachineConfig::read(&path, 16).unwrap_err();

        // Then
        assert_eq!(
            error.to_string(),
            format!(
                "invalid machine config {}: Permission denied (os error 13)",
                path.display()
            )
        );
    }

    #[test]
    fn machine_config_read__should_read_through_a_symlink_to_a_regular_file() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("real.toml");
        std::fs::write(&target, "[jobs]\nlimit = 5\n").unwrap();
        let path = dir.path().join("wtpool/machine.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(&target, &path).unwrap();
        let to_folder = dir.path().join("wtpool/folder.toml");
        symlink(dir.path(), &to_folder).unwrap();
        let to_nothing = dir.path().join("wtpool/nothing.toml");
        symlink(dir.path().join("gone"), &to_nothing).unwrap();

        // When
        let through = MachineConfig::read(&path, 16).unwrap();
        let folder = MachineConfig::read(&to_folder, 16).unwrap_err();
        let nothing = MachineConfig::read(&to_nothing, 16).unwrap_err();

        // Then
        assert_eq!(through.limit.get(), 5);
        assert_eq!(folder.fault.to_string(), "not a regular file");
        assert_eq!(nothing.fault.to_string(), "it is a symlink to nothing");
    }
}

#[test]
fn machine_config_path__should_follow_xdg_then_home() {
    // Given
    let config = std::env::temp_dir().join("config");
    let home = std::env::temp_dir().join("home");

    // When
    let paths = [
        machine_config_path(Some(&config), Some(&home)),
        machine_config_path(None, Some(&home)),
        machine_config_path(Some(Path::new("")), None),
    ];

    // Then
    assert_eq!(
        paths,
        [
            Some(config.join("wtpool/machine.toml")),
            Some(home.join(".config/wtpool/machine.toml")),
            None
        ]
    );
}
