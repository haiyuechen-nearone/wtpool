//! The `WTPOOL_*` variables and `HOME`.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::config::{Key, SlotCount};
use crate::config_file::Values;
use crate::error::{Error, SettingsError};
use crate::names::{RemoteName, SlotPrefix};
use crate::paths::AbsolutePath;
use crate::platform::dirs;

const HOME: &str = "HOME";

/// The `WTPOOL_*` variables, each checked on its own.
pub(super) fn env_values(var: &impl Fn(&str) -> Option<OsString>) -> Result<Values, Error> {
    let foreground = foreground(var)?;
    let slot_dir = set(var, Key::SlotDir)?
        .map(|path| absolute(Key::SlotDir, &path))
        .transpose()?;
    let slot_prefix = set(var, Key::SlotPrefix)?
        .map(|prefix| {
            SlotPrefix::try_new(&prefix)
                .map_err(|error| invalid(Key::SlotPrefix, prefix.clone(), error.to_string()))
        })
        .transpose()?;
    let slot_count = set(var, Key::SlotCount)?
        .map(|count| {
            count
                .trim()
                .parse::<SlotCount>()
                .map_err(|error| invalid(Key::SlotCount, count.clone(), error.to_string()))
        })
        .transpose()?;
    let remote = set(var, Key::Remote)?
        .map(|remote| {
            RemoteName::try_new(&remote)
                .map_err(|error| invalid(Key::Remote, remote.clone(), error.to_string()))
        })
        .transpose()?;
    Ok(Values {
        foreground,
        slot_dir,
        slot_prefix,
        slot_count,
        remote,
    })
}

/// `WTPOOL_FOREGROUND`, checked as [`env_values`] checks it.
pub(super) fn foreground(
    var: &impl Fn(&str) -> Option<OsString>,
) -> Result<Option<AbsolutePath>, Error> {
    set(var, Key::Foreground)?
        .map(|path| absolute(Key::Foreground, &path))
        .transpose()
}

/// The variable of `key`, `None` when unset. Set but empty or not Unicode is an error.
fn set(var: &impl Fn(&str) -> Option<OsString>, key: Key) -> Result<Option<String>, Error> {
    match var(key.var()) {
        None => Ok(None),
        Some(value) if value.is_empty() => Err(invalid(key, String::new(), "empty".to_owned())),
        Some(value) => value.into_string().map(Some).map_err(|value| {
            invalid(
                key,
                value.to_string_lossy().into_owned(),
                "not valid Unicode".to_owned(),
            )
        }),
    }
}

/// The home folder as [`dirs::home_from`] finds it through `var`, refused when there is none, so
/// a `~/` path never points somewhere else.
pub(super) fn home_dir(var: &impl Fn(&str) -> Option<OsString>) -> Result<AbsolutePath, Error> {
    checked_home(var(HOME), dirs::home_from(var))
}

/// `found`, the home folder found for the value `home` of `HOME`, refused naming that value
/// when there is none.
pub(super) fn checked_home(
    home: Option<OsString>,
    found: Option<PathBuf>,
) -> Result<AbsolutePath, Error> {
    if let Some(folder) = found.and_then(|folder| AbsolutePath::try_new(folder).ok()) {
        return Ok(folder);
    }
    let (value, reason) = match home {
        Some(home) if !home.is_empty() => (home.to_string_lossy().into_owned(), "not absolute"),
        _ => (String::new(), "not set"),
    };
    Err(SettingsError::InvalidVariable {
        var: HOME,
        value,
        reason: reason.to_owned(),
    }
    .into())
}

fn absolute(key: Key, path: &str) -> Result<AbsolutePath, Error> {
    AbsolutePath::try_new(path).map_err(|error| invalid(key, path.to_owned(), error.to_string()))
}

fn invalid(key: Key, value: String, reason: String) -> Error {
    SettingsError::InvalidVariable {
        var: key.var(),
        value,
        reason,
    }
    .into()
}
