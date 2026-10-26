use std::fs;
use std::path::{Path, PathBuf};

use super::VERIFY_EMAIL;
use crate::cmd::git;
use crate::repo::{relative, walk};

/// Remote URLs of the repositories in `run_dir` that lead outside it.
pub fn outside_remotes(run_dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let skip = |_: &str| false;
    for rel in walk(run_dir, run_dir, &skip) {
        let is_config = rel == "config" || rel.ends_with("/config");
        if !is_config {
            continue;
        }
        let config = run_dir.join(&rel);
        let Some(git_dir) = config.parent() else {
            continue;
        };
        if !git_dir.join("HEAD").exists() || !git_dir.join("objects").is_dir() {
            continue;
        }
        let Ok(listing) = git(
            git_dir,
            &[
                "config",
                "--file",
                "config",
                "--get-regexp",
                r"^remote\..*url$",
            ],
        ) else {
            continue;
        };
        for line in listing.lines() {
            let Some((_, url)) = line.split_once(' ') else {
                continue;
            };
            if !url_inside(url, git_dir, run_dir) {
                let shown = relative(run_dir, git_dir).unwrap_or_default();
                found.push(format!("{shown}: remote {url} is outside the run folder"));
            }
        }
    }
    found
}

pub fn url_inside(url: &str, git_dir: &Path, run_dir: &Path) -> bool {
    let path = url.strip_prefix("file://").unwrap_or(url);
    if path.contains("://") || (path.contains(':') && !path.starts_with('/')) {
        return false;
    }
    let candidate = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        git_dir.join(path)
    };
    resolve(&candidate).starts_with(resolve(run_dir))
}

/// Resolves symlinks in the longest existing prefix of `path`, so a remote that does not exist
/// yet still compares by its real location.
fn resolve(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut current = path;
    loop {
        if let Ok(real) = fs::canonicalize(current) {
            return rest.iter().rev().fold(real, |acc, part| acc.join(part));
        }
        match (current.parent(), current.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                current = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Branches, tags and backups in the real pool that point at a commit the harness made.
pub fn verify_refs(pool: &Path) -> Vec<String> {
    let format =
        "--format=%(refname) %(authoremail) %(committeremail) %(*authoremail) %(*committeremail)";
    let Ok(listing) = git(
        pool,
        &[
            "for-each-ref",
            format,
            "refs/heads",
            "refs/tags",
            "refs/wtpool-backup",
        ],
    ) else {
        return Vec::new();
    };
    listing
        .lines()
        .filter(|line| line.contains(VERIFY_EMAIL))
        .filter_map(|line| line.split_whitespace().next())
        .map(|refname| {
            format!(
                "{} {refname} points at a commit by wtpool-verify",
                pool.display()
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn outside_remotes__should_find_a_repository_in_a_nested_target_folder() {
        // Given
        let run = std::env::temp_dir().join(format!("xtask-remotes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&run);
        let repo = run.join("sandbox/pool/fg/target/leftover");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "--quiet"]).unwrap();
        git(
            &repo,
            &["remote", "add", "origin", "https://github.com/acme/widget"],
        )
        .unwrap();

        // When
        let found = outside_remotes(&run);

        // Then
        let _ = fs::remove_dir_all(&run);
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[rstest]
    #[case::https("https://github.com/acme/widget", false)]
    #[case::scp_form("git@github.com:acme/widget.git", false)]
    #[case::outside_absolute("/", false)]
    fn url_inside__should_refuse_urls_outside_the_run(#[case] url: &str, #[case] inside: bool) {
        // Given
        let run = std::env::temp_dir();

        // When
        let result = url_inside(url, &run, &run);

        // Then
        assert_eq!(result, inside);
    }

    #[test]
    fn url_inside__should_accept_a_bare_repository_in_the_run() {
        // Given
        let run = std::env::temp_dir();
        let bare = run.join("origin.git");

        // When
        let result = url_inside(&format!("file://{}", bare.display()), &run, &run);

        // Then
        assert!(result);
    }
}
