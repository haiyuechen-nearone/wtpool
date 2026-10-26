//! Resolving a `<ref>` to a [`Plan`] with lookups only.

use crate::error::{self, Error};
use crate::github::{ViewPr, ViewPrError};
use crate::names::{BranchName, ObjectId, PrNumber, RepoSlug};
use crate::pool::Pool;
use crate::refs::Target;
use crate::report::Report;

/// A `<ref>` resolved to the local branch it names, with lookups only.
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    pub(super) branch: BranchName,
    pub(super) step: Step,
}

#[derive(Debug, Clone)]
pub(super) enum Step {
    Local,
    LocalPr(PrHead),
    /// Only the remote has the branch.
    FetchBranch,
    /// The branch of this PR is not local yet.
    FetchPr(PrNumber),
}

#[derive(Debug, Clone)]
pub(super) struct PrHead {
    pub(super) number: PrNumber,
    pub(super) head: ObjectId,
    pub(super) fork: bool,
}

#[derive(Debug)]
#[must_use]
pub(crate) enum Unresolved {
    /// Everything there was to say, a warning or git's stderr, was reported already.
    Quiet,
    Fatal(Error),
}

impl Unresolved {
    /// The error of a command that left the pool unchanged, `consequence` naming the part that
    /// stayed as it was.
    pub(crate) fn into_error(self, reference: &str, consequence: &'static str) -> Error {
        Error::NotResolved {
            reference: reference.to_owned(),
            consequence,
            cause: match self {
                Self::Quiet => None,
                Self::Fatal(error) => Some(Box::new(error)),
            },
        }
    }
}

impl From<Error> for Unresolved {
    fn from(error: Error) -> Self {
        Self::Fatal(error)
    }
}

impl<V: ViewPr> Pool<V> {
    /// Resolves a reference to a branch name with lookups only.
    pub(crate) fn plan(
        &self,
        reference: &str,
        report: &mut impl Report,
    ) -> Result<Plan, Unresolved> {
        let target: Target = reference.parse()?;
        match target {
            Target::Pr { number, repo } => self.plan_pr(number, repo.as_ref(), report),
            Target::Branch(branch) => {
                let step = if self.has_local_branch(&branch)? {
                    Step::Local
                } else {
                    match self.remote_has_branch(&branch, report)? {
                        Some(true) => Step::FetchBranch,
                        found => {
                            return Err(Unresolved::Fatal(Error::NoSuchBranch {
                                branch: branch.to_string(),
                                remote: found.map(|_| self.config.remote.to_string()),
                            }));
                        }
                    }
                };
                Ok(Plan { branch, step })
            }
        }
    }

    fn plan_pr(
        &self,
        number: PrNumber,
        repo: Option<&RepoSlug>,
        report: &mut impl Report,
    ) -> Result<Plan, Unresolved> {
        let remote = &self.config.remote;
        let Some(url) = self.remote_url(report)? else {
            report.warn(&format!(
                "no remote '{remote}' in {}, so PR #{number} cannot be looked up",
                self.config.foreground.display()
            ));
            return Err(Unresolved::Quiet);
        };
        let Some(slug) = slug_from_url(&url) else {
            report.warn(&format!(
                "remote '{remote}' ({url}) is not on github.com, so PR #{number} cannot be looked up; PR refs need a github.com remote (GitHub Enterprise is not supported)"
            ));
            return Err(Unresolved::Quiet);
        };
        if let Some(repo) = repo
            && !repo.matches(&slug)
        {
            report.warn(&format!(
                "PR #{number} is in {repo}, but remote '{remote}' is {slug}"
            ));
            return Err(Unresolved::Quiet);
        }
        let view = match self.prs.view_pr(&slug, &number) {
            Ok(view) => view,
            Err(ViewPrError::Unavailable) => {
                report.warn(&format!(
                    "gh is not installed, so PR #{number} cannot be looked up; install it from https://cli.github.com"
                ));
                return Err(Unresolved::Quiet);
            }
            Err(ViewPrError::NotLoggedIn { said }) => {
                report.warn(&format!(
                    "gh is not logged in: run `gh auth login`{}",
                    gh_said(&said)
                ));
                return Err(Unresolved::Quiet);
            }
            Err(ViewPrError::NotFound) => {
                report.warn(&format!("no PR #{number} in {slug}"));
                return Err(Unresolved::Quiet);
            }
            Err(ViewPrError::GhFailed { said, code }) => {
                let cause = if said.is_empty() {
                    format!(" ({})", error::exit_status(code))
                } else {
                    format!(": {said}")
                };
                report.warn(&format!(
                    "gh could not look up PR #{number} in {slug}{cause}"
                ));
                return Err(Unresolved::Quiet);
            }
            Err(ViewPrError::Malformed) => {
                report.warn(&format!("unexpected answer from GitHub for PR #{number}"));
                return Err(Unresolved::Quiet);
            }
            Err(ViewPrError::Failed(error)) => return Err(Unresolved::Fatal(error)),
        };
        let fork = view.cross_repository;
        let name = if fork {
            format!("pr-{number}/{}", view.head_ref_name)
        } else {
            view.head_ref_name
        };
        let Ok(branch) = BranchName::try_new(&name) else {
            report.warn(&format!(
                "PR #{number} has head branch '{name}', which is not a valid branch name"
            ));
            return Err(Unresolved::Quiet);
        };
        let step = if self.has_local_branch(&branch)? {
            Step::LocalPr(PrHead {
                number,
                head: view.head,
                fork,
            })
        } else {
            Step::FetchPr(number)
        };
        Ok(Plan { branch, step })
    }

    fn has_local_branch(&self, branch: &BranchName) -> Result<bool, Error> {
        Ok(self
            .git()
            .run([
                "show-ref",
                "--verify",
                "--quiet",
                branch_ref(branch).as_str(),
            ])?
            .success)
    }

    /// The URL of the configured remote, `None` when the repository has no such
    /// remote. Fails when git cannot tell.
    pub(super) fn remote_url(&self, report: &mut impl Report) -> Result<Option<String>, Error> {
        let ran = self
            .git()
            .run(["remote", "get-url", self.config.remote.as_str()])?;
        // git exits with 2 when there is no such remote.
        match ran.code {
            Some(0) => Ok(ran.line().map(str::to_owned)),
            Some(2) => Ok(None),
            _ => {
                ran.forward_stderr(report);
                ran.check().map(|()| None)
            }
        }
    }

    /// `Some(true)` when the configured remote has `branch`, `None` when the repository has
    /// no such remote. Fails when the remote cannot be asked, so an unreachable
    /// remote never reads as a missing branch.
    pub(super) fn remote_has_branch(
        &self,
        branch: &BranchName,
        report: &mut impl Report,
    ) -> Result<Option<bool>, Error> {
        if self.remote_url(report)?.is_none() {
            return Ok(None);
        }
        let remote = &self.config.remote;
        // A full ref, as a short pattern also matches the tail of `refs/heads/<other>/<branch>`.
        let ran = self.git().run([
            "ls-remote",
            "--exit-code",
            "--heads",
            remote.as_str(),
            branch_ref(branch).as_str(),
        ])?;
        // `--exit-code` exits with 2 when nothing matches.
        match ran.code {
            Some(0) => Ok(Some(true)),
            Some(2) => Ok(Some(false)),
            _ => {
                ran.forward_stderr(report);
                Err(Error::LookUpRemote {
                    branch: branch.to_string(),
                    remote: remote.to_string(),
                })
            }
        }
    }
}

/// A full ref, as git takes a tag before a branch of the same short name, and
/// expands a fetch destination such as `tags/x` under `refs/` instead.
pub(super) fn branch_ref(branch: &BranchName) -> String {
    format!("refs/heads/{branch}")
}

/// The GitHub repository a remote URL points to, so gh works from any directory.
fn slug_from_url(url: &str) -> Option<RepoSlug> {
    let url = url.strip_suffix('/').unwrap_or(url);
    let url = url.strip_suffix(".git").unwrap_or(url);
    let start = ["github.com:", "github.com/"]
        .iter()
        .filter_map(|marker| url.rfind(marker).map(|at| at.saturating_add(marker.len())))
        .max()?;
    let (owner, name) = url.get(start..)?.split_once('/')?;
    RepoSlug::new(owner, name)
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
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::scp_like("git@github.com:acme/widget.git", Some("acme/widget"))]
    #[case::https("https://github.com/acme/widget.git", Some("acme/widget"))]
    #[case::https_without_suffix("https://github.com/acme/widget", Some("acme/widget"))]
    #[case::trailing_slash("https://github.com/acme/widget/", Some("acme/widget"))]
    #[case::ssh("ssh://git@github.com/acme/widget.git", Some("acme/widget"))]
    #[case::local_path_under_github_com("/srv/github.com/acme/widget.git", Some("acme/widget"))]
    #[case::local_path("/srv/git/app.git", None)]
    #[case::owner_only("git@github.com:acme", None)]
    #[case::nested_path("https://github.com/acme/widget/extra", None)]
    fn slug_from_url__should_read_github_urls_only(
        #[case] url: &str,
        #[case] expected: Option<&str>,
    ) {
        // When
        let slug = slug_from_url(url);

        // Then
        assert_eq!(slug.map(|slug| slug.to_string()).as_deref(), expected);
    }

    #[rstest]
    #[case::quiet(Unresolved::Quiet, &["'42': no slot changed"])]
    #[case::fatal_cause_first(
        Unresolved::Fatal(Error::FetchFailed),
        &["fetch failed", "'42': no slot changed"]
    )]
    fn into_error__should_name_the_reference_and_what_stayed_unchanged(
        #[case] unresolved: Unresolved,
        #[case] expected: &[&str],
    ) {
        // When
        let error = unresolved.into_error("42", "no slot changed");

        // Then
        assert_eq!(error.messages(), expected);
    }
}
