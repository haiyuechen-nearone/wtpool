//! A settings file read for an edit and written back: under a lock, through a temp file renamed
//! over it, never through a symlink.

use std::fs::{File, OpenOptions, Permissions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::{EditError, EditFault};
use crate::platform::fs::{create_private_dir, open_private, open_without_following, replace_file};
use crate::repository::{CONFIG_FILE, LOCK_FILE};

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
            path: common.join(CONFIG_FILE),
            lock: common.join(LOCK_FILE),
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
        let io_fault = |error: io::Error| fail(EditFault::Io(error));
        if let Some(folder) = &self.folder {
            create_private_dir(folder).map_err(io_fault)?;
        }
        let _lock = lock(&self.lock).map_err(|error| EditError {
            path: self.lock.clone(),
            fault: EditFault::Io(error),
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
        Err(error) => return Err(EditFault::Io(error)),
    };
    if meta.is_symlink() {
        return Err(EditFault::Symlink(link_target(path)));
    }
    if !meta.is_file() {
        return Err(EditFault::NotAFile);
    }
    let mut file = open_without_following(path)?;
    let mode = file.metadata()?.permissions();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
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

/// Replaces the file `path` with `text` as [`replace_file`] does, then gives it `mode`, the old
/// file's, through a handle that never follows a symlink. A new file stays private.
fn replace(path: &Path, text: &str, mode: Option<Permissions>) -> io::Result<()> {
    replace_file(path, text.as_bytes())?;
    match mode {
        Some(mode) => open_private(OpenOptions::new().write(true), path)?.set_permissions(mode),
        None => Ok(()),
    }
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
        assert!(matches!(
            refused.unwrap_err().fault,
            EditFault::NotAList("heavy")
        ));
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
        let real = std::fs::canonicalize(&target).unwrap();
        assert!(matches!(fault, EditFault::Symlink(named) if named == real));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "a = 1\n");
    }

    #[cfg(unix)]
    #[test]
    fn edited_file_read__should_refuse_what_it_cannot_look_at() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "").unwrap();
        let under_a_file = blocker.join("inside");
        let folder = dir.path().join("folder");

        // When
        let unlooked = read(&under_a_file);
        std::fs::create_dir(&folder).unwrap();
        let no_file = read(&folder);

        // Then
        assert_eq!(
            unlooked.err().map(|fault| fault.to_string()),
            Some(format!("Not a directory (os error {})", libc::ENOTDIR))
        );
        assert!(matches!(no_file, Err(EditFault::NotAFile)));
    }

    #[cfg(unix)]
    #[test]
    fn edited_file_edit__should_say_where_the_lock_cannot_be_made() {
        // Given
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let lock = dir.path().join(LOCK_FILE);
        std::fs::set_permissions(dir.path(), Permissions::from_mode(0o555)).unwrap();
        // Root writes in a folder no one may write in, so it makes the lock there too.
        let writes_anyway = std::fs::write(dir.path().join("probe"), "").is_ok();

        // When
        let refused = EditedFile::repository(dir.path()).edit(|_| Ok(Some("a = 1\n".to_owned())));

        // Then
        let refusal = refused.as_ref().err().map(|error| {
            let denied = matches!(&error.fault, EditFault::Io(error) if error.kind() == io::ErrorKind::PermissionDenied);
            (&error.path, denied)
        });
        assert_eq!(
            refusal,
            (!writes_anyway).then_some((&lock, true)),
            "{refused:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn edited_file_link_target__should_name_the_target_of_a_link_that_resolves_to_nothing() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("wtpool.toml");
        std::os::unix::fs::symlink("gone/machine.toml", &link).unwrap();

        // When
        let named = link_target(&link);

        // Then
        assert_eq!(named, dir.path().join("gone/machine.toml"));
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
