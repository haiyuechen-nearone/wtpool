//! The main worktree of a repository, read from the files git keeps, as `git worktree list`
//! names it first.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::git::layout::{CONFIG, DOT_GIT};
use crate::platform::fs::read_regular;

/// The longest git config read for `core.bare`.
const CONFIG_LIMIT: u64 = 1 << 20;

/// The path git lists first for the repository whose common git directory is `common`: the folder
/// that holds it when it is a `.git` folder, else `common` itself, as for a repository made with
/// `git init --separate-git-dir`. `None` for a bare repository: one whose config sets `core.bare`,
/// else, without that key, one whose common git directory is no `.git` folder.
pub(super) fn main_worktree(common: &Path) -> Option<PathBuf> {
    let in_worktree = common.file_name() == Some(OsStr::new(DOT_GIT));
    let bare = read_regular(&common.join(CONFIG), CONFIG_LIMIT)
        .ok()
        .and_then(|bytes| core_bare(&String::from_utf8_lossy(&bytes)))
        .unwrap_or(!in_worktree);
    if bare {
        return None;
    }
    if in_worktree {
        common.parent().map(Path::to_path_buf)
    } else {
        Some(common.to_path_buf())
    }
}

/// The last `bare` of the `[core]` sections of the git config `text`. `None` when none sets it,
/// or when its value is no git boolean.
fn core_bare(text: &str) -> Option<bool> {
    let mut in_core = false;
    let mut bare = None;
    for line in text.lines().map(str::trim) {
        if let Some(header) = line.strip_prefix('[') {
            let name = header.split(']').next().unwrap_or_default().trim();
            in_core = name.eq_ignore_ascii_case("core");
            continue;
        }
        let (key, value) = match line.split_once('=') {
            Some((key, value)) => (key.trim(), Some(value)),
            None => (line, None),
        };
        if in_core && key.eq_ignore_ascii_case("bare") {
            bare = git_bool(value);
        }
    }
    bare
}

/// A git config boolean: a key without `=` is true, an empty value false.
fn git_bool(value: Option<&str>) -> Option<bool> {
    let Some(value) = value else {
        return Some(true);
    };
    let value = value.split(['#', ';']).next().unwrap_or_default().trim();
    match value.trim_matches('"').to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" | "" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::bare("[core]\n\tbare = true\n", Some(true))]
    #[case::not_bare("[core]\n\trepositoryformatversion = 0\n\tbare = false\n", Some(false))]
    #[case::upper_case("[CORE]\n\tBare = Yes\n", Some(true))]
    #[case::no_value("[core]\n\tbare\n", Some(true))]
    #[case::comment_after("[core]\n\tbare = false # was true\n", Some(false))]
    #[case::last_wins("[core]\n\tbare = true\n[core]\n\tbare = false\n", Some(false))]
    #[case::other_section("[remote \"origin\"]\n\tbare = true\n", None)]
    #[case::subsection("[core \"x\"]\n\tbare = true\n", None)]
    #[case::no_key("[core]\n\tfilemode = true\n", None)]
    #[case::no_boolean("[core]\n\tbare = maybe\n", None)]
    fn core_bare__should_read_the_last_bare_of_core(
        #[case] text: &str,
        #[case] expected: Option<bool>,
    ) {
        // When
        let bare = core_bare(text);

        // Then
        assert_eq!(bare, expected);
    }

    fn must_git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn main_worktree__should_name_what_git_lists_first() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        must_git(&root, &["init", "-q", "app"]);
        must_git(&root, &["init", "-q", "--bare", "bare.git"]);
        must_git(&root, &["init", "-q", "--separate-git-dir=sep.git", "wt"]);

        // When
        let found =
            ["app/.git", "bare.git", "sep.git"].map(|common| main_worktree(&root.join(common)));

        // Then
        assert_eq!(
            found,
            [Some(root.join("app")), None, Some(root.join("sep.git"))]
        );
    }

    #[rstest]
    #[case::a_dot_git_folder(".git", true)]
    #[case::another_folder("app.git", false)]
    fn main_worktree__should_go_by_the_name_without_a_config(
        #[case] name: &str,
        #[case] has_worktree: bool,
    ) {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let common = dir.path().join(name);
        std::fs::create_dir(&common).unwrap();

        // When
        let found = main_worktree(&common);

        // Then
        assert_eq!(found.is_some(), has_worktree);
    }
}
