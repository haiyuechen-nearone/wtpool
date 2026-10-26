use std::process::Command;

use crate::error::Error;
use crate::names::{ObjectId, PrNumber, RepoSlug};
use crate::run;

/// Looks pull requests up on GitHub.
pub trait ViewPr {
    /// The head of PR `number` in `repo`.
    fn view_pr(&self, repo: &RepoSlug, number: &PrNumber) -> Result<PrView, ViewPrError>;
}

/// The head of a pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrView {
    /// The head branch name in the repository the PR comes from.
    pub head_ref_name: String,
    pub head: ObjectId,
    /// The PR comes from a fork.
    pub cross_repository: bool,
}

/// The exit status of `gh` when it has no login to ask GitHub with.
const AUTH_REQUIRED: i32 = 4;

/// The reason [`ViewPr::view_pr`] found no PR head.
#[derive(Debug, thiserror::Error)]
#[must_use]
pub enum ViewPrError {
    #[error("the GitHub CLI is not installed")]
    Unavailable,
    /// `gh` has no GitHub login: `said` is the last line it wrote to standard error.
    #[error("the GitHub CLI is not logged in")]
    NotLoggedIn { said: String },
    #[error("no such PR")]
    NotFound,
    /// `gh` failed for another reason, such as no network: `said` is the last line it wrote
    /// to standard error, `code` its exit status.
    #[error("the GitHub CLI failed")]
    GhFailed { said: String, code: Option<i32> },
    #[error("unexpected answer from GitHub")]
    Malformed,
    #[error(transparent)]
    Failed(#[from] Error),
}

/// [`ViewPr`] through the `gh` command line tool.
#[derive(Debug, Clone, Copy, Default)]
pub struct GhCli;

impl ViewPr for GhCli {
    fn view_pr(&self, repo: &RepoSlug, number: &PrNumber) -> Result<PrView, ViewPrError> {
        if !run::on_path("gh") {
            return Err(ViewPrError::Unavailable);
        }
        let repo = repo.to_string();
        let ran = run::run(Command::new("gh").args([
            "pr",
            "view",
            number.as_str(),
            "--repo",
            &repo,
            "--json",
            "headRefName,headRefOid,isCrossRepository",
            "--jq",
            r#"[.headRefName, .headRefOid, (.isCrossRepository | tostring)] | join(" ")"#,
        ]))?;
        if !ran.success {
            return Err(failure(ran.code, &ran.stderr));
        }
        let mut fields = ran.stdout.split_whitespace();
        let (Some(name), Some(head), Some(cross)) = (fields.next(), fields.next(), fields.next())
        else {
            return Err(ViewPrError::Malformed);
        };
        Ok(PrView {
            head_ref_name: name.to_owned(),
            head: ObjectId::new(head).ok_or(ViewPrError::Malformed)?,
            cross_repository: cross == "true",
        })
    }
}

/// The cause of a failed `gh pr view`, from its exit status and standard error: no login, no
/// such PR, or another failure. Only GitHub's own answer that no PR has the number counts as
/// not found.
fn failure(code: Option<i32>, stderr: &str) -> ViewPrError {
    let said = stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_owned();
    if code == Some(AUTH_REQUIRED) || stderr.contains("gh auth login") {
        ViewPrError::NotLoggedIn { said }
    } else if stderr.contains("Could not resolve to a PullRequest") {
        ViewPrError::NotFound
    } else {
        ViewPrError::GhFailed { said, code }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const LOGIN: &str = "To get started with GitHub CLI, please run:  gh auth login\n\
        Alternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token.\n";
    const LAST_LOGIN_LINE: &str = "Alternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token.";

    /// The variant of `error` and the line of gh it keeps, for comparison.
    fn summary(error: &ViewPrError) -> (&'static str, &str) {
        match error {
            ViewPrError::Unavailable => ("unavailable", ""),
            ViewPrError::NotLoggedIn { said } => ("not logged in", said),
            ViewPrError::NotFound => ("not found", ""),
            ViewPrError::GhFailed { said, .. } => ("failed", said),
            ViewPrError::Malformed => ("malformed", ""),
            ViewPrError::Failed(_) => ("could not run", ""),
        }
    }

    #[rstest]
    #[case::not_logged_in(Some(4), LOGIN, ("not logged in", LAST_LOGIN_LINE))]
    #[case::bad_token(
        Some(1),
        "HTTP 401: Bad credentials (https://api.github.com/graphql)\nTry authenticating with:  gh auth login\n",
        ("not logged in", "Try authenticating with:  gh auth login")
    )]
    #[case::no_such_pr(
        Some(1),
        "GraphQL: Could not resolve to a PullRequest with the number of 42. (repository.pullRequest)\n",
        ("not found", "")
    )]
    #[case::no_network(
        Some(1),
        "error connecting to api.github.com\ncheck your internet connection or https://githubstatus.com\n",
        ("failed", "check your internet connection or https://githubstatus.com")
    )]
    #[case::no_such_repository(
        Some(1),
        "GraphQL: Could not resolve to a Repository with the name 'acme/app'. (repository)\n",
        ("failed", "GraphQL: Could not resolve to a Repository with the name 'acme/app'. (repository)")
    )]
    #[case::silent(None, "", ("failed", ""))]
    fn failure__should_tell_a_missing_login_from_a_missing_pr(
        #[case] code: Option<i32>,
        #[case] stderr: &str,
        #[case] expected: (&str, &str),
    ) {
        // When
        let error = failure(code, stderr);

        // Then
        assert_eq!(summary(&error), expected);
    }
}
