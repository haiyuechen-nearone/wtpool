use super::*;
use crate::hook::fixture::must_pool;
use rstest::rstest;

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
        found.map(|w| (w.root, w.common)),
        expected.map(|e| (root.join(e), root.join("fg/.git")))
    );
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
