use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::cmd::git;

/// The repository that holds this xtask, so a clone always verifies itself.
pub fn root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap_or(manifest).to_path_buf()
}

pub fn head(root: &Path) -> Result<String, String> {
    Ok(git(root, &["rev-parse", "HEAD"])?.trim().to_owned())
}

/// A hash of every uncommitted change, or `None` for a clean tree. Ignored files do not count.
pub fn dirty_fingerprint(root: &Path) -> Result<Option<String>, String> {
    let diff = git(root, &["diff", "HEAD", "--binary"])?;
    let untracked = git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    if diff.is_empty() && untracked.is_empty() {
        return Ok(None);
    }
    let mut hasher = Sha256::new();
    hasher.update(diff.as_bytes());
    for path in untracked.split('\0').filter(|path| !path.is_empty()) {
        hasher.update(path.as_bytes());
        hasher.update(entry_hash(&root.join(path)).as_bytes());
    }
    Ok(Some(hex(&hasher.finalize())))
}

pub fn run_id(root: &Path) -> Result<String, String> {
    let head = head(root)?;
    let short = head.get(..12).unwrap_or(&head).to_owned();
    Ok(match dirty_fingerprint(root)? {
        None => short,
        Some(fingerprint) => format!("{short}-dirty-{}", fingerprint.get(..8).unwrap_or_default()),
    })
}

pub fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// The hash of a file's bytes, of a symlink's target, or a marker for anything unreadable.
pub fn entry_hash(path: &Path) -> String {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => match fs::read_link(path) {
            Ok(target) => format!("symlink:{}", sha256(target.as_os_str().as_encoded_bytes())),
            Err(error) => format!("unreadable:{error}"),
        },
        Ok(_) => match fs::read(path) {
            Ok(bytes) => sha256(&bytes),
            Err(error) => format!("unreadable:{error}"),
        },
        Err(error) => format!("missing:{error}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Every file and symlink below `dir`, as paths relative to `root` with `/` separators, skipping
/// the directories `skip_dir` returns true for.
pub fn walk(root: &Path, dir: &Path, skip_dir: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(rel) = relative(root, &path) else {
                continue;
            };
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if !skip_dir(&rel) {
                    stack.push(path);
                }
            } else {
                found.push(rel);
            }
        }
    }
    found.sort();
    found
}

pub fn relative(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    Some(parts.join("/"))
}

/// The commit `spec-base` names, if the ref exists.
pub fn spec_base(root: &Path) -> Option<String> {
    commit_of(root, "spec-base")
}

fn commit_of(root: &Path, reference: &str) -> Option<String> {
    git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{reference}^{{commit}}"),
        ],
    )
    .ok()
    .map(|sha| sha.trim().to_owned())
    .filter(|sha| !sha.is_empty())
}

/// The variable that names the base commit instead of `spec-base`.
pub const BASE_VARIABLE: &str = "WTPOOL_VERIFY_BASE";

/// The commit `cover` and `repo_context__should_be_unchanged` measure from, and its source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    pub commit: String,
    pub source: String,
}

/// The base commit: the ref [`BASE_VARIABLE`] names, else the tag `spec-base`, else, in a clone
/// without that tag, the merge base of `HEAD` with the remote's default branch, else the root
/// commit.
pub fn base(root: &Path) -> Result<Base, String> {
    let named = std::env::var(BASE_VARIABLE).ok();
    find_base(
        root,
        named.as_deref().filter(|named| !named.trim().is_empty()),
    )
}

fn find_base(root: &Path, named: Option<&str>) -> Result<Base, String> {
    let base = |commit: &str, source: String| Base {
        commit: commit.trim().to_owned(),
        source,
    };
    if let Some(named) = named {
        return commit_of(root, named)
            .map(|commit| base(&commit, format!("{BASE_VARIABLE}={named}")))
            .ok_or_else(|| format!("{BASE_VARIABLE}={named} names no commit"));
    }
    if let Some(commit) = spec_base(root) {
        return Ok(base(&commit, "spec-base".to_owned()));
    }
    if let Ok(commit) = git(root, &["merge-base", "HEAD", "refs/remotes/origin/HEAD"])
        && !commit.trim().is_empty()
    {
        return Ok(base(
            &commit,
            "no spec-base: the merge base of HEAD with origin's default branch".to_owned(),
        ));
    }
    let roots = git(root, &["rev-list", "--max-parents=0", "HEAD"])?;
    roots
        .lines()
        .last()
        .map(|commit| base(commit, "no spec-base: the root commit".to_owned()))
        .ok_or_else(|| "HEAD has no root commit".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn must_git(dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// A repository with two commits, `upstream` a clone of it with one more commit on `main`.
    fn must_repos(name: &str) -> (PathBuf, PathBuf, String, String) {
        let dir = std::env::temp_dir().join(format!("xtask-base-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let first = dir.join("first");
        fs::create_dir_all(&first).unwrap();
        must_git(&first, &["init", "--quiet", "-b", "main"]);
        must_git(
            &first,
            &["commit", "--quiet", "--allow-empty", "-m", "root"],
        );
        let root = must_git(&first, &["rev-parse", "HEAD"]);
        must_git(&first, &["commit", "--quiet", "--allow-empty", "-m", "two"]);
        let two = must_git(&first, &["rev-parse", "HEAD"]);
        let clone = dir.join("clone");
        must_git(&dir, &["clone", "--quiet", "first", "clone"]);
        must_git(
            &clone,
            &["commit", "--quiet", "--allow-empty", "-m", "mine"],
        );
        (first, clone, root, two)
    }

    #[test]
    fn find_base__should_prefer_the_named_ref_then_spec_base() {
        // Given
        let (first, _clone, root, two) = must_repos("named");
        must_git(&first, &["tag", "spec-base", &root]);

        // When
        let named = find_base(&first, Some("HEAD"));
        let tagged = find_base(&first, None);
        let missing = find_base(&first, Some("no-such-ref"));

        // Then
        let _ = fs::remove_dir_all(first.parent().unwrap());
        assert_eq!(named.unwrap().commit, two);
        assert_eq!(
            tagged.unwrap(),
            Base {
                commit: root,
                source: "spec-base".to_owned()
            }
        );
        assert!(missing.unwrap_err().contains("names no commit"));
    }

    #[test]
    fn find_base__should_fall_back_to_the_merge_base_then_the_root_commit() {
        // Given
        let (first, clone, root, two) = must_repos("fallback");

        // When
        let in_clone = find_base(&clone, None).unwrap();
        let without_remote = find_base(&first, None).unwrap();

        // Then
        let _ = fs::remove_dir_all(first.parent().unwrap());
        assert_eq!(in_clone.commit, two);
        assert!(
            in_clone.source.contains("merge base"),
            "{}",
            in_clone.source
        );
        assert_eq!(without_remote.commit, root);
        assert!(
            without_remote.source.contains("root commit"),
            "{}",
            without_remote.source
        );
    }
}
