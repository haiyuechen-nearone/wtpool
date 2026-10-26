//! Programs found by name on `PATH`, program names as the system compares them, and the system
//! shell.

use std::ffi::OsStr;
use std::fs::Metadata;
use std::path::PathBuf;
use std::process::Command;

/// The file a bare `name` starts: the first runnable file of that name, with the platform's
/// executable suffix, in an entry of `PATH`.
#[must_use]
pub fn find_on_path(name: &str) -> Option<PathBuf> {
    find_in(name, std::env::var_os("PATH").as_deref())
}

/// [`find_on_path`] with `path` as the value of `PATH`.
///
/// On Windows only the `.exe` suffix counts, not each one of `PATHEXT`: [`Command`] starts no
/// other file by a bare name.
#[must_use]
pub fn find_in(name: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    find_with_suffix(name, std::env::consts::EXE_SUFFIX, path)
}

fn find_with_suffix(name: &str, suffix: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    let file = format!("{name}{suffix}");
    std::env::split_paths(path?)
        .map(|dir| dir.join(&file))
        .find(|candidate| {
            candidate
                .metadata()
                .is_ok_and(|meta| meta.is_file() && runnable(&meta))
        })
}

#[cfg(unix)]
fn runnable(meta: &Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

/// Windows runs a file by its suffix alone.
#[cfg(not(unix))]
fn runnable(_: &Metadata) -> bool {
    true
}

/// `name`, a program's file name, as this system compares program names: as it is, and on Windows
/// without a final `.exe` and in lower case, as Windows finds files without case.
#[cfg(not(windows))]
#[must_use]
pub fn program_key(name: &str) -> String {
    name.to_owned()
}

#[cfg(windows)]
#[must_use]
pub fn program_key(name: &str) -> String {
    let lower = name.to_lowercase();
    lower
        .strip_suffix(".exe")
        .map_or_else(|| lower.clone(), str::to_owned)
}

/// `line` run by the system shell: `sh -c`, or `cmd /C` on Windows.
#[cfg(not(windows))]
#[must_use]
pub fn command(line: &str) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(line);
    command
}

#[cfg(windows)]
#[must_use]
pub fn command(line: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("cmd");
    // cmd splits its command line itself, so the line goes through unquoted.
    command.arg("/C").raw_arg(line);
    command
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn must_runnable(file: &Path) {
        std::fs::write(file, "").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn find_with_suffix__should_find_the_first_runnable_file_with_the_suffix() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let folders = ["empty", "bare", "first", "second"].map(|name| dir.path().join(name));
        for folder in &folders {
            std::fs::create_dir(folder).unwrap();
        }
        let [empty, bare, first, second] = &folders;
        must_runnable(&bare.join("tool"));
        must_runnable(&first.join("tool.exe"));
        must_runnable(&second.join("tool.exe"));
        let path = std::env::join_paths(&folders).unwrap();

        // When
        let found = find_with_suffix("tool", ".exe", Some(&path));

        // Then
        assert_eq!(found, Some(first.join("tool.exe")));
        assert_eq!(
            find_with_suffix("tool", ".exe", Some(empty.as_os_str())),
            None
        );
        assert_eq!(find_with_suffix("tool", ".exe", None), None);
    }

    #[cfg(unix)]
    #[test]
    fn find_with_suffix__should_skip_a_file_nobody_may_run() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let (plain, runnable) = (dir.path().join("plain"), dir.path().join("runnable"));
        for folder in [&plain, &runnable] {
            std::fs::create_dir(folder).unwrap();
        }
        std::fs::write(plain.join("tool"), "").unwrap();
        must_runnable(&runnable.join("tool"));
        let path = std::env::join_paths([&plain, &runnable]).unwrap();

        // When
        let found = find_with_suffix("tool", "", Some(&path));

        // Then
        assert_eq!(found, Some(runnable.join("tool")));
    }

    #[test]
    fn program_key__should_fold_names_as_the_system_finds_files() {
        // When
        let keys = ["cargo", "cargo.exe", "CARGO.EXE"].map(program_key);

        // Then
        let expected = if cfg!(windows) {
            ["cargo", "cargo", "cargo"]
        } else {
            ["cargo", "cargo.exe", "CARGO.EXE"]
        };
        assert_eq!(keys, expected);
    }

    #[cfg(unix)]
    #[test]
    fn command__should_run_the_line_through_sh() {
        // Given
        let mut command = command("echo \"$0\" | tr a-z A-Z");

        // When
        let output = command.output().unwrap();

        // Then
        assert!(output.status.success());
        assert_eq!(output.stdout, b"SH\n");
    }
}
