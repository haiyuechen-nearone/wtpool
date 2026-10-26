//! The layers besides `int`: the static checks, `render`, `host`, `shot`, `e2e`, `bench` and
//! `cc`, and the product builds they test through, `npm run build`, `npm run package` and
//! `npm run generate` in vscode/.
//!
//! `WTPOOL_VERIFY_WINDOWS=0` records the checks of `host` and `e2e`, the layers that open VS Code
//! windows, as not run, for runs inside a time when windows may not take focus.

mod bench;
mod builds;
mod cc;
mod cc_judge;
mod e2e;
mod focus;
mod js;
mod manifests;
mod product;
mod statics;

pub use bench::{BENCH_MODULE, bench};
pub use builds::{release_wtpool, web_build_once};
pub use cc::{cc, replay_only_reason, report_by_replay};
pub use cc_judge::REPLAY_ONLY;
pub use e2e::build_pool;
pub use js::{quick, ui};
pub use manifests::wtpool_path_problem;
pub use statics::{listen_call, network_import, text_findings};

use std::time::Duration;

use crate::parts::Ctx;
use crate::web::{self, Registry};

/// The limit of an install or build step.
const SETUP_LIMIT: Duration = Duration::from_secs(1800);
/// The limit of one layer's test runner.
const LAYER_LIMIT: Duration = Duration::from_secs(3600);

/// The checks one part wants of each layer: every check, or the one `--only` names.
struct Wanted<'a> {
    ctx: &'a Ctx,
    registry: &'a Registry,
}

impl Wanted<'_> {
    fn layer(&self, layer: &str) -> bool {
        match &self.ctx.only {
            None => true,
            Some(only) => {
                (layer == "static" && statics::UNREGISTERED.contains(&only.as_str()))
                    || self
                        .registry
                        .layers
                        .get(only)
                        .is_some_and(|layers| layers.contains(layer))
            }
        }
    }

    fn check(&self, check: &str) -> bool {
        self.ctx.wants(check)
    }

    /// The wanted checks of `layer`.
    fn in_layer(&self, layer: &str) -> Vec<String> {
        let mut checks = self.registry.in_layer(layer);
        checks.retain(|check| self.check(check));
        checks.into_iter().collect()
    }

    fn grep(&self) -> Option<String> {
        self.ctx.only.as_deref().map(web::title_pattern)
    }

    /// The tests a runner leaves out without `--only`: those of later phases, so that none of
    /// them, nor an error of its file, reaches a run of this phase.
    fn grep_invert(&self) -> Option<String> {
        if self.ctx.only.is_some() || self.ctx.later.is_empty() {
            return None;
        }
        let patterns: Vec<String> = self
            .ctx
            .later
            .iter()
            .map(|check| web::title_pattern(check))
            .collect();
        Some(patterns.join("|"))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;
    use std::time::Instant;

    use rstest::rstest;

    use super::*;

    fn ctx(only: Option<&str>) -> Ctx {
        Ctx {
            root: PathBuf::from("."),
            phase: 4,
            run_dir: PathBuf::from("."),
            commit: String::new(),
            only: only.map(str::to_owned),
            deadline: Instant::now(),
            low_priority: false,
            later: BTreeSet::new(),
        }
    }

    #[rstest]
    #[case::every_layer_without_only(None, "static", true)]
    #[case::a_static_check_no_registry_lists(Some("static:ascii_bundles"), "static", true)]
    #[case::eslint(Some("static:eslint"), "static", true)]
    #[case::stylelint(Some("static:stylelint"), "static", true)]
    #[case::no_other_layer(Some("static:ascii_bundles"), "render", false)]
    #[case::a_rust_check_the_part_runs(Some("static:fmt"), "static", false)]
    #[case::a_registered_check(Some("rows__a"), "render", true)]
    #[case::not_its_layer(Some("rows__a"), "static", false)]
    fn wanted_layer__should_follow_the_only_check(
        #[case] only: Option<&str>,
        #[case] layer: &str,
        #[case] expected: bool,
    ) {
        // Given
        let ctx = ctx(only);
        let registry = Registry {
            layers: BTreeMap::from([("rows__a".to_owned(), BTreeSet::from(["render".to_owned()]))]),
        };
        let wanted = Wanted {
            ctx: &ctx,
            registry: &registry,
        };

        // When
        let found = wanted.layer(layer);

        // Then
        assert_eq!(found, expected);
    }
}
