use std::io::{self, ErrorKind, Read};
use std::path::Path;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::config::{CopyPath, Key, SlotCount, SlotSetup};
use crate::error::{Error, ReadFault, SettingsError};
use crate::heavy::Pattern;
use crate::names::{RemoteName, SlotPrefix};
use crate::paths::AbsolutePath;
use crate::platform::fs::open_without_following;
use crate::toml_error::TomlError;

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
    pub heavy: Vec<Pattern>,
    pub build_processes: Vec<String>,
    pub slot_setup: SlotSetup,
    pub bounded_waits: Option<bool>,
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
    heavy: Vec<Pattern>,
    #[serde(default)]
    build_processes: Vec<String>,
    #[serde(default)]
    copy: Vec<String>,
    on_new_slot: Option<String>,
    bounded_waits: Option<bool>,
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
            heavy: toml.heavy,
            build_processes: toml.build_processes,
            slot_setup: SlotSetup {
                copy: check.copy(toml.copy)?,
                on_new_slot: check.text("on_new_slot", toml.on_new_slot)?,
            },
            bounded_waits: toml.bounded_waits,
        },
    })
}

/// The text of the config file at `path`, `None` when there is none. A symlink,
/// anything but a regular file, and text that is not UTF-8 are refused.
pub(crate) fn read(path: &Path) -> Result<Option<String>, Error> {
    let unreadable = |fault: ReadFault| {
        Error::from(SettingsError::ReadConfig {
            path: path.to_path_buf(),
            fault,
        })
    };
    let io_error = |error: io::Error| unreadable(ReadFault::Io(error));
    let mut file = match open_without_following(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) if path.symlink_metadata().is_ok_and(|meta| meta.is_symlink()) => {
            return Err(SettingsError::ConfigSymlink(path.to_path_buf()).into());
        }
        Err(error) => return Err(io_error(error)),
    };
    if !file.metadata().map_err(io_error)?.is_file() {
        return Err(unreadable(ReadFault::NotAFile));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(io_error)?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| unreadable(ReadFault::NotUtf8))
}

/// The keys of the machine config that a repository's `wtpool.toml` may not set.
const MACHINE_KEYS: [&str; 2] = ["limit", "enforce"];

fn from_toml<T: DeserializeOwned>(text: &str, file: &Path) -> Result<T, Error> {
    toml::from_str(text).map_err(|error| {
        let TomlError { line, reason } = TomlError::new(error, text);
        if let Some(key) = machine_key(text) {
            return SettingsError::MachineKey {
                path: file.to_path_buf(),
                line,
                key,
            }
            .into();
        }
        SettingsError::ConfigSyntax {
            path: file.to_path_buf(),
            line,
            reason,
        }
        .into()
    })
}

/// The first key of [`MACHINE_KEYS`] at the top level of the TOML `text`.
fn machine_key(text: &str) -> Option<&'static str> {
    let table: toml::Table = toml::from_str(text).ok()?;
    MACHINE_KEYS
        .into_iter()
        .find(|key| table.contains_key(*key))
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
mod tests;
