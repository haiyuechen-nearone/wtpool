use std::io::{self, ErrorKind, Read};
use std::path::Path;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::config::{CopyPath, Key, SlotCount, SlotSetup};
use crate::error::{Error, SettingsError};
use crate::names::{RemoteName, SlotPrefix};
use crate::paths::AbsolutePath;
use crate::platform::fs::open_without_following;

/// The settings that a variable and `wtpool.toml` both set, one per [`Key`], with paths made
/// absolute and names checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Values {
    pub foreground: Option<AbsolutePath>,
    pub slot_dir: Option<AbsolutePath>,
    pub slot_prefix: Option<SlotPrefix>,
    pub slot_count: Option<SlotCount>,
    pub remote: Option<RemoteName>,
}

/// The settings that only the repository's `wtpool.toml` sets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RepoOnly {
    pub build_processes: Vec<String>,
    pub slot_setup: SlotSetup,
}

/// A repository's `wtpool.toml`, checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Contents {
    pub values: Values,
    pub repo_only: RepoOnly,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WtpoolToml {
    foreground: Option<String>,
    slot_dir: Option<String>,
    slot_prefix: Option<String>,
    slot_count: Option<usize>,
    remote: Option<String>,
    #[serde(default)]
    build_processes: Vec<String>,
    #[serde(default)]
    copy: Vec<String>,
    on_new_slot: Option<String>,
}

/// Checks the values of `wtpool.toml`.
struct Checker<'a, H> {
    file: &'a Path,
    base: &'a AbsolutePath,
    home: &'a H,
}

/// Parses a repository's `wtpool.toml`. Relative paths resolve against `base`.
pub(crate) fn parse<H>(
    text: &str,
    file: &Path,
    base: &AbsolutePath,
    home: &H,
) -> Result<Contents, Error>
where
    H: Fn() -> Result<AbsolutePath, Error>,
{
    let toml: WtpoolToml = from_toml(text, file)?;
    let check = Checker { file, base, home };
    Ok(Contents {
        values: Values {
            foreground: check.path(Key::Foreground, toml.foreground)?,
            slot_dir: check.path(Key::SlotDir, toml.slot_dir)?,
            slot_prefix: check.prefix(toml.slot_prefix)?,
            slot_count: check.count(toml.slot_count)?,
            remote: check.remote(toml.remote)?,
        },
        repo_only: RepoOnly {
            build_processes: toml.build_processes,
            slot_setup: SlotSetup {
                copy: check.copy(toml.copy)?,
                on_new_slot: check.text("on_new_slot", toml.on_new_slot)?,
            },
        },
    })
}

/// The text of the config file at `path`, `None` when there is none. A symlink,
/// anything but a regular file, and text that is not UTF-8 are refused.
pub(crate) fn read(path: &Path) -> Result<Option<String>, Error> {
    let unreadable = |reason: String| {
        Error::from(SettingsError::ReadConfig {
            path: path.to_path_buf(),
            reason,
        })
    };
    let io_error = |error: io::Error| unreadable(error.to_string());
    let mut file = match open_without_following(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) if path.symlink_metadata().is_ok_and(|meta| meta.is_symlink()) => {
            return Err(SettingsError::ConfigSymlink(path.to_path_buf()).into());
        }
        Err(error) => return Err(io_error(error)),
    };
    if !file.metadata().map_err(io_error)?.is_file() {
        return Err(unreadable("not a regular file".to_owned()));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(io_error)?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| unreadable("not valid UTF-8".to_owned()))
}

fn from_toml<T: DeserializeOwned>(text: &str, file: &Path) -> Result<T, Error> {
    toml::from_str(text).map_err(|error| {
        let line = error.span().map(|span| line_of(text, span.start));
        // Without the input, the message is one line plus the key it concerns.
        let mut detail = error;
        detail.set_input(None);
        let reason = detail.to_string().trim_end().replace('\n', " ");
        SettingsError::ConfigSyntax {
            path: file.to_path_buf(),
            line,
            reason,
        }
        .into()
    })
}

/// The line that byte `offset` of `text` is on, counting from 1.
fn line_of(text: &str, offset: usize) -> usize {
    let newlines = text
        .bytes()
        .take(offset)
        .filter(|&byte| byte == b'\n')
        .count();
    newlines.saturating_add(1)
}

impl<H> Checker<'_, H>
where
    H: Fn() -> Result<AbsolutePath, Error>,
{
    fn invalid(&self, key: &'static str, value: impl ToString, reason: &str) -> Error {
        SettingsError::InvalidValue {
            key,
            value: value.to_string(),
            origin: format!("in {}", self.file.display()),
            reason: reason.to_owned(),
        }
        .into()
    }

    fn text(&self, key: &'static str, value: Option<String>) -> Result<Option<String>, Error> {
        match value {
            Some(value) if value.is_empty() => Err(self.invalid(key, "", "empty")),
            value => Ok(value),
        }
    }

    fn path(&self, key: Key, value: Option<String>) -> Result<Option<AbsolutePath>, Error> {
        let Some(value) = self.text(key.name(), value)? else {
            return Ok(None);
        };
        let joined = if let Some(rest) = value.strip_prefix("~/") {
            (self.home)()?.join(rest)
        } else if value.starts_with('~') {
            return Err(self.invalid(key.name(), &value, "only a leading ~/ is expanded"));
        } else {
            // An absolute value replaces the base when joined.
            self.base.join(&value)
        };
        // Drops `.` components, as a path in a variable loses them. `joined` is
        // absolute, so this cannot fail.
        Ok(Some(AbsolutePath::try_new(&joined).unwrap_or(joined)))
    }

    fn prefix(&self, value: Option<String>) -> Result<Option<SlotPrefix>, Error> {
        let key = Key::SlotPrefix.name();
        self.text(key, value)?
            .map(|prefix| {
                SlotPrefix::try_new(&prefix)
                    .map_err(|error| self.invalid(key, &prefix, &error.to_string()))
            })
            .transpose()
    }

    fn count(&self, value: Option<usize>) -> Result<Option<SlotCount>, Error> {
        value
            .map(|count| {
                SlotCount::try_new(count)
                    .map_err(|error| self.invalid(Key::SlotCount.name(), count, &error.to_string()))
            })
            .transpose()
    }

    fn remote(&self, value: Option<String>) -> Result<Option<RemoteName>, Error> {
        let key = Key::Remote.name();
        self.text(key, value)?
            .map(|remote| {
                RemoteName::try_new(&remote)
                    .map_err(|error| self.invalid(key, &remote, &error.to_string()))
            })
            .transpose()
    }

    fn copy(&self, paths: Vec<String>) -> Result<Vec<CopyPath>, Error> {
        paths
            .into_iter()
            .map(|path| {
                CopyPath::try_new(&path)
                    .map_err(|error| self.invalid("copy", &path, &error.to_string()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
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
        let text = "foreground = \"/srv/app\"\nslot_dir = \"slots\"\nslot_prefix = \"box\"\nslot_count = 5\nremote = \"upstream\"\nbuild_processes = [\"make\"]\ncopy = [\".envrc\", \"./config/.env\"]\non_new_slot = \"direnv allow\"\n";

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
                    build_processes: vec!["make".to_owned()],
                    slot_setup: SlotSetup {
                        copy: vec![
                            CopyPath::try_new(".envrc").unwrap(),
                            CopyPath::try_new("config/.env").unwrap(),
                        ],
                        on_new_slot: Some("direnv allow".to_owned()),
                    },
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
        "invalid /r/.git/wtpool.toml: line 1: unknown field `slots`, expected one of `foreground`, `slot_dir`, `slot_prefix`, `slot_count`, `remote`, `build_processes`, `copy`, `on_new_slot`"
    )]
    #[case::table(
        "[pool]\nslot_count = 2\n",
        "invalid /r/.git/wtpool.toml: line 1: unknown field `pool`, expected one of `foreground`, `slot_dir`, `slot_prefix`, `slot_count`, `remote`, `build_processes`, `copy`, `on_new_slot`"
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
    #[case::first_line("a\nb\n", 0, 1)]
    #[case::later_line("a\nb\n", 2, 2)]
    #[case::inside_a_character("é\n", 1, 1)]
    fn line_of__should_count_the_lines_before_the_offset(
        #[case] text: &str,
        #[case] offset: usize,
        #[case] expected: usize,
    ) {
        // When
        let line = line_of(text, offset);

        // Then
        assert_eq!(line, expected);
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
    #[case::not_readable(
        |path: &Path| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(path, "").unwrap();
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
        },
        "could not read {}: Permission denied (os error 13)"
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
}
