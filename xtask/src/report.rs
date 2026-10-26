//! `verify --report`: the verdict over a run's parts, with the checks the report computes itself,
//! written as `report.md`, `report.html` and `report.json`.

mod markdown;
mod verdict;
mod write;

pub use verdict::{Verdict, required_parts, verdict, what_to_do};
pub use write::report;
