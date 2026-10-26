//! The GitHub URLs a `<ref>` and a remote can hold.

use crate::names::{PrNumber, RepoSlug};

/// The URL forms of a remote on GitHub, each followed by `<owner>/<repo>` and an optional `.git`.
const REMOTE_PREFIXES: [&str; 3] = [
    "https://github.com/",
    "git@github.com:",
    "ssh://git@github.com/",
];

/// The repository of a remote URL in one of [`REMOTE_PREFIXES`]. Anything else is `None`.
pub(crate) fn remote_repository(url: &str) -> Option<RepoSlug> {
    let path = REMOTE_PREFIXES
        .iter()
        .find_map(|prefix| url.strip_prefix(prefix))?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    if is_slug_part(owner) && is_slug_part(repo) {
        RepoSlug::new(owner, repo)
    } else {
        None
    }
}

fn is_slug_part(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// The repository and number of a PR URL. Text after the number, such as `/files`, is ignored.
pub(crate) fn pull_request(reference: &str) -> Option<(RepoSlug, PrNumber)> {
    let rest = reference
        .strip_prefix("https://")
        .or_else(|| reference.strip_prefix("http://"))?
        .strip_prefix("github.com/")?;
    let (owner, rest) = rest.split_once('/')?;
    let (repo, rest) = rest.split_once('/')?;
    let rest = rest.strip_prefix("pull/")?;
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    Some((
        RepoSlug::new(owner, repo)?,
        PrNumber::new(rest.get(..end)?)?,
    ))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

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
    #[case::enterprise_scp_like("git@github.acme.com:acme/app.git", None)]
    #[case::subdomain("https://www.github.com/acme/app", None)]
    #[case::host_ending_in_github_com("https://notgithub.com/acme/app", None)]
    #[case::plain_http("http://github.com/acme/app", None)]
    #[case::git_protocol("git://github.com/acme/app.git", None)]
    #[case::credentials("https://user@github.com/acme/app.git", None)]
    #[case::scp_like_without_a_user("github.com:acme/app.git", None)]
    #[case::scp_like_with_another_user("alice@github.com:acme/app.git", None)]
    #[case::ssh_without_a_user("ssh://github.com/acme/app.git", None)]
    #[case::ssh_with_a_port("ssh://git@github.com:22/acme/app.git", None)]
    #[case::git_plus_ssh("git+ssh://git@github.com/acme/app.git", None)]
    #[case::upper_case_host("https://GitHub.com/acme/app", None)]
    #[case::local_path("/srv/git/app.git", None)]
    #[case::local_path_under_github_com("/srv/github.com/acme/app.git", None)]
    #[case::file_url_under_github_com("file:///srv/github.com/acme/app.git", None)]
    #[case::owner_only("git@github.com:acme", None)]
    #[case::trailing_slash("https://github.com/acme/app/", None)]
    #[case::trailing_slash_after_dot_git("https://github.com/acme/app.git/", None)]
    #[case::nested_path("https://github.com/acme/app/extra", None)]
    #[case::empty_owner("https://github.com//app", None)]
    #[case::empty_repo("https://github.com/acme/.git", None)]
    #[case::dot_dot("https://github.com/acme/..", None)]
    #[case::dot("https://github.com/./app", None)]
    #[case::query("https://github.com/acme/app?tab=readme", None)]
    #[case::space("https://github.com/acme/my app", None)]
    #[case::non_ascii("https://github.com/acmé/app", None)]
    #[case::empty("", None)]
    fn remote_repository__should_read_the_three_github_forms_only(
        #[case] url: &str,
        #[case] expected: Option<&str>,
    ) {
        // When
        let slug = remote_repository(url);

        // Then
        assert_eq!(slug.map(|slug| slug.to_string()).as_deref(), expected);
    }

    #[rstest]
    #[case::https_url("https://github.com/acme/widget/pull/1234", ("acme/widget", "1234"))]
    #[case::http_url_with_a_tail(
        "http://github.com/acme/widget-private/pull/567/files#diff",
        ("acme/widget-private", "567")
    )]
    fn pull_request__should_read_the_repository_and_number(
        #[case] reference: &str,
        #[case] expected: (&str, &str),
    ) {
        // When
        let found = pull_request(reference);

        // Then
        let found = found.map(|(slug, number)| (slug.to_string(), number.to_string()));
        assert_eq!(found, Some((expected.0.to_owned(), expected.1.to_owned())));
    }

    #[rstest]
    #[case::issue_url("https://github.com/acme/widget/issues/1234")]
    #[case::url_without_an_owner("https://github.com//widget/pull/1234")]
    #[case::url_without_a_repository("https://github.com/acme")]
    #[case::url_without_a_path_after_the_repository("https://github.com/acme/widget")]
    #[case::url_without_a_number("https://github.com/acme/widget/pull/")]
    #[case::other_host("https://gitlab.com/acme/widget/pull/1234")]
    #[case::github_enterprise_host("https://github.acme.com/acme/widget/pull/1234")]
    #[case::not_a_url("1234")]
    fn pull_request__should_refuse_any_other_text(#[case] reference: &str) {
        // When
        let found = pull_request(reference);

        // Then
        assert_eq!(found, None);
    }
}
