//! Resolving a `<ref>` to a [`Plan`] with lookups only.

use crate::error::{Error, GitError};
use crate::forge::{ChangeHead, Forge, Remote};
use crate::names::BranchName;
use crate::pool::Pool;
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
    LocalChange(ChangeHead),
    /// Only the remote has the branch.
    FetchBranch,
    /// The branch of this change is not local yet.
    FetchChange(ChangeHead),
}

/// A `<ref>` that names no branch to use.
#[derive(Debug)]
#[must_use]
pub enum Unresolved {
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

impl<F> Pool<F> {
    /// Resolves a reference to a branch name with lookups only.
    pub(crate) fn plan(&self, reference: &str, report: &mut impl Report) -> Result<Plan, Unresolved>
    where
        F: Forge,
    {
        if let Some(change) = self.forge.change(reference) {
            return self.plan_change(change, report);
        }
        let branch = BranchName::try_new(reference).map_err(|_| Error::InvalidRef {
            reference: reference.to_owned(),
            forms: self.forge.change_forms(),
        })?;
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

    fn plan_change(&self, change: F::Change, report: &mut impl Report) -> Result<Plan, Unresolved>
    where
        F: Forge,
    {
        let url = self.remote_url(report)?;
        let remote = Remote {
            name: &self.config.remote,
            url: url.as_deref(),
            foreground: &self.config.foreground,
        };
        let head = self.forge.head(change, remote, report)?;
        let branch = head.branch.clone();
        let step = if self.has_local_branch(&branch)? {
            Step::LocalChange(head)
        } else {
            Step::FetchChange(head)
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
                Err(GitError::LookUpRemote {
                    branch: branch.to_string(),
                    remote: remote.to_string(),
                }
                .into())
            }
        }
    }
}

/// A full ref, as git takes a tag before a branch of the same short name, and
/// expands a fetch destination such as `tags/x` under `refs/` instead.
pub(super) fn branch_ref(branch: &BranchName) -> String {
    format!("refs/heads/{branch}")
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::quiet(Unresolved::Quiet, &["'42': no slot changed"])]
    #[case::fatal_cause_first(
        Unresolved::Fatal(GitError::Fetch.into()),
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
