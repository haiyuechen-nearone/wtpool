use std::ffi::OsStr;

use super::*;
use rstest::rstest;

/// A pool without git: `fg` with a `.git` folder, linked worktrees `wt1` and `wt1x` whose
/// `.git` files point at `fg/.git/worktrees/<name>`, and `fake`, whose `.git` file points at
/// wt1's folder without a back link.
pub(crate) fn must_pool() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let common = root.join("fg/.git");
    std::fs::create_dir_all(common.join("objects")).unwrap();
    std::fs::write(common.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    for name in ["wt1", "wt1x"] {
        let gitdir = common.join("worktrees").join(name);
        std::fs::create_dir_all(&gitdir).unwrap();
        std::fs::create_dir_all(root.join(name)).unwrap();
        std::fs::write(
            root.join(name).join(".git"),
            format!("gitdir: {}\n", gitdir.display()),
        )
        .unwrap();
        std::fs::write(
            gitdir.join("gitdir"),
            format!("{}\n", root.join(name).join(".git").display()),
        )
        .unwrap();
        std::fs::write(gitdir.join("commondir"), "../..\n").unwrap();
    }
    std::fs::create_dir_all(root.join("fake")).unwrap();
    std::fs::write(
        root.join("fake/.git"),
        format!("gitdir: {}\n", common.join("worktrees/wt1").display()),
    )
    .unwrap();
    (dir, root)
}

#[rstest]
#[case::main_worktree("fg/a.txt", Some("fg"))]
#[case::linked_worktree("wt1/f.txt", Some("wt1"))]
#[case::new_file_in_new_folders("wt1/new/deep/f.txt", Some("wt1"))]
#[case::slot_prefix("wt1x/f.txt", Some("wt1x"))]
#[case::dot_dot("wt1/../wt1x/f.txt", Some("wt1x"))]
#[case::double_slashes("/wt1///f.txt", Some("wt1"))]
#[case::forged_git_file("fake/a.txt", None)]
#[case::outside_every_worktree("a.txt", None)]
fn worktree_of__should_find_the_worktree_by_component(
    #[case] path: &str,
    #[case] expected: Option<&str>,
) {
    // Given
    let (_dir, root) = must_pool();
    let full = PathBuf::from(format!("{}/{path}", root.display()));

    // When
    let found = worktree_of(&full);

    // Then
    assert_eq!(
        found.as_ref().map(|w| w.root.clone()),
        expected.map(|e| root.join(e))
    );
    if let Some(found) = found {
        assert_eq!(found.common, root.join("fg/.git"));
    }
}

#[cfg(unix)]
#[test]
fn worktree_of__should_refuse_a_symlinked_dot_git() {
    // Given
    let (_dir, root) = must_pool();
    std::fs::create_dir(root.join("linked")).unwrap();
    std::os::unix::fs::symlink(root.join("fg/.git"), root.join("linked/.git")).unwrap();

    // When
    let found = worktree_of(&root.join("linked/f.txt"));

    // Then
    assert_eq!(found, None);
}

#[cfg(unix)]
#[test]
fn worktree_of__should_resolve_symlinks() {
    // Given
    let (_dir, root) = must_pool();
    std::os::unix::fs::symlink(root.join("wt1x"), root.join("wt1/to-x")).unwrap();

    // When
    let found = worktree_of(&root.join("wt1/to-x/f.txt"));

    // Then
    assert_eq!(found.map(|w| w.root), Some(root.join("wt1x")));
}

/// A linked worktree `unc` of the pool at `root` whose file `unc_shaped`, its `.git` file, the
/// back link `gitdir` or `commondir`, names its path with a second slash before it.
#[cfg(not(windows))]
fn must_unc_shaped_worktree(root: &Path, unc_shaped: &str) -> PathBuf {
    let gitdir = root.join("fg/.git/worktrees/unc");
    let dir = root.join("unc");
    std::fs::create_dir_all(&gitdir).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let named = |file: &str, path: &Path| {
        let slash = if file == unc_shaped { "/" } else { "" };
        format!("{slash}{}\n", path.display())
    };
    let dot_git = format!("gitdir: {}", named(".git", &gitdir));
    std::fs::write(dir.join(".git"), dot_git).unwrap();
    std::fs::write(gitdir.join("gitdir"), named("gitdir", &dir.join(".git"))).unwrap();
    std::fs::write(
        gitdir.join("commondir"),
        named("commondir", &root.join("fg/.git")),
    )
    .unwrap();
    dir
}

#[cfg(not(windows))]
#[rstest]
#[case::none("", true)]
#[case::dot_git(".git", false)]
#[case::back_link("gitdir", false)]
#[case::commondir("commondir", false)]
fn worktree_of__should_not_read_a_unc_shaped_path_a_git_file_names(
    #[case] unc_shaped: &str,
    #[case] found: bool,
) {
    // Given
    let (_dir, root) = must_pool();
    let dir = must_unc_shaped_worktree(&root, unc_shaped);

    // When
    let worktree = worktree_of(&dir.join("f.txt"));

    // Then
    assert_eq!(worktree.map(|w| w.root), found.then_some(dir));
}

#[cfg(not(windows))]
#[test]
fn worktree_of__should_not_read_a_unc_shaped_path() {
    // Given
    let (_dir, root) = must_pool();
    let unc_shaped = PathBuf::from(format!("/{}", root.join("wt1/f.txt").display()));

    // When
    let found = worktree_of(&unc_shaped);

    // Then
    assert_eq!(found, None);
}

#[rstest]
#[case::none(None, true)]
#[case::drive(Some(Prefix::Disk(b'C')), true)]
#[case::verbatim_drive(Some(Prefix::VerbatimDisk(b'C')), true)]
#[case::unc(Some(Prefix::UNC(OsStr::new("server"), OsStr::new("share"))), false)]
#[case::verbatim_unc(
    Some(Prefix::VerbatimUNC(OsStr::new("server"), OsStr::new("share"))),
    false
)]
#[case::device(Some(Prefix::DeviceNS(OsStr::new("pipe"))), false)]
#[case::verbatim(Some(Prefix::Verbatim(OsStr::new("GLOBALROOT"))), false)]
fn is_local_prefix__should_keep_only_drives(
    #[case] prefix: Option<Prefix<'static>>,
    #[case] expected: bool,
) {
    // When
    let local = is_local_prefix(prefix);

    // Then
    assert_eq!(local, expected);
}

#[cfg(not(windows))]
#[rstest]
#[case::absolute("/a/b", true)]
#[case::relative("a/b", true)]
#[case::three_slashes("///a/b", true)]
#[case::double_slash_inside("/a//b", true)]
#[case::unc_shaped("//server/share/f.txt", false)]
#[case::server_only("//server", false)]
#[case::two_slashes_only("//", false)]
#[case::backslashes(r"\\server\share", true)]
fn is_local__should_refuse_a_path_that_starts_with_two_slashes(
    #[case] path: &str,
    #[case] expected: bool,
) {
    // When
    let local = is_local(Path::new(path));

    // Then
    assert_eq!(local, expected);
}

#[cfg(windows)]
#[rstest]
#[case::drive(r"C:\a", true)]
#[case::verbatim_drive(r"\\?\C:\a", true)]
#[case::relative(r"a\b", true)]
#[case::drive_relative("C:a", true)]
#[case::unc(r"\\server\share\a", false)]
#[case::unc_with_slashes("//server/share/a", false)]
#[case::verbatim_unc(r"\\?\UNC\server\share\a", false)]
#[case::device(r"\\.\pipe\a", false)]
#[case::verbatim(r"\\?\GLOBALROOT\a", false)]
#[case::nt_object(r"\??\UNC\server\share", false)]
#[case::server_only(r"\\server", false)]
#[case::root_without_drive(r"\a", false)]
fn is_local__should_refuse_shares_devices_and_verbatim_paths(
    #[case] path: &str,
    #[case] expected: bool,
) {
    // When
    let local = is_local(Path::new(path));

    // Then
    assert_eq!(local, expected);
}

#[rstest]
#[case::relative("/base", "a/b", "/base/a/b")]
#[case::absolute("/base", "/a", "/a")]
fn absolute__should_resolve_against_the_base(
    #[case] base: &str,
    #[case] path: &str,
    #[case] expected: &str,
) {
    // When
    let resolved = absolute(Path::new(base), path);

    // Then
    assert_eq!(resolved, PathBuf::from(expected));
}
