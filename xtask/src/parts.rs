//! The parts of `verify --part`. [`run_part`] runs one in this process, whole or with `--only`,
//! and reuses an earlier passing result of the same inputs.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::isolation::{self, Invocation, Places};
use crate::results::{CheckResult, Reused, store_part};

pub const NEXTEST_CONFIG: &str = "verify/nextest.toml";
const VERSION_CHECK: &str = "version__should_list_version_commit_and_features";

mod ctx;
mod int;
mod quick;

pub use ctx::{Ctx, PART_LIMIT, RUNNER_VARIABLES};

/// The live Claude Code contract of the `cc` part (`--contract`, `--record`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Contract {
    /// The cc checks count by the live result of the installed Claude Code, or by the replays.
    Off,
    /// `--contract`: live Claude Code sessions.
    Live,
    /// `--contract --record`: live sessions whose raw recordings the run keeps.
    LiveRecorded,
}

impl Contract {
    /// The contract that `--contract` and `--record` ask for.
    pub fn from_flags(contract: bool, record: bool) -> Result<Self, String> {
        match (contract, record) {
            (false, false) => Ok(Self::Off),
            (true, false) => Ok(Self::Live),
            (true, true) => Ok(Self::LiveRecorded),
            (false, true) => Err("--record goes only with --contract".to_owned()),
        }
    }

    pub fn live(self) -> bool {
        self != Self::Off
    }

    pub fn recorded(self) -> bool {
        self == Self::LiveRecorded
    }

    /// Refuses a live contract for `parts` without `cc`, the only part that has one.
    pub fn check_parts(self, parts: &[&str]) -> Result<(), String> {
        if self.live() && !parts.contains(&"cc") {
            return Err("--contract goes only with the cc part".to_owned());
        }
        Ok(())
    }
}

/// Every part that `--part` names.
pub const PARTS: [&str; 5] = ["quick", "int", "ui", "bench", "cc"];

/// Runs `part` in `ctx` and stores its results in the run folder. A whole part copies an earlier
/// passing result of the same inputs instead, unless `fresh` or a live `contract` asks for a run.
pub fn run_part(
    ctx: &Ctx,
    part: &str,
    contract: Contract,
    fresh: bool,
) -> Result<Vec<CheckResult>, String> {
    contract.check_parts(&[part])?;
    let hash = if ctx.only.is_none() {
        Some(crate::reuse::input_hash(&ctx.root, ctx.phase, part)?)
    } else {
        None
    };
    if let Some(hash) = &hash
        && !fresh
        && !contract.live()
        && let Some((path, file)) =
            crate::reuse::earlier_result(&ctx.root, &ctx.run_dir, part, ctx.phase, hash)
    {
        let from = path
            .strip_prefix(&ctx.root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        let reused = Reused {
            commit: file.commit.clone(),
            hash: hash.clone(),
            from,
        };
        store_part(
            &ctx.run_dir,
            part,
            ctx.phase,
            &ctx.commit,
            &file.results,
            Some(hash.clone()),
            Some(reused),
        )?;
        return Ok(file.results);
    }
    // Every part but bench holds the machine's verify lock shared, so it waits while a bench part
    // holds it alone to measure. Bench takes its own turns.
    let _turn = if part == "bench" {
        None
    } else {
        crate::machine_lock::turn()
    };
    let places = Places::real().with_extension_of(&ctx.root);
    let before = isolation::snapshot(&places, &ctx.run_dir);
    let results = match part {
        "quick" => {
            let mut results = quick::quick(ctx);
            results.extend(crate::layers::quick(ctx));
            results
        }
        "int" => int::int(ctx),
        "ui" => crate::layers::ui(ctx),
        "bench" => crate::layers::bench(ctx),
        "cc" => crate::layers::cc(ctx, contract.live(), contract.recorded()),
        other => return Err(format!("unknown part {other}; parts: {}", PARTS.join(", "))),
    };
    let after = isolation::snapshot(&places, &ctx.run_dir);
    record_invocation(ctx, part, before, after)?;
    store_part(
        &ctx.run_dir,
        part,
        ctx.phase,
        &ctx.commit,
        &results,
        hash,
        None,
    )?;
    Ok(results)
}

fn record_invocation(
    ctx: &Ctx,
    part: &str,
    before: isolation::Snapshot,
    after: isolation::Snapshot,
) -> Result<(), String> {
    let dir = ctx.run_dir.join("isolation");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let invocation = Invocation {
        part: part.to_owned(),
        before,
        after,
    };
    let text = serde_json::to_string_pretty(&invocation).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{part}-{millis}.json"));
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::off(false, false, Ok(Contract::Off))]
    #[case::live(true, false, Ok(Contract::Live))]
    #[case::recorded(true, true, Ok(Contract::LiveRecorded))]
    #[case::record_alone(false, true, Err("--record goes only with --contract".to_owned()))]
    fn contract__should_follow_the_flags(
        #[case] contract: bool,
        #[case] record: bool,
        #[case] expected: Result<Contract, String>,
    ) {
        // When
        let found = Contract::from_flags(contract, record);

        // Then
        assert_eq!(found, expected);
    }

    #[rstest]
    #[case::off_without_cc(Contract::Off, &["int"], true)]
    #[case::live_with_cc(Contract::Live, &["int", "cc"], true)]
    #[case::live_without_cc(Contract::Live, &["int"], false)]
    #[case::recorded_without_cc(Contract::LiveRecorded, &["quick"], false)]
    fn contract__should_need_the_cc_part_when_live(
        #[case] contract: Contract,
        #[case] parts: &[&str],
        #[case] fits: bool,
    ) {
        // When
        let found = contract.check_parts(parts);

        // Then
        assert_eq!(found.is_ok(), fits, "{found:?}");
    }
}
