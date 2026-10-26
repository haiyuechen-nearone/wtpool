//! [`JobFolder`]: `<state>/wtpool/jobs/`, the records and locks of this user's jobs.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use super::record::{JobId, JobRecord, RECORD_VERSION};
use crate::platform::fs::{
    check_private, create_private_dir, open_private, open_without_following,
};

/// A record holds a summary, a label and a folder; anything larger is no record.
const MAX_RECORD_BYTES: u64 = 64 * 1024;
const QUEUE_LOCK: &str = "queue.lock";
const TICKET: &str = "ticket";

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
pub struct QueueLock {
    _file: File,
}

/// Whether a job still runs, as its lock tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    Live,
    Dead,
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
        self.dir.join(format!("{id}.json"))
    }

    fn lock_path(&self, id: &JobId) -> PathBuf {
        self.dir.join(format!("{id}.lock"))
    }

    /// Takes the queue's lock, waiting while another job holds it.
    pub fn lock_queue(&self) -> Result<QueueLock, FolderError> {
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
    pub fn take_ticket(
        &self,
        _queue: &QueueLock,
        records: &[JobRecord],
    ) -> Result<u64, FolderError> {
        let counted = read_small(&self.dir.join(TICKET))
            .and_then(|text| text.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let highest = records
            .iter()
            .map(|record| record.ticket)
            .max()
            .unwrap_or(0);
        let ticket = counted.max(highest).saturating_add(1);
        self.write_whole(TICKET, ticket.to_string().as_bytes())
            .map_err(|error| self.io_error(error))?;
        Ok(ticket)
    }

    /// Creates and locks the lock file of `id`, which its job holds for its whole life. The
    /// standard library opens it close-on-exec, so the job's command never holds the lock.
    pub fn create_lock(&self, id: &JobId) -> Result<File, FolderError> {
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

    /// Whether the job `id` holds its lock. A lock that cannot be probed counts as held.
    #[must_use]
    pub fn liveness(&self, id: &JobId) -> Liveness {
        let Ok(file) = open_without_following(&self.lock_path(id)) else {
            return Liveness::Dead;
        };
        match file.try_lock_shared() {
            Ok(()) => Liveness::Dead,
            Err(_) => Liveness::Live,
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
            .filter_map(|entry| {
                let name = entry.file_name();
                let id = JobId::parse(name.to_str()?.strip_suffix(".json")?)?;
                let record: JobRecord =
                    serde_json::from_str(&read_small(&self.record_path(&id))?).ok()?;
                (record.version == RECORD_VERSION && record.id == id).then_some(record)
            })
            .collect()
    }

    /// The record of `id` while its job lives.
    #[must_use]
    pub fn live_record(&self, id: &JobId) -> Option<JobRecord> {
        let record: JobRecord = serde_json::from_str(&read_small(&self.record_path(id))?).ok()?;
        let named = record.version == RECORD_VERSION && record.id == *id;
        (named && self.liveness(id) == Liveness::Live).then_some(record)
    }

    /// The records whose job lives.
    #[must_use]
    pub fn live_records(&self) -> Vec<JobRecord> {
        self.records()
            .into_iter()
            .filter(|record| self.liveness(&record.id) == Liveness::Live)
            .collect()
    }

    /// The live records but `own`'s, under `_queue`, after deleting each dead record with its
    /// lock file, and each lock file no record names that no job holds.
    #[must_use]
    pub fn sweep(&self, _queue: &QueueLock, own: Option<&JobId>) -> Vec<JobRecord> {
        let (live, dead): (Vec<JobRecord>, Vec<JobRecord>) = self
            .records()
            .into_iter()
            .filter(|record| Some(&record.id) != own)
            .partition(|record| self.liveness(&record.id) == Liveness::Live);
        for record in &dead {
            self.delete(&record.id);
        }
        for id in self.orphan_locks(own) {
            if self.liveness(&id) == Liveness::Dead {
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
            .filter_map(|entry| JobId::parse(entry.file_name().to_str()?.strip_suffix(".lock")?))
            .filter(|id| Some(id) != own && self.record_path(id).symlink_metadata().is_err())
            .collect()
    }

    /// Writes `record` through a temp file renamed over its record.
    pub fn write_record(&self, record: &JobRecord) -> Result<(), FolderError> {
        let text = serde_json::to_vec(record).map_err(|error| self.io_error(error.into()))?;
        self.write_whole(&format!("{}.json", record.id), &text)
            .map_err(|error| self.io_error(error))
    }

    /// Deletes the record of `id`, then its lock file.
    pub fn delete(&self, id: &JobId) {
        let _ = std::fs::remove_file(self.record_path(id));
        let _ = std::fs::remove_file(self.lock_path(id));
    }

    /// Writes `bytes` as the file `name` through a temp file opened without following a
    /// symlink, created anew, and renamed over the file.
    fn write_whole(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let temp = self.dir.join(format!(".{name}.{}.tmp", std::process::id()));
        let _ = std::fs::remove_file(&temp);
        let mut file = open_private(OpenOptions::new().write(true).create_new(true), &temp)?;
        let written = file
            .write_all(bytes)
            .and_then(|()| std::fs::rename(&temp, self.dir.join(name)));
        if written.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        written
    }
}

/// The text of the small regular file at `path`, never through a symlink.
fn read_small(path: &Path) -> Option<String> {
    let file = open_without_following(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_RECORD_BYTES).read_to_string(&mut text).ok()?;
    Some(text)
}

#[cfg(test)]
mod tests;
