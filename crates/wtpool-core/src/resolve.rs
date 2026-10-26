//! Resolving a `<ref>` to the local branch it names
//! ([`plan`](crate::pool::Pool::plan), with lookups only), and fetching, creating or syncing that
//! branch once the plan is checked ([`apply`](crate::pool::Pool::apply)).

mod apply;
mod divergence;
mod guard;
mod plan;

pub(crate) use divergence::{Divergence, left_right};
pub use plan::Unresolved;
