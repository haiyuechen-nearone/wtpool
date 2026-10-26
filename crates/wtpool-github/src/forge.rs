//! [`GitHub`] as the [`Forge`] of a pool: pull requests named by number or URL, and their heads.

use wtpool_core::{
    BranchName, ChangeHead, ChangeRef, Error, Fetch, Forge, Remote, Report, Unresolved, exit_status,
};

use crate::gh::{GhCli, PrView, ViewPr, ViewPrError};
use crate::names::{PrNumber, RepoSlug};
use crate::urls;

/// GitHub, whose pull requests `gh` looks up.
#[derive(Debug, Clone, Copy, Default)]
pub struct GitHub;

/// A pull request as a `<ref>` names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    number: PrNumber,
    /// The repository a PR URL names. `None` for a bare number.
    repo: Option<RepoSlug>,
}

impl Forge for GitHub {
    type Change = PullRequest;

    /// A PR number (`1234` or `#1234`) or a URL starting with
    /// `http(s)://github.com/<owner>/<repo>/pull/<n>`.
    fn change(&self, reference: &str) -> Option<PullRequest> {
        if let Some((repo, number)) = urls::pull_request(reference) {
            return Some(PullRequest {
                number,
                repo: Some(repo),
            });
        }
        let number = PrNumber::new(reference.strip_prefix('#').unwrap_or(reference))?;
        Some(PullRequest { number, repo: None })
    }

    fn change_forms(&self) -> &'static str {
        "a PR number, a github.com PR URL"
    }

    fn head(
        &self,
        pr: PullRequest,
        remote: Remote<'_>,
        report: &mut dyn Report,
    ) -> Result<ChangeHead, Unresolved> {
        head_of(&GhCli, &pr, remote).map_err(|refused| match refused {
            Refused::Warning(warning) => {
                report.warn(&warning);
                Unresolved::Quiet
            }
            Refused::Failed(error) => Unresolved::Fatal(error),
        })
    }

    fn repository(&self, url: &str) -> Option<String> {
        urls::remote_repository(url).map(|slug| slug.to_string())
    }
}

/// Why a pull request gives no head to use.
#[derive(Debug)]
enum Refused {
    /// The reason, for the user.
    Warning(String),
    /// `gh` could not run.
    Failed(Error),
}

/// The head of `pr` as `prs` looks it up in the repository of `remote`.
fn head_of(prs: &impl ViewPr, pr: &PullRequest, remote: Remote<'_>) -> Result<ChangeHead, Refused> {
    let PullRequest { number, repo } = pr;
    let name = remote.name;
    let Some(url) = remote.url else {
        return Err(Refused::Warning(format!(
            "no remote '{name}' in {}, so PR #{number} cannot be looked up",
            remote.foreground.display()
        )));
    };
    let Some(slug) = urls::remote_repository(url) else {
        return Err(Refused::Warning(format!(
            "remote '{name}' ({url}) is not on github.com, so PR #{number} cannot be looked up; PR refs need a github.com remote (GitHub Enterprise is not supported)"
        )));
    };
    if let Some(repo) = repo
        && !repo.matches(&slug)
    {
        return Err(Refused::Warning(format!(
            "PR #{number} is in {repo}, but remote '{name}' is {slug}"
        )));
    }
    let view = prs
        .view_pr(&slug, number)
        .map_err(|error| refused(error, number, &slug))?;
    change_head(number, view)
}

/// Why `gh` gave no head of PR `number` in `slug`.
fn refused(error: ViewPrError, number: &PrNumber, slug: &RepoSlug) -> Refused {
    let warning = match error {
        ViewPrError::Failed(error) => return Refused::Failed(error),
        ViewPrError::Unavailable => format!(
            "gh is not installed, so PR #{number} cannot be looked up; install it from https://cli.github.com"
        ),
        ViewPrError::NotLoggedIn { said } => {
            format!("gh is not logged in: run `gh auth login`{}", gh_said(&said))
        }
        ViewPrError::NotFound => format!("no PR #{number} in {slug}"),
        ViewPrError::GhFailed { said, code } => {
            let cause = if said.is_empty() {
                format!(" ({})", exit_status(code))
            } else {
                format!(": {said}")
            };
            format!("gh could not look up PR #{number} in {slug}{cause}")
        }
        ViewPrError::Malformed => format!("unexpected answer from GitHub for PR #{number}"),
    };
    Refused::Warning(warning)
}

/// The head `view` names, on a branch `pr-<n>/<branch>` for a PR from a fork, so it never takes
/// the name of a branch of this repository.
fn change_head(number: &PrNumber, view: PrView) -> Result<ChangeHead, Refused> {
    let fork = view.cross_repository;
    let name = if fork {
        format!("pr-{number}/{}", view.head_ref_name)
    } else {
        view.head_ref_name
    };
    let Ok(branch) = BranchName::try_new(&name) else {
        return Err(Refused::Warning(format!(
            "PR #{number} has head branch '{name}', which is not a valid branch name"
        )));
    };
    let note = format!("{branch} comes from a fork: no upstream set, git pull will not work there");
    Ok(ChangeHead {
        name: format!("PR #{number}"),
        branch,
        head: view.head,
        fetch: if fork {
            Fetch::ChangeRef
        } else {
            Fetch::Branch
        },
        change_ref: ChangeRef {
            refspec: format!("refs/pull/{number}/head"),
            note,
        },
    })
}

/// gh's last line on standard error, as a message shows it after its own words.
fn gh_said(said: &str) -> String {
    if said.is_empty() {
        String::new()
    } else {
        format!(" (gh: {said})")
    }
}

#[cfg(test)]
mod tests;
