use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use crate::config::{Config, Key, SlotCount, SlotSetup};
use crate::config_file::{RepoOnly, Values};
use crate::error::{Error, SettingsError};
use crate::git::GitMode;
use crate::heavy::Pattern;
use crate::names::{RemoteName, SlotPrefix};
use crate::paths::AbsolutePath;
use crate::repository::Repository;

use discover::Main;
pub use files::{RepositoryFiles, RepositoryPolicy};

mod discover;
pub(crate) mod edit;
mod env;
mod files;
mod main_worktree;

/// The pool settings in effect and where each came from.
///
/// Each setting takes the first of: its `WTPOOL_*` environment variable, the
/// repository's `wtpool.toml` in the common git directory, the default. The defaults
/// of `foreground`, `slot_dir` and `slot_prefix` come from the repository's main
/// worktree: the worktree itself, the folder that holds it, and its folder name
/// followed by `-wt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The worktree kept open in the editor, always a worktree of
    /// [`repository`](Self::repository) when that is known.
    pub foreground: Setting<AbsolutePath>,
    /// Where new slots are created.
    pub slot_dir: Setting<AbsolutePath>,
    /// The start of every slot's directory name.
    pub slot_prefix: Setting<SlotPrefix>,
    /// The number of slots `setup` and `bg` fill the pool up to.
    pub slot_count: Setting<SlotCount>,
    /// The remote that pull requests are looked up in, and that branches are
    /// fetched from and track.
    pub remote: Setting<RemoteName>,
    /// The heavy patterns, from the repository's `wtpool.toml` only.
    pub heavy: Vec<Pattern>,
    /// Process names that count as builds, from the repository's `wtpool.toml` only.
    pub build_processes: Vec<String>,
    /// `copy` and `on_new_slot`, from the repository's `wtpool.toml` only.
    pub slot_setup: SlotSetup,
    /// The repository's main worktree. `None` when the repository is bare, or
    /// unknown because `WTPOOL_FOREGROUND` names no repository.
    pub main_worktree: Option<AbsolutePath>,
    /// The common git directory whose `wtpool.toml` was looked for. `None` only when
    /// `WTPOOL_FOREGROUND` names no repository, and the pool commands then report that
    /// themselves. [`Settings::require_repository`] refuses that case.
    pub repository: Option<AbsolutePath>,
}

/// One setting's value and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting<T> {
    /// The value in effect.
    pub value: T,
    /// The variable, file or default that gave [`value`](Self::value).
    pub source: Source,
}

/// Where a setting came from. Displays as `wtpool config` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The environment variable it names.
    Env(&'static str),
    /// A repository's `wtpool.toml`.
    RepoFile(PathBuf),
    /// The repository's main worktree: the default foreground.
    MainWorktree,
    /// The folder that holds the main worktree: the default slot directory.
    MainWorktreeParent,
    /// The main worktree's folder name followed by `-wt`: the default slot prefix.
    MainWorktreeName,
    /// The built in default.
    Default,
}

/// The sources of settings, highest first, read and checked.
#[derive(Debug, Clone, Copy)]
struct Layers<'a> {
    env: &'a Values,
    repo: Option<&'a RepoLayer>,
}

/// A repository's `wtpool.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RepoLayer {
    file: AbsolutePath,
    /// The settings a variable can override.
    values: Values,
    repo_only: RepoOnly,
}

impl Settings {
    /// Reads the settings through `var`, for example [`std::env::var_os`], and
    /// the `wtpool.toml` of the repository found as below, by git in `git_mode`.
    ///
    /// A `WTPOOL_*` variable that is set but empty or not valid Unicode is refused.
    /// Relative paths in variables resolve against the current directory. `HOME`
    /// counts only when it is absolute: without such a `HOME`, the system's home
    /// folder stands in, and a `~/` path is refused when neither gives one.
    ///
    /// The repository is the one `WTPOOL_FOREGROUND` names when it is set, so the
    /// repository of `cwd` then plays no part. Else it is the one holding `cwd`,
    /// found from any of its worktrees, else [`SettingsError::NoRepository`].
    /// `GIT_CEILING_DIRECTORIES` limits the search above `cwd` as it limits git.
    /// A bare repository found there inside a worktree of another repository is
    /// refused with [`SettingsError::RepositoryUnknown`] before its `wtpool.toml` is read.
    ///
    /// A default that comes from the main worktree is refused when there is
    /// none: in a bare repository, and when `WTPOOL_FOREGROUND` names no repository.
    /// A foreground that is not a worktree of the repository is refused, so one
    /// repository's settings never act on another's pool.
    pub fn load(
        var: impl Fn(&str) -> Option<OsString>,
        cwd: Option<&AbsolutePath>,
        git_mode: GitMode,
    ) -> Result<Self, Error> {
        let env = env::env_values(&var)?;
        let home = || env::home_dir(&var);
        let ceiling = var("GIT_CEILING_DIRECTORIES");
        let found = match &env.foreground {
            Some(foreground) => discover::named(foreground, git_mode)?,
            None => discover::enclosing(cwd, ceiling.as_deref(), git_mode)?,
        };
        let repo = match &found.repository {
            Some(repository) => discover::repo_layer(repository, &found.main, &home)?,
            None => None,
        };
        let layers = Layers {
            env: &env,
            repo: repo.as_ref(),
        };
        let repository = found
            .repository
            .map(|repository| repository.common_dir().clone());
        let settings = resolve(layers, &found.main, repository)?;
        // Any other foreground is the one WTPOOL_FOREGROUND names, whose repository
        // this is, or the main worktree, so only wtpool.toml can name a worktree of
        // another one.
        if let Source::RepoFile(_) = settings.foreground.source
            && let Some(repository) = &settings.repository
        {
            settings.check_foreground(repository, git_mode)?;
        }
        Ok(settings)
    }

    /// The pool configuration these settings describe.
    #[must_use]
    pub fn into_config(self) -> Config {
        Config {
            foreground: self.foreground,
            slot_dir: self.slot_dir.value,
            slot_prefix: self.slot_prefix,
            slot_count: self.slot_count,
            remote: self.remote.value,
            main_worktree: self.main_worktree,
            slot_setup: self.slot_setup,
        }
    }

    /// The common git directory of [`repository`](Self::repository). Refused when
    /// `WTPOOL_FOREGROUND` names no repository, so no `wtpool.toml` could be read.
    pub fn require_repository(&self) -> Result<&AbsolutePath, Error> {
        self.repository.as_ref().ok_or_else(|| {
            SettingsError::NoRepositoryAt(self.foreground.value.to_path_buf()).into()
        })
    }

    /// Refuses a foreground that is not a worktree of `repository`, as git in `git_mode` finds.
    fn check_foreground(&self, repository: &AbsolutePath, git_mode: GitMode) -> Result<(), Error> {
        let foreground = &self.foreground;
        let actual = Repository::at(&foreground.value, git_mode)?;
        if actual.as_ref().map(Repository::common_dir) == Some(repository) {
            return Ok(());
        }
        Err(foreground.source.invalid_value(
            Key::Foreground,
            foreground.value.display().to_string(),
            format!("not a worktree of {}", repository.display()),
        ))
    }

    /// Each setting `wtpool config` shows, in its order: the name, the value and its source.
    #[must_use]
    pub fn rows(&self) -> [(&'static str, Value, &Source); 5] {
        [
            (
                Key::Foreground.name(),
                Value::Text(self.foreground.value.display().to_string()),
                &self.foreground.source,
            ),
            (
                Key::SlotDir.name(),
                Value::Text(self.slot_dir.value.display().to_string()),
                &self.slot_dir.source,
            ),
            (
                Key::SlotPrefix.name(),
                Value::Text(self.slot_prefix.value.as_str().to_owned()),
                &self.slot_prefix.source,
            ),
            (
                Key::SlotCount.name(),
                Value::Number(self.slot_count.value.get()),
                &self.slot_count.source,
            ),
            (
                Key::Remote.name(),
                Value::Text(self.remote.value.as_str().to_owned()),
                &self.remote.source,
            ),
        ]
    }
}

/// A setting's value as `wtpool config` prints it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Number(usize),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Text(text) => f.write_str(text),
            Value::Number(number) => write!(f, "{number}"),
        }
    }
}

impl Source {
    /// The error for an invalid `value` of the setting `key`. A value from the
    /// environment or a default is named by its variable, any other by its key.
    pub(crate) fn invalid_value(&self, key: Key, value: String, reason: String) -> Error {
        self.invalid(key.var(), key.name(), value, reason)
    }

    /// [`Source::invalid_value`] for the setting whose variable is `var` and whose
    /// key is `key`.
    pub(crate) fn invalid(
        &self,
        var: &'static str,
        key: &'static str,
        value: String,
        reason: String,
    ) -> Error {
        match self.origin() {
            None => SettingsError::InvalidVariable { var, value, reason },
            Some(origin) => SettingsError::InvalidValue {
                key,
                value,
                origin,
                reason,
            },
        }
        .into()
    }

    /// The words a message names the setting `key` by, with its value as `shown`,
    /// such as `=3`, after its source: `WTPOOL_SLOT_COUNT=3` for the variable `var`,
    /// `the default slot_count=3`, or `slot_count=3 in <file>`.
    pub(crate) fn describe(&self, var: &str, key: &str, shown: &str) -> String {
        match (self, self.origin()) {
            (Source::Default, _) => format!("the default {key}{shown}"),
            (_, None) => format!("{var}{shown}"),
            (_, Some(origin)) => format!("{key}{shown} {origin}"),
        }
    }

    /// Where a value that no variable gave came from, such as `in <file>`.
    fn origin(&self) -> Option<String> {
        match self {
            Source::Env(_) | Source::Default => None,
            Source::RepoFile(file) => Some(format!("in {}", file.display())),
            Source::MainWorktree => Some("from the main worktree".to_owned()),
            Source::MainWorktreeParent => Some("from the parent of the main worktree".to_owned()),
            Source::MainWorktreeName => Some("from the main worktree name".to_owned()),
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Env(var) => write!(f, "env {var}"),
            Source::RepoFile(file) => write!(f, "{}", file.display()),
            Source::MainWorktree => f.write_str("main worktree"),
            Source::MainWorktreeParent => f.write_str("parent of the main worktree"),
            Source::MainWorktreeName => f.write_str("main worktree name + -wt"),
            Source::Default => f.write_str("default"),
        }
    }
}

impl<'a> Layers<'a> {
    /// The environment's or the repository file's value of `key`, in that order.
    /// `get` reads that value from either.
    fn first<T: Clone>(&self, key: Key, get: impl Fn(&Values) -> Option<&T>) -> Option<Setting<T>> {
        if let Some(value) = get(self.env) {
            return Some(Setting::new(value.clone(), Source::Env(key.var())));
        }
        let repo = self.repo?;
        get(&repo.values)
            .map(|value| Setting::new(value.clone(), Source::RepoFile(repo.file.to_path_buf())))
    }
}

impl<T> Setting<T> {
    pub fn new(value: T, source: Source) -> Self {
        Self { value, source }
    }
}

/// Picks each setting from `layers`, falling back to the defaults, with `main` as
/// the source of the ones that come from the main worktree.
fn resolve(
    layers: Layers<'_>,
    main: &Main,
    repository: Option<AbsolutePath>,
) -> Result<Settings, Error> {
    let foreground = match layers.first(Key::Foreground, |values| values.foreground.as_ref()) {
        Some(setting) => setting,
        None => Setting::new(
            main.worktree(Key::Foreground)?.clone(),
            Source::MainWorktree,
        ),
    };
    let slot_dir = match layers.first(Key::SlotDir, |values| values.slot_dir.as_ref()) {
        Some(setting) => setting,
        None => Setting::new(
            main.worktree(Key::SlotDir)?.parent_dir(),
            Source::MainWorktreeParent,
        ),
    };
    let slot_prefix = match layers.first(Key::SlotPrefix, |values| values.slot_prefix.as_ref()) {
        Some(setting) => setting,
        None => Setting::new(
            discover::prefix_of(main.worktree(Key::SlotPrefix)?)?,
            Source::MainWorktreeName,
        ),
    };
    let slot_count = layers
        .first(Key::SlotCount, |values| values.slot_count.as_ref())
        .unwrap_or_else(|| Setting::new(SlotCount::default(), Source::Default));
    let remote = layers
        .first(Key::Remote, |values| values.remote.as_ref())
        .unwrap_or_else(|| Setting::new(RemoteName::default(), Source::Default));
    let RepoOnly {
        heavy,
        build_processes,
        slot_setup,
        ..
    } = layers
        .repo
        .map(|repo| repo.repo_only.clone())
        .unwrap_or_default();
    let main_worktree = match main {
        Main::Worktree(path) => Some(path.clone()),
        Main::Bare { .. } | Main::Unknown(_) => None,
    };
    Ok(Settings {
        foreground,
        slot_dir,
        slot_prefix,
        slot_count,
        remote,
        heavy,
        build_processes,
        slot_setup,
        main_worktree,
        repository,
    })
}

#[cfg(test)]
mod tests;
