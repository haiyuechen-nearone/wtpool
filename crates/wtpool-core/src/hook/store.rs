//! Record files in `wtpool-activity/`: the lock each writer takes, reads that never follow a
//! symlink or block on a FIFO, and writes through a temp file renamed over the record.

use std::fmt;
use std::fs::{File, Metadata, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

use super::record::Record;
use crate::activity::{ACTIVITY_DIR, ActorKey, Stem, read_record_file};
use crate::platform::fs::{open_private, owner, replace_file, same_file};

/// The most attempts a writer makes at a lock that keeps being replaced.
const LOCK_ATTEMPTS: usize = 64;

/// The real, owned `wtpool-activity/` folder of one repository.
#[derive(Debug, Clone)]
pub struct ActivityDir {
    dir: PathBuf,
    owner: Option<u32>,
}

/// The lock of one record, held until dropped.
#[derive(Debug)]
pub struct Locked {
    _file: File,
    pub path: PathBuf,
}

/// The reason a record's lock was not taken.
#[derive(Debug)]
pub enum LockError {
    /// The lock file belongs to another user.
    ForeignOwner,
    /// The lock path names a FIFO, a folder or another entry that is not a regular file.
    NotAFile,
    /// Another process holds the lock, and the caller would not wait for it.
    Held,
    /// The file locked was no longer the one the path names.
    Replaced,
    Io(io::Error),
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ForeignOwner => f.write_str("the lock file belongs to another user"),
            Self::NotAFile => f.write_str("the lock file is not a regular file"),
            Self::Held => f.write_str("another process holds the lock"),
            Self::Replaced => f.write_str("the lock file was replaced while it was locked"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for LockError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Waits for a lock another process holds, or gives up at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wait {
    Block,
    Never,
}

impl ActivityDir {
    /// The folder `<common>/wtpool-activity` when it exists as a real folder, not a symlink. It is
    /// never created here.
    #[must_use]
    pub fn open(common: &Path) -> Option<Self> {
        let dir = common.join(ACTIVITY_DIR);
        let meta = dir.symlink_metadata().ok()?;
        meta.is_dir().then(|| Self {
            dir,
            owner: owner(&meta),
        })
    }

    /// The user who owns the folder, the one the hook runs as, whose files a harness's adapter
    /// may name. `None` where the system tells no owner.
    #[must_use]
    pub fn owner(&self) -> Option<u32> {
        self.owner
    }

    /// `true` when the file `meta` describes belongs to the folder's owner, or when the system
    /// tells no owner, as on Windows. The folder's permissions alone then keep other users' files
    /// out.
    fn owns(&self, meta: &Metadata) -> bool {
        self.owner.is_none() || owner(meta) == self.owner
    }

    #[must_use]
    pub fn is_sandbox(&self) -> bool {
        self.dir.join(".verify-sandbox").symlink_metadata().is_ok()
    }

    #[must_use]
    pub fn file_path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    #[must_use]
    pub fn record_path(&self, stem: &str) -> PathBuf {
        self.dir.join(format!("{stem}.json"))
    }

    /// Takes the lock of the record `stem`, waiting while another process holds it. It tries
    /// again while the lock file found is not the one the path names, as `wtpool hook gc` deleted
    /// it meanwhile, and after an I/O error, up to [`LOCK_ATTEMPTS`] times.
    pub fn lock(&self, stem: &str) -> Result<Locked, LockError> {
        let path = self.lock_path(stem);
        let mut last = LockError::Replaced;
        for _ in 0..LOCK_ATTEMPTS {
            match self.try_lock(&path, Wait::Block) {
                Err(error @ (LockError::Replaced | LockError::Io(_))) => last = error,
                taken => return taken,
            }
        }
        Err(last)
    }

    /// The lock of the record `stem` in one attempt that never waits: [`LockError::Held`] while
    /// another process holds it. gc takes this one, as a record being written is not old.
    pub fn lock_now(&self, stem: &str) -> Result<Locked, LockError> {
        self.try_lock(&self.lock_path(stem), Wait::Never)
    }

    fn lock_path(&self, stem: &str) -> PathBuf {
        self.dir.join(format!("{stem}.json.lock"))
    }

    fn try_lock(&self, path: &Path, wait: Wait) -> Result<Locked, LockError> {
        let file = open_private(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false),
            path,
        )?;
        let held = file.metadata()?;
        if !self.owns(&held) {
            return Err(LockError::ForeignOwner);
        }
        if !held.is_file() {
            return Err(LockError::NotAFile);
        }
        match wait {
            Wait::Block => file.lock()?,
            Wait::Never => file.try_lock().map_err(|error| match error {
                TryLockError::WouldBlock => LockError::Held,
                TryLockError::Error(error) => LockError::Io(error),
            })?,
        }
        let named = path.symlink_metadata()?;
        // Where the system tells no file identity, as on Windows, the lock file still in place
        // counts as the one locked.
        if !same_file(&named, &held).unwrap_or(true) {
            return Err(LockError::Replaced);
        }
        Ok(Locked {
            _file: file,
            path: path.to_path_buf(),
        })
    }

    /// The regular file `name` of the folder, open to read and write, created with mode 0600
    /// when missing. A symlink, FIFO or other entry there that is neither a regular file nor a
    /// folder is replaced, never followed or read. `None` when the file belongs to another
    /// user or cannot be opened.
    #[must_use]
    pub fn open_file(&self, name: &str) -> Option<File> {
        let path = self.file_path(name);
        let mut replaced = false;
        loop {
            let mut options = OpenOptions::new();
            options.read(true).write(true).create(true).truncate(false);
            let replace = match open_private(&mut options, &path) {
                Ok(file) => {
                    let meta = file.metadata().ok()?;
                    if meta.is_file() {
                        return self.owns(&meta).then_some(file);
                    }
                    true
                }
                Err(_) => path
                    .symlink_metadata()
                    .is_ok_and(|meta| !meta.is_file() && !meta.is_dir()),
            };
            if !replace {
                return None;
            }
            std::fs::remove_file(&path).ok()?;
            (!replaced).then_some(())?;
            replaced = true;
        }
    }

    /// The record `stem`, or `None` when [`read_record_file`] refuses its file or
    /// [`Record::from_json`] its text.
    #[must_use]
    pub fn read(&self, stem: &str) -> Option<Record> {
        Record::from_json(&read_record_file(&self.record_path(stem))?)
    }

    /// Writes `record` as `stem` with [`replace_file`], so a symlink or FIFO there is replaced. A
    /// record that cannot fit in [`MAX_RECORD_BYTES`](crate::activity::MAX_RECORD_BYTES) leaves
    /// the file as it was ([`Record::to_json`]).
    pub fn write(&self, stem: &str, record: &Record) -> io::Result<()> {
        replace_file(&self.record_path(stem), &record.to_json()?)
    }

    /// Deletes the record `stem` and its lock, held by the caller.
    pub fn delete(&self, stem: &str, lock: Locked) {
        let _ = std::fs::remove_file(self.record_path(stem));
        let _ = std::fs::remove_file(&lock.path);
        drop(lock);
    }

    /// The record stems in the folder: `<session>` and `<session>.<agent>`.
    #[must_use]
    pub fn stems(&self) -> Vec<String> {
        let mut stems: Vec<String> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
            .filter_map(|name| name.strip_suffix(".json").map(str::to_owned))
            .filter(|stem| is_stem(stem))
            .collect();
        stems.sort();
        stems
    }

    /// The stems of lock files whose record is gone.
    #[must_use]
    pub fn orphan_locks(&self) -> Vec<String> {
        std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
            .filter_map(|name| name.strip_suffix(".json.lock").map(str::to_owned))
            .filter(|stem| is_stem(stem) && self.record_path(stem).symlink_metadata().is_err())
            .collect()
    }

    /// The stems of the records of `actor`'s session: the session's own and its subagents'.
    #[must_use]
    pub fn session_stems(&self, actor: &ActorKey) -> Vec<String> {
        self.stems()
            .into_iter()
            .filter(|stem| Stem::parse(stem).is_some_and(|stem| stem.of_session(actor)))
            .collect()
    }

    /// The stems of the subagents of `actor`'s session.
    #[must_use]
    pub fn subagents_of(&self, actor: &ActorKey) -> Vec<String> {
        self.stems()
            .into_iter()
            .filter(|stem| {
                Stem::parse(stem).is_some_and(|stem| stem.agent.is_some() && stem.of_session(actor))
            })
            .collect()
    }
}

fn is_stem(stem: &str) -> bool {
    Stem::parse(stem).is_some()
}

#[cfg(test)]
pub(crate) mod tests;
