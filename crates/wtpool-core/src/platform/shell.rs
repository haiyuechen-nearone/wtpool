//! Programs found by name on `PATH`, program names as the system compares them, and the system
//! shell.

use std::ffi::{OsStr, OsString};
use std::fs::Metadata;
use std::path::PathBuf;
use std::process::Command;

/// The file a bare `name` starts: the first runnable file of that name, with the platform's
/// executable suffix, in an absolute entry of `PATH`. A relative or empty entry names a folder
/// under the working folder, which a repository the command runs in could fill, so it is skipped.
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
        .filter(|dir| dir.is_absolute())
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

/// `name` started from the file [`find_on_path`] finds. Without one, the system looks `name` up
/// itself in the absolute entries of `PATH` alone, and reports a missing program as it always
/// does.
#[must_use]
pub fn program(name: &str) -> Command {
    program_in(name, std::env::var_os("PATH").as_deref())
}

fn program_in(name: &str, path: Option<&OsStr>) -> Command {
    if let Some(file) = find_in(name, path) {
        let mut command = Command::new(file);
        // The program sees the name it was called by, as it does from a shell.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::arg0(&mut command, name);
        return command;
    }
    let mut command = Command::new(name);
    match absolute_entries(path) {
        Some(entries) => command.env("PATH", entries),
        None => command.env_remove("PATH"),
    };
    command
}

/// The absolute entries of `path`, a value of `PATH`, as one again; `None` without any.
fn absolute_entries(path: Option<&OsStr>) -> Option<OsString> {
    let entries: Vec<PathBuf> = std::env::split_paths(path?)
        .filter(|dir| dir.is_absolute())
        .collect();
    if entries.is_empty() {
        return None;
    }
    std::env::join_paths(entries).ok()
}

/// `line` run by the system shell: `sh -c`, or `cmd /C` on Windows.
#[cfg(not(windows))]
#[must_use]
pub fn command(line: &str) -> Command {
    let mut command = program("sh");
    command.arg("-c").arg(line);
    command
}

#[cfg(windows)]
#[must_use]
pub fn command(line: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = program("cmd");
    // cmd splits its command line itself, so the line goes through unquoted.
    command.arg("/C").raw_arg(line);
    command
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn must_runnable(file: &Path) {
        if !file.exists() {
            std::fs::write(file, "").unwrap();
        }
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

    /// `target`, canonical, as a path relative to the working folder, so a `PATH` entry of it
    /// stands for the folder a command runs in.
    fn relative_from_cwd(target: &Path) -> PathBuf {
        use std::path::Component;
        let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
        let target = target.canonicalize().unwrap();
        let ups = cwd
            .components()
            .filter(|part| matches!(part, Component::Normal(_)))
            .count();
        let mut relative = PathBuf::new();
        for _ in 0..ups {
            relative.push("..");
        }
        for part in target.components() {
            if let Component::Normal(name) = part {
                relative.push(name);
            }
        }
        relative
    }

    #[test]
    fn find_in__should_skip_relative_and_empty_entries() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let (planted, system) = (dir.path().join("planted"), dir.path().join("system"));
        for folder in [&planted, &system] {
            std::fs::create_dir(folder).unwrap();
        }
        let file = format!("tool{}", std::env::consts::EXE_SUFFIX);
        must_runnable(&planted.join(&file));
        must_runnable(&system.join(&file));
        let relative = relative_from_cwd(&planted);
        assert!(relative.join(&file).is_file(), "{}", relative.display());
        let entries = [relative.as_path(), Path::new(""), system.as_path()];
        let path = std::env::join_paths(entries).unwrap();

        // When
        let found = find_in("tool", Some(&path));

        // Then
        assert_eq!(found, Some(system.join(&file)));
        let relative_only = std::env::join_paths([&relative, Path::new("")]).unwrap();
        assert_eq!(find_in("tool", Some(&relative_only)), None);
    }

    #[cfg(unix)]
    #[test]
    fn program_in__should_not_start_a_git_from_a_relative_entry() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let planted = dir.path().join("planted");
        std::fs::create_dir(&planted).unwrap();
        let ran = dir.path().join("ran");
        let git = planted.join("git");
        std::fs::write(&git, format!("#!/bin/sh\ntouch '{}'\n", ran.display())).unwrap();
        must_runnable(&git);
        let relative = relative_from_cwd(&planted);
        assert!(relative.join("git").is_file(), "{}", relative.display());
        let path = std::env::join_paths([&relative, Path::new("")]).unwrap();

        // When
        let mut command = program_in("git", Some(&path));
        let output = command.arg("--version").output();

        // Then
        assert_eq!(command.get_program(), OsStr::new("git"));
        assert!(!ran.exists(), "the planted git ran: {output:?}");
    }

    #[test]
    fn program_in__should_start_the_file_an_absolute_entry_holds() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let file = dir
            .path()
            .join(format!("tool{}", std::env::consts::EXE_SUFFIX));
        must_runnable(&file);
        let path = std::env::join_paths([Path::new(""), dir.path()]).unwrap();

        // When
        let command = program_in("tool", Some(&path));

        // Then
        assert_eq!(command.get_program(), file.as_os_str());
        assert_eq!(command.get_envs().count(), 0);
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
