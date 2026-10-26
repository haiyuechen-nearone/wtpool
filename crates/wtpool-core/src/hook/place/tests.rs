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
