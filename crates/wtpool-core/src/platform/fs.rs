//! Opens that never follow a symlink, what a file's metadata tells of its identity, owner and
//! age, and the prefix a path starts with.

use std::fs::{File, Metadata, OpenOptions};
use std::io;
use std::path::{Component, Path, Prefix};

/// Opens `path` with `options`, failing when its last component is a symlink, and never blocking
/// on a FIFO. A file it creates only its owner may read and write.
pub fn open_private(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    let file = without_following(options).open(path)?;
    refuse_symlink(&file.metadata()?)?;
    Ok(file)
}

/// [`open_private`] to read.
pub fn open_without_following(path: &Path) -> io::Result<File> {
    open_private(OpenOptions::new().read(true), path)
}

#[cfg(unix)]
fn without_following(options: &mut OpenOptions) -> &mut OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    options
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
}

/// The open gives the link itself rather than its target, as `symlink_metadata` does, so
/// [`refuse_symlink`] then reads the handle and no path check can race the open. Windows keeps
/// FIFOs out of the file system, and a file's access list, not a mode, keeps others out.
#[cfg(windows)]
fn without_following(options: &mut OpenOptions) -> &mut OpenOptions {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
}

fn refuse_symlink(meta: &Metadata) -> io::Result<()> {
    if meta.is_symlink() {
        return Err(io::Error::other("symlink"));
    }
    Ok(())
}

/// A file's identity: its device and its number there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileId {
    device: u64,
    inode: u64,
}

/// The identity of the file `meta` describes, `None` where the system tells none.
#[cfg(unix)]
#[must_use]
pub fn file_id(meta: &Metadata) -> Option<FileId> {
    use std::os::unix::fs::MetadataExt;
    Some(FileId {
        device: meta.dev(),
        inode: meta.ino(),
    })
}

/// The standard library tells a Windows file's identity only through an unstable API.
#[cfg(not(unix))]
#[must_use]
pub fn file_id(_: &Metadata) -> Option<FileId> {
    None
}

/// `Some(true)` when `a` and `b` describe one file, `None` where the system tells no file identity.
#[must_use]
pub fn same_file(a: &Metadata, b: &Metadata) -> Option<bool> {
    same_id(file_id(a), file_id(b))
}

fn same_id(a: Option<FileId>, b: Option<FileId>) -> Option<bool> {
    Some(a? == b?)
}

/// The id of the user who owns the file `meta` describes, `None` where the system tells none.
#[cfg(unix)]
#[must_use]
pub fn owner(meta: &Metadata) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    Some(meta.uid())
}

/// The standard library tells no owner of a Windows file.
#[cfg(not(unix))]
#[must_use]
pub fn owner(_: &Metadata) -> Option<u32> {
    None
}

/// When the file `meta` describes last changed, in seconds and nanoseconds since the Unix epoch.
#[cfg(unix)]
#[must_use]
pub fn modified(meta: &Metadata) -> (i64, i64) {
    use std::os::unix::fs::MetadataExt;
    (meta.mtime(), meta.mtime_nsec())
}

#[cfg(not(unix))]
#[must_use]
pub fn modified(meta: &Metadata) -> (i64, i64) {
    let since = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .unwrap_or_default();
    (
        i64::try_from(since.as_secs()).unwrap_or(i64::MAX),
        i64::from(since.subsec_nanos()),
    )
}

/// The prefix of `path`, as Windows reads it.
#[cfg(windows)]
#[must_use]
pub fn path_prefix(path: &Path) -> Option<Prefix<'_>> {
    match path.components().next()? {
        Component::Prefix(prefix) => Some(prefix.kind()),
        _ => None,
    }
}

/// No path has a prefix here, but one that starts with exactly two slashes, as `//server/share`
/// does, reads as a UNC share: POSIX leaves such a path to the system, and Windows and Cygwin
/// read it so.
#[cfg(not(windows))]
#[must_use]
pub fn path_prefix(path: &Path) -> Option<Prefix<'_>> {
    let text = path.as_os_str().as_encoded_bytes();
    if !text.starts_with(b"//") || text.starts_with(b"///") {
        return None;
    }
    let mut names = path.components().filter_map(|component| match component {
        Component::Normal(name) => Some(name),
        _ => None,
    });
    Some(Prefix::UNC(
        names.next().unwrap_or_default(),
        names.next().unwrap_or_default(),
    ))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const ONE: FileId = FileId {
        device: 1,
        inode: 7,
    };
    const OTHER: FileId = FileId {
        device: 2,
        inode: 7,
    };

    #[rstest]
    #[case::one_file(Some(ONE), Some(ONE), Some(true))]
    #[case::two_files(Some(ONE), Some(OTHER), Some(false))]
    #[case::first_unknown(None, Some(ONE), None)]
    #[case::second_unknown(Some(ONE), None, None)]
    #[case::both_unknown(None, None, None)]
    fn same_id__should_tell_only_when_both_ids_are_known(
        #[case] a: Option<FileId>,
        #[case] b: Option<FileId>,
        #[case] expected: Option<bool>,
    ) {
        // When
        let same = same_id(a, b);

        // Then
        assert_eq!(same, expected);
    }

    #[cfg(unix)]
    #[test]
    fn open_without_following__should_refuse_a_symlink() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, "text").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        // When
        let through_link = open_without_following(&link);
        let direct = open_without_following(&target);

        // Then
        assert!(through_link.is_err());
        assert!(direct.is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn refuse_symlink__should_refuse_only_a_symlink() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, "text").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        // When
        let refused = refuse_symlink(&link.symlink_metadata().unwrap());
        let kept = refuse_symlink(&target.symlink_metadata().unwrap());

        // Then
        assert_eq!(refused.unwrap_err().to_string(), "symlink");
        assert!(kept.is_ok());
    }
}
