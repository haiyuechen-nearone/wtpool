//! The activity folder and the record files in it.

use std::io::Read;
use std::path::Path;

use crate::platform::fs::open_without_following;

/// The folder in the common git directory that holds the activity records.
pub const ACTIVITY_DIR: &str = "wtpool-activity";
/// The `version` of every record this wtpool writes and reads.
pub const RECORD_VERSION: u32 = 1;
/// The most bytes a record file holds. A reader never reads a larger one, so a writer stays
/// within it.
pub const MAX_RECORD_BYTES: usize = 16 * 1024;

/// Creates `<common>/wtpool-activity/`, where hooks write, unless something is there already.
/// A hook never calls it, as it writes only into a folder that exists.
pub fn create_activity(common: &Path) -> std::io::Result<()> {
    let dir = common.join(ACTIVITY_DIR);
    if dir.symlink_metadata().is_ok() {
        return Ok(());
    }
    match std::fs::create_dir(&dir) {
        Err(error) if error.kind() != std::io::ErrorKind::AlreadyExists => Err(error),
        _ => Ok(()),
    }
}

/// The bytes of the record file at `path`. `None` for a symlink, a FIFO or anything else that is
/// not a regular file, and for a file over [`MAX_RECORD_BYTES`].
#[must_use]
pub fn read_record_file(path: &Path) -> Option<Vec<u8>> {
    let file = open_without_following(path).ok()?;
    let meta = file.metadata().ok()?;
    let limit = u64::try_from(MAX_RECORD_BYTES).unwrap_or(u64::MAX);
    if !meta.is_file() || meta.len() > limit {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= MAX_RECORD_BYTES).then_some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_activity__should_create_the_folder_once() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");

        // When
        let first = create_activity(dir.path());
        let again = create_activity(dir.path());
        let failed = create_activity(&missing);

        // Then
        assert!(first.is_ok() && again.is_ok());
        assert!(dir.path().join("wtpool-activity").is_dir());
        assert_eq!(failed.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }
}
