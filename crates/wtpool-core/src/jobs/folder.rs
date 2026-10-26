//! [`JobFolder`]: `<state>/wtpool/jobs/`, the records and locks of this user's jobs.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

use super::record::{JobId, JobRecord, RECORD_VERSION};
use crate::platform::fs::{
    check_private, create_private_dir, open_private, open_without_following, read_regular,
    replace_file,
};

/// A record holds a summary, a label and a folder, so a larger file is no record.
const MAX_RECORD_BYTES: u64 = 64 * 1024;
const QUEUE_LOCK: &str = "queue.lock";
const TICKET: &str = "ticket";
const RECORD_SUFFIX: &str = ".json";
const LOCK_SUFFIX: &str = ".lock";

/// The private folder of this user's job records and locks.
#[derive(Debug, Clone)]
pub struct JobFolder {
    dir: PathBuf,
}

/// A job folder that cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum FolderError {
    #[error("refusing the job folder {}: {reason}", path.display())]
    Unsafe { path: PathBuf, reason: String },
    #[error("cannot use the job folder {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
}

/// The exclusive lock on `queue.lock`, held until dropped.
#[derive(Debug)]
pub(super) struct QueueLock {
    _file: File,
}

/// The lock of a job: held while the job lives, free once it is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockState {
    Live,
    Dead,
}

/// A file of the job folder, as its name tells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobFile {
    Record(JobId),
    Lock(JobId),
    /// The queue's lock or its ticket counter.
    Queue,
    /// A file a write renames over another once it is whole.
    Temp,
    Other,
}

impl JobFolder {
    /// `wtpool/jobs` in the state folder `state`.
    #[must_use]
    pub fn path_in(state: &Path) -> PathBuf {
        state.join("wtpool/jobs")
    }

    /// The folder at `dir`, created private when missing. A symlink, anything but a folder, or
    /// a folder of another user or that others may write to is refused.
    pub fn create(dir: PathBuf) -> Result<Self, FolderError> {
        if let Err(source) = create_private_dir(&dir) {
            return Err(FolderError::Io { path: dir, source });
        }
        Self::checked(dir)
    }

    /// The folder at `dir` when there is one, refused as [`JobFolder::create`] refuses it.
    pub fn existing(dir: PathBuf) -> Result<Option<Self>, FolderError> {
        if dir.symlink_metadata().is_err() {
            return Ok(None);
        }
        Self::checked(dir).map(Some)
    }

    fn checked(dir: PathBuf) -> Result<Self, FolderError> {
        let unsafe_because = |reason: String| FolderError::Unsafe {
            path: dir.clone(),
            reason,
        };
        let meta = dir
            .symlink_metadata()
            .map_err(|error| unsafe_because(error.to_string()))?;
        if meta.is_symlink() {
            return Err(unsafe_because("it is a symlink".to_owned()));
        }
        if !meta.is_dir() {
            return Err(unsafe_because("it is not a folder".to_owned()));
        }
        check_private(&meta).map_err(|fault| unsafe_because(fault.to_string()))?;
        Ok(Self { dir })
    }

    /// The kind of the file called `name` in a job folder.
    #[must_use]
    pub fn file_kind(name: &OsStr) -> JobFile {
        let Some(name) = name.to_str() else {
            return JobFile::Other;
        };
        let id_before = |suffix: &str| name.strip_suffix(suffix).and_then(JobId::parse);
        if name == QUEUE_LOCK || name == TICKET {
            JobFile::Queue
        } else if name.starts_with('.') && name.ends_with(".tmp") {
            JobFile::Temp
        } else if let Some(id) = id_before(RECORD_SUFFIX) {
            JobFile::Record(id)
        } else if let Some(id) = id_before(LOCK_SUFFIX) {
            JobFile::Lock(id)
        } else {
            JobFile::Other
        }
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn io_error(&self, source: io::Error) -> FolderError {
        FolderError::Io {
            path: self.dir.clone(),
            source,
        }
    }

    fn record_path(&self, id: &JobId) -> PathBuf {
        self.dir.join(format!("{id}{RECORD_SUFFIX}"))
    }

    fn lock_path(&self, id: &JobId) -> PathBuf {
        self.dir.join(format!("{id}{LOCK_SUFFIX}"))
    }

    /// Takes the queue's lock, waiting while another job holds it.
    pub(super) fn lock_queue(&self) -> Result<QueueLock, FolderError> {
        let file = open_private(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false),
            &self.dir.join(QUEUE_LOCK),
        )
        .map_err(|error| self.io_error(error))?;
        file.lock().map_err(|error| self.io_error(error))?;
        Ok(QueueLock { _file: file })
    }

    /// The next ticket, under `_queue`: one past the counter's and every ticket of `records`.
    pub(super) fn take_ticket(
        &self,
        _queue: &QueueLock,
        records: &[JobRecord],
    ) -> Result<u64, FolderError> {
        let counted = read_text(&self.dir.join(TICKET))
            .and_then(|text| text.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let highest = records
            .iter()
            .map(|record| record.ticket)
            .max()
            .unwrap_or(0);
        let ticket = counted.max(highest).saturating_add(1);
        replace_file(&self.dir.join(TICKET), ticket.to_string().as_bytes())
            .map_err(|error| self.io_error(error))?;
        Ok(ticket)
    }

    /// Creates and locks the lock file of `id`. Its job holds it for its whole life. The
    /// standard library opens it close-on-exec, so the job's command never holds the lock.
    pub(crate) fn create_lock(&self, id: &JobId) -> Result<File, FolderError> {
        let file = open_private(
            OpenOptions::new().read(true).write(true).create_new(true),
            &self.lock_path(id),
        )
        .map_err(|error| self.io_error(error))?;
        match file.try_lock() {
            Ok(()) => Ok(file),
            Err(TryLockError::WouldBlock) => Err(self.io_error(io::Error::other(
                "another process holds a lock file just made",
            ))),
            Err(TryLockError::Error(error)) => Err(self.io_error(error)),
        }
    }

    /// The state of the lock of the job `id`. A lock that cannot be probed counts as held.
    #[must_use]
    pub fn liveness(&self, id: &JobId) -> LockState {
        let Ok(file) = open_without_following(&self.lock_path(id)) else {
            return LockState::Dead;
        };
        match file.try_lock_shared() {
            Ok(()) => LockState::Dead,
            Err(_) => LockState::Live,
        }
    }

    /// Every record of the folder, those that cannot be read left out.
    #[must_use]
    pub fn records(&self) -> Vec<JobRecord> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| match Self::file_kind(&entry.file_name()) {
                JobFile::Record(id) => self.named_record(&id),
                _ => None,
            })
            .collect()
    }

    /// The record of `id` while its job lives.
    #[must_use]
    pub fn live_record(&self, id: &JobId) -> Option<JobRecord> {
        self.named_record(id)
            .filter(|_| self.liveness(id) == LockState::Live)
    }

    /// The record of `id`, when its file holds one of this version that names `id`.
    fn named_record(&self, id: &JobId) -> Option<JobRecord> {
        let record: JobRecord = serde_json::from_str(&read_text(&self.record_path(id))?).ok()?;
        (record.version == RECORD_VERSION && record.id == *id).then_some(record)
    }

    /// The records whose job lives.
    #[must_use]
    pub fn live_records(&self) -> Vec<JobRecord> {
        self.records()
            .into_iter()
            .filter(|record| self.liveness(&record.id) == LockState::Live)
            .collect()
    }

    /// The live records but `own`'s, under `_queue`, after deleting each dead record with its
    /// lock file, and each lock file no record names that no job holds.
    #[must_use]
    pub(super) fn sweep(&self, _queue: &QueueLock, own: Option<&JobId>) -> Vec<JobRecord> {
        let (live, dead): (Vec<JobRecord>, Vec<JobRecord>) = self
            .records()
            .into_iter()
            .filter(|record| Some(&record.id) != own)
            .partition(|record| self.liveness(&record.id) == LockState::Live);
        for record in &dead {
            self.delete(&record.id);
        }
        for id in self.orphan_locks(own) {
            if self.liveness(&id) == LockState::Dead {
                let _ = std::fs::remove_file(self.lock_path(&id));
            }
        }
        live
    }

    fn orphan_locks(&self, own: Option<&JobId>) -> Vec<JobId> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| match Self::file_kind(&entry.file_name()) {
                JobFile::Lock(id) => Some(id),
                _ => None,
            })
            .filter(|id| Some(id) != own && self.record_path(id).symlink_metadata().is_err())
            .collect()
    }

    /// Writes `record` through a temp file renamed over its record.
    pub(super) fn write_record(&self, record: &JobRecord) -> Result<(), FolderError> {
        let text = serde_json::to_vec(record).map_err(|error| self.io_error(error.into()))?;
        replace_file(&self.record_path(&record.id), &text).map_err(|error| self.io_error(error))
    }

    /// Deletes the record of `id`, then its lock file.
    pub(super) fn delete(&self, id: &JobId) {
        let _ = std::fs::remove_file(self.record_path(id));
        let _ = std::fs::remove_file(self.lock_path(id));
    }
}

/// The text of the small regular file at `path`, never through a symlink.
fn read_text(path: &Path) -> Option<String> {
    String::from_utf8(read_regular(path, MAX_RECORD_BYTES).ok()?).ok()
}

#[cfg(test)]
mod tests;
