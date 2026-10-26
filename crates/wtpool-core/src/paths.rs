use std::io;
use std::ops::Deref;
use std::path::{Component, Path, PathBuf};

/// A path that means the same directory whatever the current directory is, so
/// git run with `-C` elsewhere sees the same place as this process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsolutePath(PathBuf);

impl AbsolutePath {
    /// Resolves a relative `path` against the current directory, without
    /// following symlinks or `..`. Fails when `path` is empty or the current
    /// directory cannot be read.
    pub fn try_new(path: impl AsRef<Path>) -> io::Result<Self> {
        std::path::absolute(path).map(Self)
    }

    /// The directory that holds this one. The root holds itself.
    #[must_use]
    pub(crate) fn parent_dir(&self) -> Self {
        Self(self.0.parent().unwrap_or(&self.0).to_path_buf())
    }

    #[must_use]
    pub fn join(&self, part: impl AsRef<Path>) -> Self {
        Self(self.0.join(part))
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    #[must_use]
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

impl Deref for AbsolutePath {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for AbsolutePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl From<AbsolutePath> for PathBuf {
    fn from(path: AbsolutePath) -> Self {
        path.0
    }
}

/// Compares resolved paths, so a trailing slash, a relative path or a symlink
/// cannot slip past a foreground check.
pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    match (resolved(a), resolved(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a == b,
    }
}

/// A path whose last component is missing resolves through its parent.
fn resolved(path: &Path) -> Option<PathBuf> {
    path.canonicalize().ok().or_else(|| {
        let parent = path.parent()?.canonicalize().ok()?;
        Some(parent.join(path.file_name()?))
    })
}

/// `path` with symlinks resolved when it exists, so it compares with the folders that the
/// process table and folder events name.
#[must_use]
pub(crate) fn comparable(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// `true` when `path` is absolute with only normal components after its root: no `.` or `..`.
#[must_use]
pub fn is_plain_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::relative("slots", "slots")]
    #[case::relative_through_dot_dot_kept_as_written("repo/../app", "repo/../app")]
    fn absolute_path__should_resolve_a_relative_path_against_the_current_directory(
        #[case] path: &str,
        #[case] under_cwd: &str,
    ) {
        // Given
        let cwd = std::env::current_dir().unwrap();

        // When
        let absolute = AbsolutePath::try_new(path).unwrap();

        // Then
        assert_eq!(absolute.as_path(), cwd.join(under_cwd));
    }

    #[test]
    fn absolute_path__should_keep_an_absolute_path() {
        // When
        let path = AbsolutePath::try_new("/srv/app").unwrap();

        // Then
        assert_eq!(PathBuf::from(path), Path::new("/srv/app"));
    }

    #[test]
    fn absolute_path__should_refuse_an_empty_path() {
        // When
        let path = AbsolutePath::try_new("");

        // Then
        assert!(path.is_err());
    }

    #[cfg(unix)]
    #[rstest]
    #[case::trailing_slash("dir/", "dir", true)]
    #[case::missing_directory_through_its_parent("sub/../gone", "gone", true)]
    #[case::symlink("link", "dir", true)]
    #[case::other_directory("dir", "sub", false)]
    #[case::missing_parent_as_written("missing/a", "missing/a", true)]
    #[case::missing_parent_spelled_otherwise("missing/a", "missing/../missing/a", false)]
    #[case::dot_dot_after_a_file_as_written("file/..", "file/..", true)]
    #[case::empty_paths("", "", true)]
    fn same_path__should_compare_resolved_paths(
        #[case] a: &str,
        #[case] b: &str,
        #[case] expected: bool,
    ) {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("dir")).unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("file"), "").unwrap();
        std::os::unix::fs::symlink(root.join("dir"), root.join("link")).unwrap();
        let under_root = |path: &str| {
            if path.is_empty() {
                PathBuf::new()
            } else {
                root.join(path)
            }
        };

        // When
        let same = same_path(&under_root(a), &under_root(b));

        // Then
        assert_eq!(same, expected);
    }

    #[cfg(unix)]
    #[test]
    fn comparable__should_resolve_symlinks_and_keep_missing_paths() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, dir.path().join("link")).unwrap();

        // When
        let linked = comparable(&dir.path().join("link"));
        let missing = comparable(Path::new("/no/such/folder"));

        // Then
        assert_eq!(linked, real.canonicalize().unwrap());
        assert_eq!(missing, PathBuf::from("/no/such/folder"));
    }
}
