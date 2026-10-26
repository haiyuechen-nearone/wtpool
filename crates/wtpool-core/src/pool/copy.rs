//! One `copy` file, from the main worktree into a slot, without following a symlink on either
//! side and without overwriting anything.

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use crate::config::CopyPath;
use crate::config_file;
use crate::error::Error;
use crate::paths::AbsolutePath;
use crate::report::Report;
use crate::run::Git;

/// The kind of a path, read without following a symlink.
#[derive(Debug)]
enum Kind {
    Missing,
    Directory,
    File(Permissions),
    Symlink,
    /// A FIFO, a socket or a device.
    Other,
}

/// Copies `path` from the main worktree `main` into `slot`, unless the slot has it
/// already or the main worktree does not. Warns and copies nothing when git does
/// not ignore it in the main worktree, or when it, or a folder on the way to it on
/// either side, is a symlink or of another kind than expected.
pub(super) fn copy_path(
    main: &AbsolutePath,
    slot: &AbsolutePath,
    path: &CopyPath,
    report: &mut impl Report,
) -> Result<(), Error> {
    let failed = |source: io::Error| Error::Copy {
        path: path.to_string(),
        slot: slot.to_path_buf(),
        source,
    };
    let target = slot.join(path.as_path());
    if !matches!(kind_of(&target).map_err(failed)?, Kind::Missing) {
        return Ok(());
    }
    let permissions = match source_file(main, path.as_path()).map_err(failed)? {
        Source::Missing => return Ok(()),
        Source::Refused(reason) => {
            report.warn(&format!("not copying {path}: {reason}"));
            return Ok(());
        }
        Source::File(permissions) => permissions,
    };
    if !ignored(main, path, report)? {
        report.warn(&format!(
            "not copying {path}: git does not ignore it in {}, so the slot would not stay clean",
            main.display()
        ));
        return Ok(());
    }
    if let Some(reason) = make_folders(slot, path.as_path()).map_err(failed)? {
        report.warn(&format!(
            "not copying {path} into {}: {reason}",
            slot.display()
        ));
        return Ok(());
    }
    write_copy(&main.join(path.as_path()), &target, permissions).map_err(failed)
}

/// The entry of the main worktree at a `copy` path.
#[derive(Debug)]
enum Source {
    Missing,
    /// A symlink or something other than a regular file, for this reason.
    Refused(String),
    File(Permissions),
}

fn kind_of(path: &Path) -> io::Result<Kind> {
    match path.symlink_metadata() {
        Ok(metadata) if metadata.is_symlink() => Ok(Kind::Symlink),
        Ok(metadata) if metadata.is_dir() => Ok(Kind::Directory),
        Ok(metadata) if metadata.is_file() => Ok(Kind::File(metadata.permissions())),
        Ok(_) => Ok(Kind::Other),
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            Ok(Kind::Missing)
        }
        Err(error) => Err(error),
    }
}

/// The regular file at `path` below `main`, reached through real folders only.
fn source_file(main: &Path, path: &Path) -> io::Result<Source> {
    let mut here = main.to_path_buf();
    for part in path.parent().into_iter().flat_map(Path::iter) {
        here.push(part);
        match kind_of(&here)? {
            Kind::Directory => {}
            Kind::Symlink => {
                return Ok(Source::Refused(format!("{} is a symlink", here.display())));
            }
            // A file where a folder would be also leaves the path naming nothing.
            Kind::Missing | Kind::File(_) | Kind::Other => return Ok(Source::Missing),
        }
    }
    let file = main.join(path);
    Ok(match kind_of(&file)? {
        Kind::Missing => Source::Missing,
        Kind::File(permissions) => Source::File(permissions),
        Kind::Symlink => Source::Refused(format!("{} is a symlink", file.display())),
        Kind::Directory | Kind::Other => {
            Source::Refused(format!("{} is not a regular file", file.display()))
        }
    })
}

/// Creates the folders of `path` that `slot` lacks. Returns the reason to stop when one on
/// the way is a symlink or no folder, as the branch checked out there may have it.
fn make_folders(slot: &Path, path: &Path) -> io::Result<Option<String>> {
    let mut here = PathBuf::from(slot);
    for part in path.parent().into_iter().flat_map(Path::iter) {
        here.push(part);
        match kind_of(&here)? {
            Kind::Directory => {}
            Kind::Missing => fs::create_dir(&here)?,
            Kind::Symlink | Kind::File(_) | Kind::Other => {
                return Ok(Some(format!("{} is not a folder", here.display())));
            }
        }
    }
    Ok(None)
}

/// `true` when git ignores `path` in the main worktree `main`. A tracked file is not
/// ignored. Fails when git cannot tell.
fn ignored(main: &AbsolutePath, path: &CopyPath, report: &mut impl Report) -> Result<bool, Error> {
    let ran = Git::at(main).run([
        OsStr::new("check-ignore"),
        OsStr::new("--quiet"),
        OsStr::new("--"),
        path.as_path().as_os_str(),
    ])?;
    match ran.code {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => {
            ran.forward_stderr(report);
            Err(Error::IgnoreUnknown {
                path: path.to_string(),
                main: main.to_path_buf(),
            })
        }
    }
}

/// Copies the file `source` to `target`, a path that must not exist yet, with the
/// permissions `permissions`. A partly written `target` is removed again.
fn write_copy(source: &Path, target: &Path, permissions: Permissions) -> io::Result<()> {
    let mut from = config_file::open_without_following(source)?;
    let mut to = create_new(target)?;
    let copied = io::copy(&mut from, &mut to).and_then(|_| to.set_permissions(permissions));
    if copied.is_err() {
        let _ = fs::remove_file(target);
    }
    copied
}

/// Creates `path`, failing when anything is there, a symlink included. Only the
/// owner can read it until the copy's own permissions are set.
#[cfg(unix)]
fn create_new(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_new(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn kind_of__should_tell_a_fifo_from_a_regular_file() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("fifo");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(made.success());

        // When
        let kind = kind_of(&fifo).unwrap();

        // Then
        assert!(matches!(kind, Kind::Other), "{kind:?}");
    }

    #[test]
    fn write_copy__should_remove_a_target_it_could_not_finish() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let unreadable = dir.path().join("folder");
        fs::create_dir(&unreadable).unwrap();
        let target = dir.path().join("copy");
        let permissions = fs::metadata(dir.path()).unwrap().permissions();

        // When
        let copied = write_copy(&unreadable, &target, permissions);

        // Then
        assert!(copied.is_err());
        assert!(!target.exists());
    }
}
