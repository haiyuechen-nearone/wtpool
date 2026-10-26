//! [`GitHub`], the [`Forge`](wtpool_core::Forge) of github.com: a `<ref>` of `wtpool fg` or
//! `wtpool bg` may name a pull request by number or URL, `gh` looks its head up, and a remote's URL
//! names its repository.

mod forge;
mod gh;
mod names;
mod urls;

pub use forge::{GitHub, PullRequest};
