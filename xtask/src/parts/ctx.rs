//! [`Ctx`], the state the checks of one part invocation share, and the commands they start.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::cmd::{self, Ran};
use crate::isolation::VERIFY_EMAIL;
use crate::results::{CheckResult, FailKind};

/// The longest one part invocation runs: its checks share this deadline.
pub const PART_LIMIT: Duration = Duration::from_secs(3600);

/// The retry and test order variables of the layer runners (vscode/test), passed on to them.
/// Unset, a runner keeps its own retries and order; `0`, the value freeze sets, turns retries off.
/// Any other value would change the outcome of a check, so a run refuses it.
pub const RUNNER_VARIABLES: [&str; 4] = [
    "WTPOOL_VERIFY_RETRIES",
    "WTPOOL_VERIFY_HOST_RETRIES",
    "WTPOOL_VERIFY_HOST_ORDER",
    "WTPOOL_VERIFY_HOST_INVERT",
];

/// The tree, phase, run folder, `--only` filter, deadline and priority that every check of one
/// part invocation shares.
pub struct Ctx {
    pub root: PathBuf,
    pub phase: u8,
    pub run_dir: PathBuf,
    pub commit: String,
    pub only: Option<String>,
    pub deadline: Instant,
    /// True when the jobs this context starts yield the processor to the owner's work
    /// (crate::priority): every part's but bench's.
    pub low_priority: bool,
    /// The checks that run only from a phase after `phase` (crate::trace::later_in): no layer
    /// runs or counts them, so a run of an earlier phase is never judged by a later phase's checks.
    pub later: BTreeSet<String>,
}

impl Ctx {
    /// A context whose deadline is [`PART_LIMIT`] from now, or an error when a
    /// [`RUNNER_VARIABLES`] value is neither unset nor `0`.
    pub fn new(
        root: PathBuf,
        phase: u8,
        run_dir: PathBuf,
        commit: String,
        only: Option<String>,
    ) -> Result<Self, String> {
        if let Some(problem) = runner_variables_problem(|name| std::env::var_os(name)) {
            return Err(problem);
        }
        let later = crate::trace::later_in(&root, phase);
        Ok(Self {
            later,
            root,
            phase,
            run_dir,
            commit,
            only,
            deadline: Instant::now()
                .checked_add(PART_LIMIT)
                .unwrap_or_else(Instant::now),
            low_priority: false,
        })
    }

    /// This context, its jobs started at the lower priority when `low` and the owner allows it.
    pub fn with_low_priority(mut self, low: bool) -> Self {
        self.low_priority = low && crate::priority::wanted();
        self
    }

    fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    pub(crate) fn limit(&self, limit: Duration) -> Duration {
        limit.min(self.remaining())
    }

    /// Whether this invocation runs and counts `check`: `--only` names it or is absent, and it
    /// runs from this phase or an earlier one.
    pub(crate) fn wants(&self, check: &str) -> bool {
        self.counts(check) && self.only.as_deref().is_none_or(|only| only == check)
    }

    /// Whether `check` runs from this phase or an earlier one.
    pub(crate) fn counts(&self, check: &str) -> bool {
        !self.later.contains(check)
    }

    pub fn sandbox(&self) -> PathBuf {
        self.run_dir.join("sandbox")
    }

    /// Every process the harness starts sees a sandbox pool, never the real one.
    pub(crate) fn command(&self, program: &str) -> Command {
        self.command_at(program, self.low_priority)
    }

    /// [`Ctx::command`] at the normal priority whatever this context's, for check processes that
    /// time sleeps and file events: the lower priority of macOS coalesces timers, so a 10 ms sleep
    /// there takes about 35 ms.
    pub(crate) fn normal_priority_command(&self, program: &str) -> Command {
        self.command_at(program, false)
    }

    fn command_at(&self, program: &str, low_priority: bool) -> Command {
        let mut command = crate::priority::command(program, low_priority);
        command
            .current_dir(&self.root)
            .env("WTPOOL_FOREGROUND", self.sandbox().join("pool/fg"))
            .env("WTPOOL_SLOT_DIR", self.sandbox().join("pool"))
            .env("WTPOOL_VERIFY_SANDBOX", "1")
            // Every layer's runner and check reads the phase it verifies from here: a check of
            // an earlier phase that a later phase rewords gates the new expectation on it.
            .env("WTPOOL_VERIFY_PHASE", self.phase.to_string())
            .env("TZ", "UTC")
            // Git names a user from `EMAIL` only where no configuration does, as in the real pool,
            // so a commit a run's process makes there names wtpool-verify.
            .env("EMAIL", VERIFY_EMAIL);
        for name in [
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_BUILD_RUSTFLAGS",
            "RUSTDOCFLAGS",
            "CLIPPY_CONF_DIR",
            "CARGO_TARGET_DIR",
            "CARGO_BUILD_TARGET_DIR",
            "WTPOOL_BIN",
        ] {
            command.env_remove(name);
        }
        command
    }

    pub(crate) fn log(&self, name: &str, ran: &Ran) {
        let _ = fs::create_dir_all(self.run_dir.join("logs"));
        let text = format!(
            "status: {:?}\n--- stdout\n{}\n--- stderr\n{}\n",
            ran.status, ran.stdout, ran.stderr
        );
        let _ = fs::write(self.log_path(name), text);
    }

    /// The log [`Ctx::run_logged`] writes for `name`: `<run>/logs/<name>.log`.
    pub(crate) fn log_path(&self, name: &str) -> PathBuf {
        self.run_dir.join("logs").join(format!("{name}.log"))
    }

    pub(crate) fn run_logged(
        &self,
        name: &str,
        command: &mut Command,
        limit: Duration,
    ) -> Result<Ran, String> {
        let ran = cmd::run(command, self.limit(limit))?;
        self.log(name, &ran);
        Ok(ran)
    }

    pub(crate) fn command_check(
        &self,
        check: &str,
        layer: &str,
        command: &mut Command,
        limit: Duration,
    ) -> CheckResult {
        match self.run_logged(&check.replace(':', "_"), command, limit) {
            Err(error) => CheckResult::error(check, layer, error),
            Ok(ran) if ran.ok() => CheckResult::pass(check, layer, "").with_seconds(ran.seconds),
            Ok(ran) if ran.timed_out() => CheckResult::fail(
                check,
                layer,
                FailKind::Timeout,
                format!("over {} s", limit.as_secs()),
            )
            .with_seconds(ran.seconds),
            Ok(ran) => CheckResult::assertion(check, layer, ran.tail(40)).with_seconds(ran.seconds),
        }
    }
}

/// The refusal of the first [`RUNNER_VARIABLES`] value, read through `var`, that is neither unset
/// nor `0`.
fn runner_variables_problem(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<String> {
    RUNNER_VARIABLES.iter().find_map(|name| {
        let value = var(name)?;
        (value != "0").then(|| {
            format!(
                "{name} is {value:?}: a verify run takes it only unset, for the runners' own \
                 retries and order, or 0, as freeze sets it; unset it"
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn ctx(only: Option<&str>) -> Ctx {
        Ctx {
            root: PathBuf::from("."),
            phase: 3,
            run_dir: PathBuf::from("."),
            commit: String::new(),
            only: only.map(str::to_owned),
            deadline: Instant::now(),
            low_priority: false,
            later: BTreeSet::from(["job__should_x".to_owned()]),
        }
    }

    #[rstest]
    #[case::earlier_check(None, "rows__a", true)]
    #[case::later_check(None, "job__should_x", false)]
    #[case::only_the_earlier_check(Some("rows__a"), "rows__a", true)]
    #[case::only_another_check(Some("rows__b"), "rows__a", false)]
    #[case::only_the_later_check(Some("job__should_x"), "job__should_x", false)]
    fn wants__should_leave_out_later_phases_and_follow_only(
        #[case] only: Option<&str>,
        #[case] check: &str,
        #[case] expected: bool,
    ) {
        // Given
        let ctx = ctx(only);

        // When
        let wanted = ctx.wants(check);

        // Then
        assert_eq!(wanted, expected);
    }

    #[rstest]
    #[case::unset(None, None)]
    #[case::zero(Some("0"), None)]
    #[case::retries(Some("2"), Some("WTPOOL_VERIFY_HOST_ORDER is \"2\""))]
    #[case::reverse(Some("reverse"), Some("WTPOOL_VERIFY_HOST_ORDER is \"reverse\""))]
    #[case::empty(Some(""), Some("WTPOOL_VERIFY_HOST_ORDER is \"\""))]
    fn runner_variables_problem__should_refuse_all_but_unset_and_zero(
        #[case] value: Option<&str>,
        #[case] refusal: Option<&str>,
    ) {
        // Given
        let var = |name: &str| match name {
            "WTPOOL_VERIFY_RETRIES" => Some("0".into()),
            "WTPOOL_VERIFY_HOST_ORDER" => value.map(Into::into),
            _ => None,
        };

        // When
        let found = runner_variables_problem(var);

        // Then
        assert_eq!(
            found.as_deref().and_then(|text| text.split(':').next()),
            refusal
        );
    }
}
