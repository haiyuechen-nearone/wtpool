//! [`MachineConfig`]: this user's limit, policy and heavy patterns for every job on this machine,
//! from `<config>/wtpool/machine.toml`.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::heavy::Pattern;
use crate::platform::dirs;
use crate::platform::fs::check_private;

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

/// A machine config that cannot be read or is not valid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid machine config {}: {reason}", path.display())]
pub struct MachineConfigError {
    pub path: PathBuf,
    pub reason: String,
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

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Enforce {
    pub const ALL: [Self; 3] = [Self::Off, Self::Warn, Self::Require];

    /// The policy `off`, `warn` or `require` names, as the machine config writes it.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "off" => Some(Self::Off),
            "warn" => Some(Self::Warn),
            "require" => Some(Self::Require),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Warn => "warn",
            Self::Require => "require",
        }
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
        let invalid = |reason: String| MachineConfigError {
            path: path.to_path_buf(),
            reason,
        };
        let Some(text) = read_private(path).map_err(invalid)? else {
            return Ok(Self::defaults(path.to_path_buf(), cores));
        };
        let parsed: MachineToml = toml::from_str(&text).map_err(|error| {
            let line = error.span().map(|span| line_of(&text, span.start));
            let mut detail = error;
            detail.set_input(None);
            let reason = detail.to_string().trim_end().replace('\n', " ");
            invalid(match line {
                Some(line) => format!("line {line}: {reason}"),
                None => reason,
            })
        })?;
        let mut config = Self::defaults(path.to_path_buf(), cores);
        let Some(jobs) = parsed.jobs else {
            return Ok(config);
        };
        if let Some(count) = jobs.limit {
            config.limit = Limit::new(count).ok_or_else(|| {
                invalid(format!(
                    "limit = {count}: the limit is a whole number from {} to {}",
                    LIMIT_BOUNDS.0, LIMIT_BOUNDS.1
                ))
            })?;
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
fn read_private(path: &Path) -> Result<Option<String>, String> {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return match path.symlink_metadata() {
                Ok(_) => Err("it is a symlink to nothing".to_owned()),
                Err(_) => Ok(None),
            };
        }
        Err(error) => return Err(error.to_string()),
    };
    if !meta.is_file() {
        return Err("not a regular file".to_owned());
    }
    check_private(&meta).map_err(|fault| fault.to_string())?;
    let mut folders = vec![path.parent().map(Path::to_path_buf)];
    if path.symlink_metadata().is_ok_and(|link| link.is_symlink()) {
        let target = std::fs::canonicalize(path).map_err(|error| error.to_string())?;
        folders.push(target.parent().map(Path::to_path_buf));
    }
    for folder in folders.into_iter().flatten() {
        let held = std::fs::metadata(&folder).map_err(|error| error.to_string())?;
        check_private(&held)
            .map_err(|fault| format!("its folder {}: {fault}", folder.display()))?;
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "not valid UTF-8".to_owned())
}

/// The line that byte `offset` of `text` is on, counting from 1.
fn line_of(text: &str, offset: usize) -> usize {
    text.bytes()
        .take(offset)
        .filter(|&byte| byte == b'\n')
        .count()
        .saturating_add(1)
}

#[cfg(test)]
mod tests;
