//! The check registry of verify/checks.toml and the results of the JavaScript layers:
//! Vitest (render), Mocha (host) and Playwright (shot, e2e and the Chromium budget).

mod merge;
mod registry;
mod reporters;

pub use merge::{layer_results, merge, test_result, worst_outcomes};
pub use registry::{CHECKS, Registry, layer_part};
pub use reporters::{
    Format, TestOutcome, check_of, classify, parse, parse_extra, strip_ansi, title_pattern,
};
