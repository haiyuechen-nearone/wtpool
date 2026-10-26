//! The GitHub adapter of `wtpool fg` and `wtpool bg`: a `<ref>` may name a pull request by number
//! or URL, and `gh` looks its head up. [`GitHub`] is the [`Forge`](wtpool_core::Forge) that the
//! binary `wtpool` gives its pool, which also names the repository of a remote on github.com.

mod forge;
mod gh;
mod names;
mod urls;

pub use forge::{GitHub, PullRequest};
