//! The harness's own folders outside the repository, after the XDG base directories:
//!
//! - the cache, `$XDG_CACHE_HOME/wtpool-verify`, else `~/.cache/wtpool-verify`: reused clones, short
//!   links for VS Code sockets;
//! - the data folder, `$XDG_DATA_HOME/wtpool-verify`, else `~/.local/share/wtpool-verify`: the contract
//!   run's recordings and its build copy of `wtpool`.
//!
//! A variable that is empty or names a relative path counts as unset, as the XDG specification
//! says.

use std::ffi::OsString;
use std::path::PathBuf;

const NAME: &str = "wtpool-verify";

/// The home folder: `HOME`, which every sandbox sets, else the platform's own.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(std::env::home_dir)
}

/// `$XDG_CACHE_HOME/wtpool-verify`, else `~/.cache/wtpool-verify`.
pub fn cache_dir() -> Result<PathBuf, String> {
    base(std::env::var_os("XDG_CACHE_HOME"), ".cache")
}

/// `$XDG_DATA_HOME/wtpool-verify`, else `~/.local/share/wtpool-verify`.
pub fn data_dir() -> Result<PathBuf, String> {
    base(std::env::var_os("XDG_DATA_HOME"), ".local/share")
}

/// The longest Unix socket path VS Code accepts: it refuses a longer one, as `sun_path` holds 104
/// bytes on macOS and 108 on Linux, each with the closing zero.
const SOCKET_LIMIT: usize = if cfg!(target_os = "macos") { 103 } else { 107 };

/// The longest socket name VS Code puts in its user data folder: the first 4 characters of its
/// version, `-`, up to 6 of the socket's type and `.sock`, such as `1.10-main.sock`.
const SOCKET_NAME: usize = 4 + 1 + 6 + 5;

/// Why VS Code cannot open its main socket in the user data folder the oracle gives it, a short
/// link `s/<8 hex>/user-data` in [`cache_dir`] (vscode/test/support/code.mjs), or `None` when it
/// can. It names the variable to shorten. Windows uses named pipes, and Linux with
/// `XDG_RUNTIME_DIR` puts the socket there, so neither needs the check.
pub fn socket_problem() -> Option<String> {
    if cfg!(windows) || (cfg!(target_os = "linux") && std::env::var_os("XDG_RUNTIME_DIR").is_some())
    {
        return None;
    }
    let variable = match std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => "XDG_CACHE_HOME",
        _ => "HOME",
    };
    socket_problem_in(&cache_dir().ok()?, variable)
}

/// [`socket_problem`] for the cache folder `cache`, which `variable` sets.
fn socket_problem_in(cache: &std::path::Path, variable: &str) -> Option<String> {
    let folder = cache.join("s").join("01234567").join("user-data");
    let length = folder.as_os_str().len() + 1 + SOCKET_NAME;
    let over = length.checked_sub(SOCKET_LIMIT).filter(|over| *over > 0)?;
    Some(format!(
        "VS Code's socket in {} would be {length} bytes, over the limit of {SOCKET_LIMIT}: shorten \
         {variable} by {over} bytes or more, or set XDG_CACHE_HOME to a short absolute folder such \
         as /tmp/wtpool-cache",
        folder.display()
    ))
}

fn base(variable: Option<OsString>, default: &str) -> Result<PathBuf, String> {
    if let Some(dir) = variable.map(PathBuf::from).filter(|dir| dir.is_absolute()) {
        return Ok(dir.join(NAME));
    }
    home()
        .map(|home| home.join(default).join(NAME))
        .ok_or_else(|| "no home folder: HOME is not set".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn socket_problem_in__should_name_the_variable_past_the_limit() {
        // Given
        let fits = PathBuf::from("/Users/you/.cache/wtpool-verify");
        let long = PathBuf::from(format!("/{}/wtpool-verify", "x".repeat(80)));

        // When
        let (short, too_long) = (
            socket_problem_in(&fits, "HOME"),
            socket_problem_in(&long, "XDG_CACHE_HOME"),
        );

        // Then
        assert_eq!(short, None);
        let message = too_long.unwrap();
        assert!(message.contains("shorten XDG_CACHE_HOME by "), "{message}");
        assert!(
            message.contains(&format!("over the limit of {SOCKET_LIMIT}")),
            "{message}"
        );
    }

    #[rstest]
    #[case::set(Some("/x/cache"), Some("/x/cache/wtpool-verify"))]
    #[case::unset(None, None)]
    #[case::empty(Some(""), None)]
    #[case::relative(Some("x/cache"), None)]
    fn base__should_follow_the_variable_only_when_absolute(
        #[case] variable: Option<&str>,
        #[case] expected: Option<&str>,
    ) {
        // Given
        let variable = variable.map(OsString::from);

        // When
        let found = base(variable, ".cache");

        // Then
        let expected = match expected {
            Some(dir) => PathBuf::from(dir),
            None => home().unwrap().join(".cache/wtpool-verify"),
        };
        assert_eq!(found, Ok(expected));
    }
}
