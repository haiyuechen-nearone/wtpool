use std::cell::RefCell;
use std::io;
use std::path::Path;

use rstest::rstest;
use wtpool_core::{GitError, ObjectId, RemoteName};

use super::*;

const HEAD: &str = "1111111111111111111111111111111111111111";
const ORIGIN: &str = "git@github.com:acme/app.git";

/// A [`ViewPr`] that gives its answer to the first lookup and records each lookup.
struct FakePr {
    answer: RefCell<Option<Result<PrView, ViewPrError>>>,
    asked: RefCell<Vec<String>>,
}

impl ViewPr for FakePr {
    fn view_pr(&self, repo: &RepoSlug, number: &PrNumber) -> Result<PrView, ViewPrError> {
        self.asked.borrow_mut().push(format!("{repo} #{number}"));
        self.answer
            .borrow_mut()
            .take()
            .unwrap_or(Err(ViewPrError::Malformed))
    }
}

fn view(head_ref_name: &str, cross_repository: bool) -> PrView {
    PrView {
        head_ref_name: head_ref_name.to_owned(),
        head: ObjectId::new(HEAD).unwrap(),
        cross_repository,
    }
}

/// The warning, or the error's messages, of a refused head.
fn reason(refused: Refused) -> String {
    match refused {
        Refused::Warning(warning) => warning,
        Refused::Failed(error) => error.messages().join("\n"),
    }
}

/// The head of the PR `reference` names, on a remote `origin` at `url`, when gh answers
/// `answer`, and the lookups gh was asked for.
fn must_head(
    answer: Result<PrView, ViewPrError>,
    reference: &str,
    url: Option<&str>,
) -> (Result<ChangeHead, String>, Vec<String>) {
    let prs = FakePr {
        answer: RefCell::new(Some(answer)),
        asked: RefCell::new(Vec::new()),
    };
    let origin = RemoteName::default();
    let remote = Remote {
        name: &origin,
        url,
        foreground: Path::new("/repo/app"),
    };
    let pr = GitHub.change(reference).unwrap();
    let head = head_of(&prs, &pr, remote).map_err(reason);
    (head, prs.asked.take())
}

#[rstest]
#[case::number("1234", ("1234", None))]
#[case::hash_number("#1234", ("1234", None))]
#[case::https_url("https://github.com/acme/widget/pull/1234", ("1234", Some("acme/widget")))]
#[case::http_url_with_a_tail(
    "http://github.com/acme/widget-private/pull/567/files#diff",
    ("567", Some("acme/widget-private"))
)]
fn change__should_read_a_pr_number_or_url(
    #[case] reference: &str,
    #[case] expected: (&str, Option<&str>),
) {
    // When
    let pr = GitHub.change(reference);

    // Then
    let pr = pr.map(|pr| (pr.number.to_string(), pr.repo.map(|repo| repo.to_string())));
    assert_eq!(
        pr,
        Some((expected.0.to_owned(), expected.1.map(str::to_owned)))
    );
}

#[rstest]
#[case::branch("main")]
#[case::digits_then_letters("1234a")]
#[case::two_hashes("##1234")]
#[case::issue_url("https://github.com/acme/widget/issues/1234")]
#[case::other_host("https://gitlab.com/acme/widget/pull/1234")]
#[case::github_enterprise_host("https://github.acme.com/acme/widget/pull/1234")]
#[case::invalid_branch_name("a..b")]
fn change__should_name_no_pr_for_anything_else(#[case] reference: &str) {
    // When
    let pr = GitHub.change(reference);

    // Then
    assert_eq!(pr, None);
}

#[test]
fn change_forms__should_name_the_number_and_url_forms() {
    // When
    let forms = GitHub.change_forms();

    // Then
    assert_eq!(forms, "a PR number, a github.com PR URL");
}

#[rstest]
#[case::scp_like(ORIGIN, Some("acme/app"))]
#[case::https("https://github.com/acme/app", Some("acme/app"))]
#[case::local_path_under_github_com("/srv/github.com/acme/app.git", None)]
#[case::trailing_slash("https://github.com/acme/app/", None)]
fn repository__should_name_only_a_github_remote(#[case] url: &str, #[case] expected: Option<&str>) {
    // When
    let repository = GitHub.repository(url);

    // Then
    assert_eq!(repository.as_deref(), expected);
}

#[rstest]
#[case::no_origin_remote(
    Err(ViewPrError::Malformed),
    "42",
    None,
    "no remote 'origin' in /repo/app, so PR #42 cannot be looked up",
    &[]
)]
#[case::origin_not_on_github(
    Err(ViewPrError::Malformed),
    "42",
    Some("/srv/elsewhere.git"),
    "remote 'origin' (/srv/elsewhere.git) is not on github.com, so PR #42 cannot be looked up; PR refs need a github.com remote (GitHub Enterprise is not supported)",
    &[]
)]
#[case::origin_under_a_github_com_folder(
    Err(ViewPrError::Malformed),
    "42",
    Some("/srv/github.com/acme/app.git"),
    "remote 'origin' (/srv/github.com/acme/app.git) is not on github.com, so PR #42 cannot be looked up; PR refs need a github.com remote (GitHub Enterprise is not supported)",
    &[]
)]
#[case::origin_on_github_enterprise(
    Err(ViewPrError::Malformed),
    "42",
    Some("git@github.acme.com:acme/app.git"),
    "remote 'origin' (git@github.acme.com:acme/app.git) is not on github.com, so PR #42 cannot be looked up; PR refs need a github.com remote (GitHub Enterprise is not supported)",
    &[]
)]
#[case::url_of_another_repository(
    Err(ViewPrError::Malformed),
    "https://github.com/acme/app-private/pull/42",
    Some(ORIGIN),
    "PR #42 is in acme/app-private, but remote 'origin' is acme/app",
    &[]
)]
#[case::gh_not_installed(
    Err(ViewPrError::Unavailable),
    "42",
    Some(ORIGIN),
    "gh is not installed, so PR #42 cannot be looked up; install it from https://cli.github.com",
    &["acme/app #42"]
)]
#[case::gh_not_logged_in(
    Err(ViewPrError::NotLoggedIn { stderr_line: "Alternatively, populate the GH_TOKEN environment variable.".to_owned() }),
    "42",
    Some(ORIGIN),
    "gh is not logged in: run `gh auth login` (gh: Alternatively, populate the GH_TOKEN environment variable.)",
    &["acme/app #42"]
)]
#[case::gh_not_logged_in_silently(
    Err(ViewPrError::NotLoggedIn { stderr_line: String::new() }),
    "42",
    Some(ORIGIN),
    "gh is not logged in: run `gh auth login`",
    &["acme/app #42"]
)]
#[case::gh_failed(
    Err(ViewPrError::GhFailed { stderr_line: "error connecting to api.github.com".to_owned(), code: Some(1) }),
    "42",
    Some(ORIGIN),
    "gh could not look up PR #42 in acme/app: error connecting to api.github.com",
    &["acme/app #42"]
)]
#[case::gh_failed_silently(
    Err(ViewPrError::GhFailed { stderr_line: String::new(), code: Some(2) }),
    "42",
    Some(ORIGIN),
    "gh could not look up PR #42 in acme/app (exit status 2)",
    &["acme/app #42"]
)]
#[case::gh_killed(
    Err(ViewPrError::GhFailed { stderr_line: String::new(), code: None }),
    "42",
    Some(ORIGIN),
    "gh could not look up PR #42 in acme/app (killed by a signal)",
    &["acme/app #42"]
)]
#[case::no_such_pr(
    Err(ViewPrError::NotFound),
    "42",
    Some(ORIGIN),
    "no PR #42 in acme/app",
    &["acme/app #42"]
)]
#[case::unexpected_answer(
    Err(ViewPrError::Malformed),
    "42",
    Some(ORIGIN),
    "unexpected answer from GitHub for PR #42",
    &["acme/app #42"]
)]
#[case::head_that_is_not_a_branch_name(
    Ok(view("a..b", false)),
    "42",
    Some(ORIGIN),
    "PR #42 has head branch 'a..b', which is not a valid branch name",
    &["acme/app #42"]
)]
#[case::gh_cannot_run(
    Err(ViewPrError::from(Error::from(GitError::Spawn {
        program: "gh".to_owned(),
        source: io::Error::from(io::ErrorKind::PermissionDenied),
    }))),
    "42",
    Some(ORIGIN),
    "could not run gh: permission denied",
    &["acme/app #42"]
)]
fn head_of__should_refuse_a_pr_without_a_usable_head(
    #[case] answer: Result<PrView, ViewPrError>,
    #[case] reference: &str,
    #[case] url: Option<&str>,
    #[case] reason: &str,
    #[case] asked: &[&str],
) {
    // When
    let (head, looked_up) = must_head(answer, reference, url);

    // Then
    assert_eq!(head, Err(reason.to_owned()));
    assert_eq!(looked_up, asked);
}

#[rstest]
#[case::same_repository(
    view("feature", false),
    "42",
    ORIGIN,
    ("PR #42", "feature", Fetch::Branch, "refs/pull/42/head")
)]
#[case::fork(
    view("topic", true),
    "#7",
    ORIGIN,
    ("PR #7", "pr-7/topic", Fetch::ChangeRef, "refs/pull/7/head")
)]
#[case::url_of_the_repository_in_other_case(
    view("feature", false),
    "https://github.com/Acme/APP/pull/42",
    "https://github.com/acme/app.git",
    ("PR #42", "feature", Fetch::Branch, "refs/pull/42/head")
)]
fn head_of__should_name_the_branch_and_ref_of_the_pr(
    #[case] answer: PrView,
    #[case] reference: &str,
    #[case] url: &str,
    #[case] expected: (&str, &str, Fetch, &str),
) {
    // When
    let (head, looked_up) = must_head(Ok(answer), reference, Some(url));

    // Then
    let (name, branch, fetch, refspec) = expected;
    assert_eq!(
        head,
        Ok(ChangeHead {
            name: name.to_owned(),
            branch: BranchName::try_new(branch).unwrap(),
            head: ObjectId::new(HEAD).unwrap(),
            fetch,
            change_ref: ChangeRef {
                refspec: refspec.to_owned(),
                note: format!(
                    "{branch} comes from a fork: no upstream set, git pull will not work there"
                ),
            },
        })
    );
    let number = refspec
        .trim_start_matches("refs/pull/")
        .trim_end_matches("/head");
    assert_eq!(looked_up, [format!("acme/app #{number}")]);
}
