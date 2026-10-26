use std::path::PathBuf;

use rstest::rstest;

use super::*;

const FILE: &str = "/r/.git/wtpool.toml";

fn home() -> Result<AbsolutePath, Error> {
    Ok(AbsolutePath::try_new("/home/me").unwrap())
}

fn no_home() -> Result<AbsolutePath, Error> {
    Err(SettingsError::InvalidVariable {
        var: "HOME",
        value: String::new(),
        reason: "not set".to_owned(),
    }
    .into())
}

fn base() -> AbsolutePath {
    AbsolutePath::try_new("/w").unwrap()
}

fn parse_ok(text: &str) -> Contents {
    parse(text, Path::new(FILE), &base(), &home).unwrap()
}

fn parse_error(text: &str) -> String {
    parse(text, Path::new(FILE), &base(), &home)
        .unwrap_err()
        .to_string()
}

#[test]
fn parse__should_read_every_key() {
    // Given
    let text = "foreground = \"/srv/app\"\nslot_dir = \"slots\"\nslot_prefix = \"box\"\nslot_count = 5\nremote = \"upstream\"\nheavy = [\"cargo test\"]\nbuild_processes = [\"make\"]\ncopy = [\".envrc\", \"./config/.env\"]\non_new_slot = \"direnv allow\"\nbounded_waits = false\n";

    // When
    let contents = parse_ok(text);

    // Then
    assert_eq!(
        contents,
        Contents {
            values: Values {
                foreground: Some(AbsolutePath::try_new("/srv/app").unwrap()),
                slot_dir: Some(AbsolutePath::try_new("/w/slots").unwrap()),
                slot_prefix: Some(SlotPrefix::try_new("box").unwrap()),
                slot_count: Some(SlotCount::try_new(5).unwrap()),
                remote: Some(RemoteName::try_new("upstream").unwrap()),
            },
            repo_only: RepoOnly {
                heavy: vec![Pattern::parse("cargo test").unwrap()],
                build_processes: vec!["make".to_owned()],
                slot_setup: SlotSetup {
                    copy: vec![
                        CopyPath::try_new(".envrc").unwrap(),
                        CopyPath::try_new("config/.env").unwrap(),
                    ],
                    on_new_slot: Some("direnv allow".to_owned()),
                },
                bounded_waits: Some(false),
            },
        }
    );
}

#[test]
fn parse__should_leave_keys_the_file_omits_unset() {
    // When
    let contents = parse_ok("# nothing set\n");

    // Then
    assert_eq!(contents, Contents::default());
}

#[rstest]
#[case::foreground(Key::Foreground)]
#[case::slot_dir(Key::SlotDir)]
#[case::slot_prefix(Key::SlotPrefix)]
#[case::slot_count(Key::SlotCount)]
#[case::remote(Key::Remote)]
fn parse__should_know_the_name_of_every_key(#[case] key: Key) {
    // Given
    let text = format!("{} = 1\n", key.name());

    // When
    let parsed = parse(&text, Path::new(FILE), &base(), &home);

    // Then
    assert!(
        parsed
            .as_ref()
            .err()
            .is_none_or(|error| !error.to_string().contains("unknown field")),
        "{parsed:?}"
    );
}

#[rstest]
#[case::absolute("/srv/app", "/srv/app")]
#[case::relative("app", "/w/app")]
#[case::dot(".", "/w")]
#[case::relative_through_dot_dot_kept_as_written("../app", "/w/../app")]
#[case::under_home("~/src/app", "/home/me/src/app")]
fn parse__should_make_a_path_absolute(#[case] path: &str, #[case] expected: &str) {
    // Given
    let text = format!("slot_dir = \"{path}\"\n");

    // When
    let contents = parse_ok(&text);

    // Then
    assert_eq!(contents.values.slot_dir.unwrap().as_os_str(), expected);
}

#[rstest]
#[case::unknown_key(
    "slots = 2\n",
    "invalid /r/.git/wtpool.toml: line 1: unknown field `slots`, expected one of `foreground`, `slot_dir`, `slot_prefix`, `slot_count`, `remote`, `heavy`, `build_processes`, `copy`, `on_new_slot`, `bounded_waits`"
)]
#[case::table(
    "[pool]\nslot_count = 2\n",
    "invalid /r/.git/wtpool.toml: line 1: unknown field `pool`, expected one of `foreground`, `slot_dir`, `slot_prefix`, `slot_count`, `remote`, `heavy`, `build_processes`, `copy`, `on_new_slot`, `bounded_waits`"
)]
#[case::text_where_a_path_belongs(
    "foreground = 3\n",
    "invalid /r/.git/wtpool.toml: line 1: invalid type: integer `3`, expected a string in `foreground`"
)]
#[case::count_that_is_not_a_number(
    "slot_count = \"3\"\n",
    "invalid /r/.git/wtpool.toml: line 1: invalid type: string \"3\", expected usize in `slot_count`"
)]
#[case::negative_count(
    "slot_count = -1\n",
    "invalid /r/.git/wtpool.toml: line 1: invalid value: integer `-1`, expected usize in `slot_count`"
)]
#[case::not_toml(
    "slot_count =\n",
    "invalid /r/.git/wtpool.toml: line 1: string values must be quoted, expected literal string"
)]
#[case::zero_count(
    "slot_count = 0\n",
    "invalid slot_count='0' in /r/.git/wtpool.toml: must be at least 1"
)]
#[case::empty_path(
    "slot_dir = \"\"\n",
    "invalid slot_dir='' in /r/.git/wtpool.toml: empty"
)]
#[case::empty_prefix(
    "slot_prefix = \"\"\n",
    "invalid slot_prefix='' in /r/.git/wtpool.toml: empty"
)]
#[case::prefix_with_a_slash(
    "slot_prefix = \"../box\"\n",
    "invalid slot_prefix='../box' in /r/.git/wtpool.toml: a slot name cannot contain '/'"
)]
#[case::home_of_another_user(
    "foreground = \"~app/src\"\n",
    "invalid foreground='~app/src' in /r/.git/wtpool.toml: only a leading ~/ is expanded"
)]
#[case::empty_remote("remote = \"\"\n", "invalid remote='' in /r/.git/wtpool.toml: empty")]
#[case::remote_that_is_an_option(
    "remote = \"--upload-pack=x\"\n",
    "invalid remote='--upload-pack=x' in /r/.git/wtpool.toml: not a valid git remote name"
)]
#[case::copy_that_is_not_a_list(
    "copy = \".envrc\"\n",
    "invalid /r/.git/wtpool.toml: line 1: invalid type: string \".envrc\", expected a sequence in `copy`"
)]
#[case::empty_copy_path("copy = [\"\"]\n", "invalid copy='' in /r/.git/wtpool.toml: empty")]
#[case::absolute_copy_path(
    "copy = [\".envrc\", \"/etc/app.env\"]\n",
    "invalid copy='/etc/app.env' in /r/.git/wtpool.toml: must be relative to the main worktree"
)]
#[case::copy_path_that_goes_up(
    "copy = [\"../.env\"]\n",
    "invalid copy='../.env' in /r/.git/wtpool.toml: must not contain '..'"
)]
#[case::empty_command(
    "on_new_slot = \"\"\n",
    "invalid on_new_slot='' in /r/.git/wtpool.toml: empty"
)]
#[case::command_list(
    "on_new_slot = [\"direnv\", \"allow\"]\n",
    "invalid /r/.git/wtpool.toml: line 1: invalid type: sequence, expected a string in `on_new_slot`"
)]
#[case::bad_heavy_pattern(
    "heavy = [\"make *\"]\n",
    "invalid /r/.git/wtpool.toml: line 1: \"make *\" is not a heavy pattern: only the program's name may end in *, and it needs a name before the * in `heavy`"
)]
#[case::the_machine_limit(
    "slot_count = 2\nlimit = 4\n",
    "invalid /r/.git/wtpool.toml: line 2: unknown key `limit`: heavy jobs read it from this machine's config alone; set it with wtpool job limit"
)]
#[case::the_machine_policy(
    "enforce = \"off\"\n",
    "invalid /r/.git/wtpool.toml: line 1: unknown key `enforce`: heavy jobs read it from this machine's config alone; set it with wtpool job enforce"
)]
fn parse__should_refuse_an_invalid_file(#[case] text: &str, #[case] expected: &str) {
    // When
    let error = parse_error(text);

    // Then
    assert_eq!(error, expected);
}

#[test]
fn parse__should_refuse_a_home_path_without_home() {
    // When
    let error = parse(
        "slot_dir = \"~/slots\"\n",
        Path::new(FILE),
        &base(),
        &no_home,
    );

    // Then
    assert_eq!(error.unwrap_err().to_string(), "invalid HOME='': not set");
}

#[rstest]
#[case::unknown(None, "invalid /r/.git/wtpool.toml: oops")]
#[case::known(Some(3), "invalid /r/.git/wtpool.toml: line 3: oops")]
fn config_syntax__should_name_the_line_when_known(
    #[case] line: Option<usize>,
    #[case] expected: &str,
) {
    // Given
    let error = SettingsError::ConfigSyntax {
        path: PathBuf::from(FILE),
        line,
        reason: "oops".to_owned(),
    };

    // When
    let message = error.to_string();

    // Then
    assert_eq!(message, expected);
}

#[test]
fn read__should_return_the_text_of_a_file() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wtpool.toml");
    std::fs::write(&path, "slot_count = 2\n").unwrap();

    // When
    let text = read(&path).unwrap();

    // Then
    assert_eq!(text.as_deref(), Some("slot_count = 2\n"));
}

#[test]
fn read__should_find_nothing_where_there_is_no_file() {
    // Given
    let dir = tempfile::tempdir().unwrap();

    // When
    let text = read(&dir.path().join("wtpool.toml")).unwrap();

    // Then
    assert_eq!(text, None);
}

#[cfg(unix)]
#[rstest]
#[case::symlink(
    |path: &Path| std::os::unix::fs::symlink(path.with_file_name("real.toml"), path).unwrap(),
    "refusing to read {}: it is a symlink"
)]
#[case::directory(
    |path: &Path| std::fs::create_dir(path).unwrap(),
    "could not read {}: not a regular file"
)]
#[case::fifo_that_must_not_block(
    |path: &Path| assert!(std::process::Command::new("mkfifo").arg(path).status().unwrap().success()),
    "could not read {}: not a regular file"
)]
#[case::not_utf8(
    |path: &Path| std::fs::write(path, b"slot_prefix = \"\xff\"\n").unwrap(),
    "could not read {}: not valid UTF-8"
)]
fn read__should_refuse_what_is_not_a_readable_text_file(
    #[case] make: fn(&Path),
    #[case] expected: &str,
) {
    // Given
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("real.toml"), "slot_count = 2\n").unwrap();
    let path = dir.path().join("wtpool.toml");
    make(&path);

    // When
    let error = read(&path);

    // Then
    assert_eq!(
        error.unwrap_err().to_string(),
        expected.replace("{}", &path.display().to_string())
    );
}

#[cfg(unix)]
#[test]
fn read__should_refuse_a_file_this_user_may_not_read() {
    use std::os::unix::fs::PermissionsExt;

    // Root reads a file of mode 000 all the same.
    if rustix::process::geteuid().is_root() {
        return;
    }
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wtpool.toml");
    std::fs::write(&path, "").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

    // When
    let error = read(&path);

    // Then
    assert_eq!(
        error.unwrap_err().to_string(),
        format!(
            "could not read {}: Permission denied (os error 13)",
            path.display()
        )
    );
}
