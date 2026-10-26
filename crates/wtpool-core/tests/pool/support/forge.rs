//! A fake forge that answers every pull request lookup the same way.

use wtpool_core::{
    BranchName, ChangeHead, ChangeRef, Fetch, Forge, ObjectId, Remote, Report, Unresolved,
};

/// A [`Forge`] that names pull requests as GitHub does, `42`, `#42` or a URL ending in
/// `/pull/42`, and gives every one the head of its [`PrAnswer`]. It looks a PR up only in the
/// fixture's origin.
pub(crate) struct FakePr(pub(crate) PrAnswer);

impl Forge for FakePr {
    type Change = String;

    fn change(&self, reference: &str) -> Option<String> {
        let number = reference
            .rsplit_once("/pull/")
            .map_or(reference.strip_prefix('#').unwrap_or(reference), |(_, n)| n);
        (!number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
            .then(|| number.to_owned())
    }

    fn change_forms(&self) -> &'static str {
        "a PR number, a github.com PR URL"
    }

    fn head(
        &self,
        number: String,
        remote: Remote<'_>,
        _: &mut impl Report,
    ) -> Result<ChangeHead, Unresolved> {
        let (PrAnswer::View(view), Some(url)) = (&self.0, remote.url) else {
            panic!("looked up PR #{number} in {}", remote.name);
        };
        assert!(
            url.ends_with("/github.com/acme/app.git"),
            "looked up PR #{number} in {} at {url}",
            remote.name
        );
        let branch = if view.cross_repository {
            format!("pr-{number}/{}", view.head_ref_name)
        } else {
            view.head_ref_name.clone()
        };
        Ok(ChangeHead {
            name: format!("PR #{number}"),
            branch: BranchName::try_new(&branch).unwrap(),
            head: view.head.clone(),
            fetch: if view.cross_repository {
                Fetch::ChangeRef
            } else {
                Fetch::Branch
            },
            change_ref: ChangeRef {
                refspec: format!("refs/pull/{number}/head"),
                note: format!(
                    "{branch} comes from a fork: no upstream set, git pull will not work there"
                ),
            },
        })
    }

    fn repository(&self, _: &str) -> Option<String> {
        None
    }
}

/// The answer of a [`FakePr`] to every lookup.
#[derive(Debug, Clone)]
pub(crate) enum PrAnswer {
    /// The test must not look any PR up.
    NoLookup,
    View(PrView),
}

/// The head of a pull request, as GitHub tells it.
#[derive(Debug, Clone)]
pub(crate) struct PrView {
    head_ref_name: String,
    head: ObjectId,
    cross_repository: bool,
}

pub(crate) fn pr_view(head_ref_name: &str, head: &str, cross_repository: bool) -> PrView {
    PrView {
        head_ref_name: head_ref_name.to_owned(),
        head: ObjectId::new(head).unwrap(),
        cross_repository,
    }
}
