//! The user's home, config and state folders.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The home folder: `home`, the value of `HOME`, else the folder `system` names, as on Windows,
/// where `HOME` is usually unset. `None` when neither is absolute.
///
/// An empty or relative `HOME` counts as unset, so no path built on it resolves against the
/// current folder, as that may be a worktree.
#[must_use]
pub fn home_folder(
    home: Option<OsString>,
    system: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    home.map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .or_else(|| system().filter(|folder| folder.is_absolute()))
}

/// The [`home_folder`] of the `HOME` that `var` reads, such as [`std::env::var_os`].
#[must_use]
pub fn home_from(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    home_folder(var("HOME"), std::env::home_dir)
}

/// The [`home_folder`] of this process.
#[must_use]
pub fn home() -> Option<PathBuf> {
    home_from(|name| std::env::var_os(name))
}

/// The folder for state files: `state_home`, the value of `XDG_STATE_HOME`, else `.local/state`
/// in `home`. A relative or empty folder counts as unset, as in [`home_folder`].
#[must_use]
pub fn state_folder(state_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    xdg_folder(state_home, home, ".local/state")
}

/// The folder for config files: `config_home`, the value of `XDG_CONFIG_HOME`, else `.config` in
/// `home`, as [`state_folder`] reads its own.
#[must_use]
pub fn config_folder(config_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    xdg_folder(config_home, home, ".config")
}

/// `xdg_home` when absolute, else `under_home` in an absolute `home`.
fn xdg_folder(xdg_home: Option<&Path>, home: Option<&Path>, under_home: &str) -> Option<PathBuf> {
    xdg_home
        .filter(|path| path.is_absolute())
        .map(Path::to_path_buf)
        .or_else(|| {
            home.filter(|path| path.is_absolute())
                .map(|home| home.join(under_home))
        })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    /// A folder that is absolute on every platform: `/home/me` names no drive on Windows.
    fn absolute(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[rstest]
    #[case::home(Some(absolute("me")), Some(absolute("system")), Some(absolute("me")))]
    #[case::system_home_without_home(None, Some(absolute("system")), Some(absolute("system")))]
    #[case::empty_home(
        Some(PathBuf::new()),
        Some(absolute("system")),
        Some(absolute("system"))
    )]
    #[case::relative_home(
        Some(PathBuf::from("me")),
        Some(absolute("system")),
        Some(absolute("system"))
    )]
    #[case::empty_home_without_a_system_home(Some(PathBuf::new()), None, None)]
    #[case::relative_home_and_system_home(
        Some(PathBuf::from("me")),
        Some(PathBuf::from("me")),
        None
    )]
    #[case::relative_system_home(None, Some(PathBuf::from("home")), None)]
    #[case::no_home_at_all(None, None, None)]
    fn home_folder__should_take_an_absolute_home_else_the_system_home_folder(
        #[case] home: Option<PathBuf>,
        #[case] system: Option<PathBuf>,
        #[case] expected: Option<PathBuf>,
    ) {
        // When
        let folder = home_folder(home.map(OsString::from), || system);

        // Then
        assert_eq!(folder, expected);
    }

    #[rstest]
    #[case::state_home(Some(absolute("s")), Some(absolute("h")), Some(absolute("s")))]
    #[case::relative_state_home(
        Some(PathBuf::from("s")),
        Some(absolute("h")),
        Some(absolute("h").join(".local/state"))
    )]
    #[case::home_only(None, Some(absolute("h")), Some(absolute("h").join(".local/state")))]
    #[case::relative_home(None, Some(PathBuf::from("h")), None)]
    #[case::empty_home(None, Some(PathBuf::new()), None)]
    #[case::relative_state_home_and_home(Some(PathBuf::from("s")), Some(PathBuf::from("h")), None)]
    #[case::neither(None, None, None)]
    fn state_folder__should_follow_xdg(
        #[case] state_home: Option<PathBuf>,
        #[case] home: Option<PathBuf>,
        #[case] expected: Option<PathBuf>,
    ) {
        // When
        let folder = state_folder(state_home.as_deref(), home.as_deref());

        // Then
        assert_eq!(folder, expected);
    }

    #[rstest]
    #[case::config_home(Some(absolute("c")), Some(absolute("h")), Some(absolute("c")))]
    #[case::relative_config_home(
        Some(PathBuf::from("c")),
        Some(absolute("h")),
        Some(absolute("h").join(".config"))
    )]
    #[case::empty_config_home(
        Some(PathBuf::new()),
        Some(absolute("h")),
        Some(absolute("h").join(".config"))
    )]
    #[case::home_only(None, Some(absolute("h")), Some(absolute("h").join(".config")))]
    #[case::neither(None, Some(PathBuf::from("h")), None)]
    fn config_folder__should_follow_xdg(
        #[case] config_home: Option<PathBuf>,
        #[case] home: Option<PathBuf>,
        #[case] expected: Option<PathBuf>,
    ) {
        // When
        let folder = config_folder(config_home.as_deref(), home.as_deref());

        // Then
        assert_eq!(folder, expected);
    }
}
