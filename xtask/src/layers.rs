//! The layers phase 1 adds (verification.md, "Layers"): the static checks of the extension,
//! `render`, `host`, `shot`, `e2e` and `bench`, and the product builds they test through:
//! `npm run build`, `npm run package` and `npm run generate` in vscode/.
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
            Some(only) => self
                .registry
                .layers
                .get(only)
                .is_some_and(|layers| layers.contains(layer)),
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
}
