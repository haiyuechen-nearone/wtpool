//! A settings file read for an edit and written back: under a lock, through a temp file renamed
//! over it, never through a symlink.

use std::fs::{File, OpenOptions, Permissions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use super::{EditError, EditFault};
use crate::platform::fs::{create_private_dir, open_private, open_without_following};

/// A file the settings commands edit, and the lock its writers take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EditedFile {
    path: PathBuf,
    lock: PathBuf,
    /// Created, private, when missing: the machine config's own folder.
    folder: Option<PathBuf>,
}

impl EditedFile {
    /// The machine config at `path`, locked by `machine.toml.lock` beside it.
    pub(super) fn machine(path: &Path) -> Self {
        let mut lock = path.as_os_str().to_owned();
        lock.push(".lock");
        Self {
            path: path.to_path_buf(),
            lock: PathBuf::from(lock),
            folder: path.parent().map(Path::to_path_buf),
        }
    }

    /// The `wtpool.toml` of the common git directory `common`, locked by the pool lock.
    pub(super) fn repository(common: &Path) -> Self {
        Self {
            path: common.join("wtpool.toml"),
            lock: common.join("wtpool.lock"),
            folder: None,
        }
    }

    /// Hands `edit` the file's text, empty when there is none, under the lock, and writes the
    /// text it gives back. `None` leaves the file as it is.
    pub(super) fn edit(
        &self,
        edit: impl FnOnce(&str) -> Result<Option<String>, EditFault>,
    ) -> Result<(), EditError> {
        let fail = |fault| EditError {
            path: self.path.clone(),
            fault,
        };
        let io_fault = |error: io::Error| fail(EditFault::Io(error.to_string()));
        if let Some(folder) = &self.folder {
            create_private_dir(folder).map_err(io_fault)?;
        }
        let _lock = lock(&self.lock).map_err(|error| EditError {
            path: self.lock.clone(),
            fault: EditFault::Io(error.to_string()),
        })?;
        let current = read(&self.path).map_err(fail)?;
        let text = current.as_ref().map_or("", |(text, _)| text.as_str());
        let Some(edited) = edit(text).map_err(fail)? else {
            return Ok(());
        };
        replace(&self.path, &edited, current.map(|(_, mode)| mode)).map_err(io_fault)
    }
}

/// Holds an exclusive lock on the file `path` until the file closes, waiting while another
/// process holds it.
fn lock(path: &Path) -> io::Result<File> {
    let file = open_private(
        OpenOptions::new().create(true).write(true).truncate(false),
        path,
    )?;
    file.lock()?;
    Ok(file)
}

/// The text and the mode of the regular file at `path`, `None` when nothing is there.
fn read(path: &Path) -> Result<Option<(String, Permissions)>, EditFault> {
    let meta = match path.symlink_metadata() {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(EditFault::Io(error.to_string())),
    };
    if meta.is_symlink() {
        return Err(EditFault::Symlink(link_target(path)));
    }
    if !meta.is_file() {
        return Err(EditFault::NotAFile);
    }
    let mut file =
        open_without_following(path).map_err(|error| EditFault::Io(error.to_string()))?;
    let mode = file
        .metadata()
        .map_err(|error| EditFault::Io(error.to_string()))?
        .permissions();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| EditFault::Io(error.to_string()))?;
    let text = String::from_utf8(bytes).map_err(|_| EditFault::NotUtf8)?;
    Ok(Some((text, mode)))
}

/// The file the symlink `link` leads to, as far as it resolves.
fn link_target(link: &Path) -> PathBuf {
    std::fs::canonicalize(link).unwrap_or_else(|_| {
        let target = std::fs::read_link(link).unwrap_or_default();
        link.parent()
            .map_or(target.clone(), |folder| folder.join(&target))
    })
}

/// Writes `text` to a temp file beside `path`, created anew without following a symlink, with
/// `mode` or else private, and renames it over `path`.
fn replace(path: &Path, text: &str, mode: Option<Permissions>) -> io::Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let _ = std::fs::remove_file(&temp);
    let written = write_new(&temp, text, mode).and_then(|()| std::fs::rename(&temp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

/// Closes the file before it returns, so the rename that follows works on Windows too.
fn write_new(path: &Path, text: &str, mode: Option<Permissions>) -> io::Result<()> {
    let mut file = open_private(OpenOptions::new().write(true).create_new(true), path)?;
    file.write_all(text.as_bytes())?;
    if let Some(mode) = mode {
        file.set_permissions(mode)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edited_file_edit__should_create_a_missing_file_and_keep_nothing_else() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/wtpool/machine.toml");
        let file = EditedFile::machine(&path);

        // When
        let created = file.edit(|text| Ok(Some(format!("{text}a = 1\n"))));
        let unchanged = file.edit(|_| Ok(None));
        let refused = file.edit(|_| Err(EditFault::NotAList("heavy")));

        // Then
        assert!(created.is_ok() && unchanged.is_ok());
        assert_eq!(refused.unwrap_err().fault, EditFault::NotAList("heavy"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 1\n");
        let mut left: Vec<String> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["machine.toml", "machine.toml.lock"]);
    }

    #[cfg(unix)]
    #[test]
    fn edited_file_edit__should_refuse_a_symlink_naming_its_target() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("real.toml");
        std::fs::write(&target, "a = 1\n").unwrap();
        std::os::unix::fs::symlink(&target, dir.path().join("wtpool.toml")).unwrap();
        let file = EditedFile::repository(dir.path());

        // When
        let refused = file.edit(|_| Ok(Some(String::new())));

        // Then
        let fault = refused.unwrap_err().fault;
        assert_eq!(
            fault,
            EditFault::Symlink(std::fs::canonicalize(&target).unwrap())
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "a = 1\n");
    }

    #[cfg(unix)]
    #[test]
    fn edited_file_edit__should_keep_the_mode_and_replace_the_file() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wtpool.toml");
        std::fs::write(&path, "a = 1\n").unwrap();
        std::fs::set_permissions(&path, Permissions::from_mode(0o640)).unwrap();
        let before = path.metadata().unwrap().ino();

        // When
        let edited = EditedFile::repository(dir.path()).edit(|_| Ok(Some("a = 2\n".to_owned())));

        // Then
        let meta = path.metadata().unwrap();
        assert!(edited.is_ok());
        assert_eq!(meta.permissions().mode() & 0o777, 0o640);
        assert_ne!(meta.ino(), before);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 2\n");
    }
}
