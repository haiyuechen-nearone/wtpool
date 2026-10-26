//! The repository a pool belongs to (S10): the main worktree's folder name and the GitHub
//! `<owner>/<repo>` that a remote's URL names, read without the network.

use super::document::Repository;
use crate::paths::AbsolutePath;
use crate::run::Git;

/// The URL forms a GitHub remote is recognised by, each followed by `<owner>/<repo>` and an
/// optional `.git`.
const GITHUB_PREFIXES: [&str; 3] = [
    "https://github.com/",
    "git@github.com:",
    "ssh://git@github.com/",
];

/// The repository whose main worktree is called `name`, with the GitHub slug of `remote`'s URL
/// as git reads it in `dir`.
#[must_use]
pub fn repository(name: String, dir: &AbsolutePath, remote: &str) -> Repository {
    Repository {
        name,
        github: remote_url(dir, remote).as_deref().and_then(github_slug),
    }
}

/// The URL of the remote `remote` in the repository of `dir`, `None` without such a remote.
#[must_use]
pub fn remote_url(dir: &AbsolutePath, remote: &str) -> Option<String> {
    let ran = Git::at(dir).run(["remote", "get-url", "--", remote]).ok()?;
    ran.line().filter(|url| !url.is_empty()).map(str::to_owned)
}

/// `<owner>/<repo>` of a GitHub URL: `https://github.com/<owner>/<repo>`,
/// `git@github.com:<owner>/<repo>` or `ssh://git@github.com/<owner>/<repo>`, each with an
/// optional `.git`. Each part holds ASCII letters, digits, `-`, `_` and `.`, and is no `.` or
/// `..`. Anything else is `None`.
#[must_use]
pub fn github_slug(url: &str) -> Option<String> {
    let path = GITHUB_PREFIXES
        .iter()
        .find_map(|prefix| url.strip_prefix(prefix))?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    (is_slug_part(owner) && is_slug_part(repo)).then(|| format!("{owner}/{repo}"))
}

fn is_slug_part(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::status::fixture::{must_fixture, must_git};

    #[rstest]
    #[case::https("https://github.com/acme/app", Some("acme/app"))]
    #[case::https_dot_git("https://github.com/acme/app.git", Some("acme/app"))]
    #[case::scp_like("git@github.com:acme/app", Some("acme/app"))]
    #[case::scp_like_dot_git("git@github.com:acme/app.git", Some("acme/app"))]
    #[case::ssh("ssh://git@github.com/acme/app", Some("acme/app"))]
    #[case::ssh_dot_git("ssh://git@github.com/acme/app.git", Some("acme/app"))]
    #[case::dots_and_dashes("https://github.com/acme-co/my_app.v2.git", Some("acme-co/my_app.v2"))]
    #[case::only_one_suffix_dropped("https://github.com/acme/app.git.git", Some("acme/app.git"))]
    #[case::other_host("https://gitlab.com/acme/app.git", None)]
    #[case::enterprise("https://github.acme.com/acme/app.git", None)]
    #[case::subdomain("https://www.github.com/acme/app", None)]
    #[case::plain_http("http://github.com/acme/app", None)]
    #[case::credentials("https://user@github.com/acme/app.git", None)]
    #[case::ssh_with_a_port("ssh://git@github.com:22/acme/app.git", None)]
    #[case::upper_case_host("https://GitHub.com/acme/app", None)]
    #[case::local_path("/srv/git/app.git", None)]
    #[case::local_path_under_github_com("/srv/github.com/acme/app.git", None)]
    #[case::owner_only("git@github.com:acme", None)]
    #[case::trailing_slash("https://github.com/acme/app/", None)]
    #[case::nested_path("https://github.com/acme/app/extra", None)]
    #[case::empty_owner("https://github.com//app", None)]
    #[case::empty_repo("https://github.com/acme/.git", None)]
    #[case::dot_dot("https://github.com/acme/..", None)]
    #[case::dot("https://github.com/./app", None)]
    #[case::query("https://github.com/acme/app?tab=readme", None)]
    #[case::space("https://github.com/acme/my app", None)]
    #[case::non_ascii("https://github.com/acmé/app", None)]
    #[case::empty("", None)]
    fn github_slug__should_read_the_three_github_forms_only(
        #[case] url: &str,
        #[case] expected: Option<&str>,
    ) {
        // When
        let slug = github_slug(url);

        // Then
        assert_eq!(slug.as_deref(), expected);
    }

    #[test]
    fn repository__should_read_the_remote_of_the_pool() {
        // Given
        let fixture = must_fixture();
        let main = AbsolutePath::try_new(&fixture.main).unwrap();
        must_git(
            &fixture.main,
            &["remote", "add", "github", "git@github.com:acme/app.git"],
        );

        // When
        let github = repository("app".to_owned(), &main, "github");
        let local = repository("app".to_owned(), &main, "origin");
        let missing = repository("app".to_owned(), &main, "--nope");

        // Then
        assert_eq!(github.github.as_deref(), Some("acme/app"));
        assert_eq!(github.name, "app");
        assert_eq!(local.github, None);
        assert!(remote_url(&main, "origin").is_some_and(|url| url.ends_with("origin.git")));
        assert_eq!(missing.github, None);
        assert_eq!(remote_url(&main, "--nope"), None);
    }
}
