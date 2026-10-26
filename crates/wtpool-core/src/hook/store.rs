//! Record files in `wtpool-activity/`: the lock each writer takes (H24), reads that never follow a
//! symlink or block on a FIFO, and writes through a temp file renamed over the record (H18).

use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::record::Record;
use crate::activity::{ACTIVITY_DIR, ActorKey, Stem, read_record_file};

/// How often a writer takes the lock again after finding it replaced.
const LOCK_ATTEMPTS: usize = 64;

/// The real, owned `wtpool-activity/` folder of one repository.
#[derive(Debug, Clone)]
pub struct Activity {
    dir: PathBuf,
    owner: u32,
}

/// The lock of one record, held until dropped.
#[derive(Debug)]
pub struct Locked {
    _file: File,
    pub path: PathBuf,
}

impl Activity {
    /// The folder `<common>/wtpool-activity` when it exists as a real folder, not a symlink (H6).
    /// It is never created here.
    #[must_use]
    pub fn open(common: &Path) -> Option<Self> {
        let dir = common.join(ACTIVITY_DIR);
        let meta = dir.symlink_metadata().ok()?;
        meta.is_dir().then(|| Self {
            dir,
            owner: owner(&meta),
        })
    }

    /// The user who owns the folder, whom H6 makes the one the hook runs as, and whose
    /// files a harness's adapter may name (H27).
    #[must_use]
    pub fn owner(&self) -> u32 {
        self.owner
    }

    /// Whether the folder marks a sandbox pool (H6).
    #[must_use]
    pub fn is_sandbox(&self) -> bool {
        self.dir.join(".verify-sandbox").symlink_metadata().is_ok()
    }

    /// The path of the file `name` in the folder.
    #[must_use]
    pub fn file_path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    #[must_use]
    pub fn record_path(&self, stem: &str) -> PathBuf {
        self.dir.join(format!("{stem}.json"))
    }

    /// Takes the lock of the record `stem`, again while the lock file found is not the one the
    /// path names, as `wtpool hook gc` deleted it meanwhile, up to [`LOCK_ATTEMPTS`] times. `None`
    /// when the folder belongs to another user or the lock cannot be taken.
    #[must_use]
    pub fn lock(&self, stem: &str) -> Option<Locked> {
        let path = self.dir.join(format!("{stem}.json.lock"));
        (0..LOCK_ATTEMPTS).find_map(|_| self.try_lock(&path, true))
    }

    /// The lock of the record `stem` in one attempt that never waits: `None` while another
    /// process holds it. gc takes this one, as a record being written is not old (H21).
    #[must_use]
    pub fn lock_now(&self, stem: &str) -> Option<Locked> {
        self.try_lock(&self.dir.join(format!("{stem}.json.lock")), false)
    }

    /// One attempt at the lock at `path`, waiting for it with `wait`: `None` when it cannot be
    /// taken, or when the file locked is no longer the one the path names.
    fn try_lock(&self, path: &Path, wait: bool) -> Option<Locked> {
        let file = open_private(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false),
            path,
            true,
        )
        .ok()?;
        let held = file.metadata().ok()?;
        (owner(&held) == self.owner && held.is_file()).then_some(())?;
        if wait {
            file.lock().ok()?;
        } else {
            file.try_lock().ok()?;
        }
        let named = path.symlink_metadata().ok()?;
        same_file(&named, &held).then(|| Locked {
            _file: file,
            path: path.to_path_buf(),
        })
    }

    /// The regular file `name` of the folder, open to read and write, created with mode 0600
    /// when missing. A symlink, FIFO or other entry there that is neither a regular file nor a
    /// folder is replaced, never followed or read (H18). `None` when the file belongs to another
    /// user or cannot be opened.
    #[must_use]
    pub fn open_file(&self, name: &str) -> Option<File> {
        let path = self.file_path(name);
        for _ in 0..2 {
            let mut options = OpenOptions::new();
            options.read(true).write(true).create(true).truncate(false);
            let replace = match open_private(&mut options, &path, true) {
                Ok(file) => {
                    let meta = file.metadata().ok()?;
                    if meta.is_file() {
                        return (owner(&meta) == self.owner).then_some(file);
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
        }
        None
    }

    /// The record `stem`, `None` when it is missing, no regular file, over 16 KiB or no
    /// record of version 1. Never follows a symlink or blocks on a FIFO (H18).
    #[must_use]
    pub fn read(&self, stem: &str) -> Option<Record> {
        Record::from_json(&read_record_file(&self.record_path(stem))?)
    }

    /// Writes `record` as `stem` through a temp file opened `O_CREAT | O_EXCL | O_NOFOLLOW` and
    /// renamed over the record, which replaces a symlink or FIFO there. A record that cannot fit
    /// in [`MAX_RECORD_BYTES`](crate::activity::MAX_RECORD_BYTES) leaves the file as it was
    /// ([`Record::to_json`]).
    pub fn write(&self, stem: &str, record: &Record) -> io::Result<()> {
        let text = record.to_json()?;
        let temp = self
            .dir
            .join(format!(".{stem}.json.{}.tmp", std::process::id()));
        let _ = std::fs::remove_file(&temp);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = open_private(&mut options, &temp, false)?;
        let written = file
            .write_all(&text)
            .and_then(|()| std::fs::rename(&temp, self.record_path(stem)));
        if written.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        written
    }

    /// Deletes the record `stem` and its lock, which the caller holds.
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

/// Opens `path` with `options`, created with mode 0600, never through a symlink, and with
/// `nonblocking` never blocking on a FIFO.
#[cfg(unix)]
fn open_private(options: &mut OpenOptions, path: &Path, nonblocking: bool) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    let flags = if nonblocking {
        libc::O_NOFOLLOW | libc::O_NONBLOCK
    } else {
        libc::O_NOFOLLOW
    };
    options.mode(0o600).custom_flags(flags).open(path)
}

/// Opens `path` with `options`, refusing a symlink, which the open itself cannot do here.
#[cfg(not(unix))]
fn open_private(options: &mut OpenOptions, path: &Path, _nonblocking: bool) -> io::Result<File> {
    if path.symlink_metadata().is_ok_and(|meta| meta.is_symlink()) {
        return Err(io::Error::other("symlink"));
    }
    options.open(path)
}

/// The user that owns a file.
#[cfg(unix)]
fn owner(meta: &Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    meta.uid()
}

/// Without Unix owners every file counts as the user's own.
#[cfg(not(unix))]
fn owner(_: &Metadata) -> u32 {
    0
}

/// Whether `a` and `b` describe one file, by device and inode.
#[cfg(unix)]
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.ino() == b.ino() && a.dev() == b.dev()
}

/// The standard library tells no file identity here, so a lock file still in place counts as
/// the one locked.
#[cfg(not(unix))]
fn same_file(_: &Metadata, _: &Metadata) -> bool {
    true
}

/// `true` for the stem of a record (H17, H25).
fn is_stem(stem: &str) -> bool {
    Stem::parse(stem).is_some()
}

#[cfg(test)]
pub(crate) mod tests;
