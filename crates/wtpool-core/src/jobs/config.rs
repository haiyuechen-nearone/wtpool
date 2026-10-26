//! [`MachineConfig`]: this user's limit, policy and heavy patterns for every job on this machine,
//! from `<config>/wtpool/machine.toml`.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::at_line;
use crate::heavy::Pattern;
use crate::platform::dirs;
use crate::platform::fs::{NotPrivate, check_private};
use crate::toml_error::TomlError;

/// The lowest and highest limit the machine config takes.
pub const LIMIT_BOUNDS: (u32, u32) = (1, 64);
/// The highest default limit, whatever the core count.
pub const MAX_DEFAULT_LIMIT: u32 = 8;

/// How many jobs run at once, from 1 to 64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Limit(u32);

/// The policy for heavy commands outside a job.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Enforce {
    Off,
    #[default]
    Warn,
    Require,
}

/// Where a setting of the machine config comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigSource {
    Machine,
    Default,
}

/// The machine config in effect: the file's values, the defaults where it has none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineConfig {
    pub path: PathBuf,
    pub limit: Limit,
    pub limit_source: ConfigSource,
    pub default_limit: Limit,
    pub enforce: Enforce,
    pub enforce_source: ConfigSource,
    pub heavy: Vec<Pattern>,
}

/// Text that is not a limit: no whole number, or one outside [`LIMIT_BOUNDS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the limit is a whole number from {} to {}", LIMIT_BOUNDS.0, LIMIT_BOUNDS.1)]
pub struct InvalidLimit;

/// Text that names no [`Enforce`] policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the policy is off, warn or require")]
pub struct InvalidEnforce;

/// A machine config that cannot be read or is not valid.
#[derive(Debug, thiserror::Error)]
#[error("invalid machine config {}: {fault}", path.display())]
pub struct MachineConfigError {
    pub path: PathBuf,
    pub fault: MachineConfigFault,
}

/// The fault of a [`MachineConfigError`].
#[derive(Debug, thiserror::Error)]
pub enum MachineConfigFault {
    #[error("it is a symlink to nothing")]
    DanglingSymlink,
    #[error("not a regular file")]
    NotAFile,
    #[error("{0}")]
    NotPrivate(NotPrivate),
    #[error("its folder {}: {fault}", folder.display())]
    FolderNotPrivate { folder: PathBuf, fault: NotPrivate },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("not valid UTF-8")]
    NotUtf8,
    #[error("{}{reason}", at_line(*line))]
    Syntax { line: Option<usize>, reason: String },
    #[error("limit = {count}: {}", InvalidLimit)]
    Limit { count: i64 },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MachineToml {
    jobs: Option<JobsTable>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JobsTable {
    limit: Option<i64>,
    enforce: Option<Enforce>,
    #[serde(default)]
    heavy: Vec<Pattern>,
}

impl Limit {
    /// `count` when it lies within [`LIMIT_BOUNDS`].
    #[must_use]
    pub fn new(count: i64) -> Option<Self> {
        let count = u32::try_from(count).ok()?;
        (LIMIT_BOUNDS.0..=LIMIT_BOUNDS.1)
            .contains(&count)
            .then_some(Self(count))
    }

    /// A quarter of `cores`, rounded down and kept from 1 to 8.
    #[must_use]
    pub fn default_for(cores: usize) -> Self {
        let quarter = u32::try_from(cores / 4).unwrap_or(u32::MAX);
        Self(quarter.clamp(LIMIT_BOUNDS.0, MAX_DEFAULT_LIMIT))
    }

    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

impl FromStr for Limit {
    type Err = InvalidLimit;

    /// A limit written in decimal digits alone, without a sign.
    fn from_str(text: &str) -> Result<Self, InvalidLimit> {
        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(InvalidLimit);
        }
        text.parse().ok().and_then(Self::new).ok_or(InvalidLimit)
    }
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Enforce {
    pub const ALL: [Self; 3] = [Self::Off, Self::Warn, Self::Require];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Warn => "warn",
            Self::Require => "require",
        }
    }
}

impl FromStr for Enforce {
    type Err = InvalidEnforce;

    fn from_str(name: &str) -> Result<Self, InvalidEnforce> {
        Self::ALL
            .into_iter()
            .find(|policy| policy.as_str() == name)
            .ok_or(InvalidEnforce)
    }
}

impl fmt::Display for Enforce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ConfigSource {
    pub const ALL: [Self; 2] = [Self::Machine, Self::Default];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Machine => "machine",
            Self::Default => "default",
        }
    }
}

impl fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `<config>/wtpool/machine.toml`, `<config>` the [`dirs::config_folder`] of `config_home` and
/// `home`. `None` without an absolute folder for it.
#[must_use]
pub fn machine_config_path(config_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    Some(dirs::config_folder(config_home, home)?.join("wtpool/machine.toml"))
}

impl MachineConfig {
    /// The defaults for a machine of `cores` cores, as a missing file at `path` gives them.
    #[must_use]
    pub fn defaults(path: PathBuf, cores: usize) -> Self {
        let default_limit = Limit::default_for(cores);
        Self {
            path,
            limit: default_limit,
            limit_source: ConfigSource::Default,
            default_limit,
            enforce: Enforce::default(),
            enforce_source: ConfigSource::Default,
            heavy: Vec::new(),
        }
    }

    /// The config at `path` on a machine of `cores` cores, the defaults where the file or a key
    /// is missing. Reading follows a symlink to a regular file. The file and the folders that
    /// hold it and its target must belong to this user and be writable by no one else.
    pub fn read(path: &Path, cores: usize) -> Result<Self, MachineConfigError> {
        let invalid = |fault: MachineConfigFault| MachineConfigError {
            path: path.to_path_buf(),
            fault,
        };
        let Some(text) = read_private(path).map_err(invalid)? else {
            return Ok(Self::defaults(path.to_path_buf(), cores));
        };
        let parsed: MachineToml = toml::from_str(&text).map_err(|error| {
            let TomlError { line, reason } = TomlError::new(error, &text);
            invalid(MachineConfigFault::Syntax { line, reason })
        })?;
        let mut config = Self::defaults(path.to_path_buf(), cores);
        let Some(jobs) = parsed.jobs else {
            return Ok(config);
        };
        if let Some(count) = jobs.limit {
            config.limit =
                Limit::new(count).ok_or_else(|| invalid(MachineConfigFault::Limit { count }))?;
            config.limit_source = ConfigSource::Machine;
        }
        if let Some(enforce) = jobs.enforce {
            config.enforce = enforce;
            config.enforce_source = ConfigSource::Machine;
        }
        config.heavy = jobs.heavy;
        Ok(config)
    }
}

/// The text of the private regular file at `path`, `None` when nothing is there.
fn read_private(path: &Path) -> Result<Option<String>, MachineConfigFault> {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return match path.symlink_metadata() {
                Ok(_) => Err(MachineConfigFault::DanglingSymlink),
                Err(_) => Ok(None),
            };
        }
        Err(error) => return Err(error.into()),
    };
    if !meta.is_file() {
        return Err(MachineConfigFault::NotAFile);
    }
    check_private(&meta).map_err(MachineConfigFault::NotPrivate)?;
    let mut folders = vec![path.parent().map(Path::to_path_buf)];
    if path.symlink_metadata().is_ok_and(|link| link.is_symlink()) {
        folders.push(std::fs::canonicalize(path)?.parent().map(Path::to_path_buf));
    }
    for folder in folders.into_iter().flatten() {
        let held = std::fs::metadata(&folder)?;
        check_private(&held)
            .map_err(|fault| MachineConfigFault::FolderNotPrivate { folder, fault })?;
    }
    String::from_utf8(std::fs::read(path)?)
        .map(Some)
        .map_err(|_| MachineConfigFault::NotUtf8)
}

#[cfg(test)]
mod tests;
