//! The [`Repository`] of a pool, read without the network.

use super::document::Repository;
use crate::git::Git;

/// The repository whose main worktree is called `name`, with the name `forge_repository` gives
/// `remote`'s URL as `git` reads it.
#[must_use]
pub fn repository(
    name: String,
    git: &Git,
    remote: &str,
    forge_repository: fn(&str) -> Option<String>,
) -> Repository {
    Repository {
        name,
        github: remote_url(git, remote)
            .as_deref()
            .and_then(forge_repository),
    }
}

/// The URL of the remote `remote` in the repository of `git`, `None` without such a remote.
#[must_use]
pub fn remote_url(git: &Git, remote: &str) -> Option<String> {
    let ran = git.run(["remote", "get-url", "--", remote]).ok()?;
    ran.line().filter(|url| !url.is_empty()).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AbsolutePath;
    use crate::status::fixture::{https_path, must_fixture, must_git};

    #[test]
    fn repository__should_read_the_remote_of_the_pool() {
        // Given
        let fixture = must_fixture();
        let main = Git::at(&AbsolutePath::try_new(&fixture.main).unwrap());
        must_git(
            &fixture.main,
            &["remote", "add", "github", "https://github.com/acme/app.git"],
        );

        // When
        let github = repository("app".to_owned(), &main, "github", https_path);
        let local = repository("app".to_owned(), &main, "origin", https_path);
        let missing = repository("app".to_owned(), &main, "--nope", https_path);

        // Then
        assert_eq!(github.github.as_deref(), Some("acme/app"));
        assert_eq!(github.name, "app");
        assert_eq!(local.github, None);
        assert!(remote_url(&main, "origin").is_some_and(|url| url.ends_with("origin.git")));
        assert_eq!(missing.github, None);
        assert_eq!(remote_url(&main, "--nope"), None);
    }
}
