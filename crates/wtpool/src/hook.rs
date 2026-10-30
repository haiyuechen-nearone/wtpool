//! `wtpool hook`, `wtpool hook policy`, `wtpool hook gc` and `wtpool hook install`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use wtpool_claude_code::ClaudeCode;
use wtpool_core::activity::HarnessName;
use wtpool_core::hook::{self, Adapter, HookEnv};
use wtpool_core::policy::PolicyEnv;
use wtpool_opencode::OpenCode;

use crate::verify_files;

/// The time a hook may run before its watchdog ends it.
const WATCHDOG: Duration = Duration::from_secs(1);

/// The harnesses this binary registers: the only list of the names `--harness` takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harness {
    ClaudeCode,
    OpenCode,
}

impl Harness {
    pub const ALL: [Self; 2] = [Self::ClaudeCode, Self::OpenCode];

    #[must_use]
    pub fn name(self) -> HarnessName {
        match self {
            Self::ClaudeCode => ClaudeCode::HARNESS,
            Self::OpenCode => OpenCode::HARNESS,
        }
    }

    /// The harness `--harness` names, Claude Code without the flag, `None` for a name this
    /// binary does not register.
    #[must_use]
    pub fn named(name: Option<&str>) -> Option<Self> {
        match name {
            None => Some(Self::ClaudeCode),
            Some(name) => Self::ALL
                .into_iter()
                .find(|harness| harness.name().as_str() == name),
        }
    }

    /// Every registered name, as `claude-code|opencode`.
    #[must_use]
    pub fn names() -> String {
        Self::ALL
            .map(|harness| harness.name().to_string())
            .join("|")
    }

    /// The harness's adapter with this process's environment, its install text with the policy
    /// check of shell calls when `policy` is set.
    fn adapter(self, policy: bool) -> Adapters {
        match self {
            Self::ClaudeCode => Adapters::ClaudeCode(ClaudeCode::from_process()),
            Self::OpenCode => Adapters::OpenCode(OpenCode::from_process().with_policy(policy)),
        }
    }
}

/// The adapters `wtpool hook` can run, one variant per harness.
#[derive(Debug)]
enum Adapters {
    ClaudeCode(ClaudeCode),
    OpenCode(OpenCode),
}

impl Adapters {
    fn handle(&self, input: &[u8], env: &HookEnv) {
        match self {
            Self::ClaudeCode(adapter) => hook::handle(adapter, input, env),
            Self::OpenCode(adapter) => hook::handle(adapter, input, env),
        }
    }

    fn judge(&self, input: &[u8], env: &PolicyEnv) -> Option<String> {
        match self {
            Self::ClaudeCode(adapter) => hook::judge_policy(adapter, input, env),
            Self::OpenCode(adapter) => hook::judge_policy(adapter, input, env),
        }
    }

    fn install(&self, binary: &str) -> hook::Printed {
        match self {
            Self::ClaudeCode(adapter) => adapter.install(binary),
            Self::OpenCode(adapter) => adapter.install(binary),
        }
    }

    fn require_note(&self, binary: &str) -> Option<String> {
        match self {
            Self::ClaudeCode(adapter) => adapter.require_note(binary),
            Self::OpenCode(adapter) => adapter.require_note(binary),
        }
    }

    /// The harness's own name for the event of `input`, if it parses.
    #[cfg(feature = "verify")]
    fn event_name(&self, input: &[u8]) -> Option<String> {
        match self {
            Self::ClaudeCode(adapter) => adapter.event_name(input),
            Self::OpenCode(adapter) => adapter.event_name(input),
        }
    }
}

/// `wtpool hook`: reads the payload of the harness `harness` names on stdin and writes its
/// record. Prints nothing and exits 0 on every path, a panic, a hang or a harness this binary
/// does not register included.
pub fn run_hook(harness: Option<&str>) {
    let env = hook_env();
    let log = env.log.clone();
    arm_watchdog(log.clone());
    std::panic::set_hook(Box::new(move |info| {
        let at = info
            .location()
            .map(|l| format!(" at {}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let line = format!("wtpool hook: panicked{at}");
        log.iter().for_each(|dir| hook::log_line(dir, &line));
    }));
    let _ = std::panic::catch_unwind(|| {
        let Some(adapter) = registered(harness, env.log.as_deref()) else {
            return;
        };
        let input = hook::read_input(std::io::stdin().lock());
        verify_panic(&adapter, &input);
        adapter.handle(&input, &env);
    });
}

/// `wtpool hook policy`: reads the payload on stdin and prints the harness's refusal of a heavy
/// shell call outside a job, or nothing. It writes no file but a line in the hook's log for a
/// harness this binary does not register, and every fault, a panic or a hang included, ends it
/// with nothing printed and exit 0, so the call goes on.
pub fn run_policy(harness: Option<&str>) {
    arm_watchdog(None);
    std::panic::set_hook(Box::new(|_| {}));
    let decision = std::panic::catch_unwind(|| {
        let adapter = registered(harness, hook_env().log.as_deref())?;
        let input = hook::read_input(std::io::stdin().lock());
        verify_panic(&adapter, &input);
        let env = PolicyEnv::from_env(|name| std::env::var_os(name))?;
        adapter.judge(&input, &env)
    });
    if let Ok(Some(line)) = decision {
        let mut stdout = std::io::stdout().lock();
        let _ = stdout
            .write_all(line.as_bytes())
            .and_then(|()| stdout.flush());
    }
}

/// The adapter of the harness `harness` names. For a name this binary does not register, `None`
/// and one line in the hook's log at `log`.
fn registered(harness: Option<&str>, log: Option<&Path>) -> Option<Adapters> {
    let found = Harness::named(harness);
    if found.is_none()
        && let (Some(dir), Some(name)) = (log, harness)
    {
        hook::log_line(
            dir,
            &format!(
                "wtpool hook: skipped an event: no harness {name:?} is registered ({})",
                Harness::names()
            ),
        );
    }
    found.map(|harness| harness.adapter(false))
}

/// The note of each registered harness whose hooks refuse no shell call through the wtpool at
/// `binary`, in registry order.
#[must_use]
pub fn require_notes(binary: &str) -> Vec<String> {
    Harness::ALL
        .into_iter()
        .filter_map(|harness| harness.adapter(false).require_note(binary))
        .collect()
}

/// Ends the process with exit 0 once it has run for [`WATCHDOG`], noting that in the hook's log
/// at `log` when it is given.
fn arm_watchdog(log: Option<PathBuf>) {
    std::thread::spawn(move || {
        std::thread::sleep(WATCHDOG);
        if let Some(dir) = &log {
            hook::log_line(dir, "wtpool hook: the watchdog ended a run after 1 s");
        }
        std::process::exit(0);
    });
}

/// Runs `wtpool hook gc` on the pool whose common git directory is `common`, and returns the
/// line it prints.
#[must_use]
pub fn gc_line(common: &Path) -> String {
    let collected = hook::collect_pool(common, &hook_env());
    format!(
        "wtpool hook gc: kept {}, deleted {} records\n",
        collected.kept, collected.deleted
    )
}

/// The reasons `wtpool hook install` prints nothing.
#[derive(Debug, PartialEq, Eq)]
pub enum InstallError {
    /// `--harness` names a harness this binary does not register.
    Unknown(String),
    /// This binary is no file the hooks can run.
    Refused(String),
}

/// `wtpool hook install`: the hook config of the harness `harness` names, Claude Code's block
/// without it, and a note on the hooks the harness runs; for OpenCode the plugin, with the
/// policy check of shell calls when `policy` is set.
///
/// # Errors
///
/// [`InstallError::Unknown`] for a harness this binary does not register, and
/// [`InstallError::Refused`] when this binary cannot be the one the hooks run.
pub fn install_text(harness: Option<&str>, policy: bool) -> Result<hook::Printed, InstallError> {
    let found = Harness::named(harness)
        .ok_or_else(|| InstallError::Unknown(harness.unwrap_or_default().to_owned()))?;
    let binary = this_binary().map_err(InstallError::Refused)?;
    if hook::is_under_target(&binary) {
        return Err(InstallError::Refused(format!(
            "{} is a build under a target folder, which cargo replaces; install wtpool elsewhere \
             (for example ~/.local/bin/wtpool) and run wtpool hook install from there",
            binary.display()
        )));
    }
    Ok(found.adapter(policy).install(&binary.to_string_lossy()))
}

/// The absolute path of this binary, its symlinks resolved.
pub fn this_binary() -> Result<PathBuf, String> {
    std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| format!("cannot tell where this wtpool is: {error}"))
}

/// The inputs of `wtpool hook` and `wtpool hook gc` besides the payload: this process's
/// environment, and the system's clock and process table unless [`verify_files`] names files for
/// them.
fn hook_env() -> HookEnv {
    let (clock, processes) = verify_files();
    HookEnv::from_process(clock, processes)
}

/// In a `verify` build, panics when `WTPOOL_VERIFY_PANIC` names the payload's event.
#[cfg(feature = "verify")]
#[expect(clippy::panic)]
fn verify_panic(adapter: &Adapters, input: &[u8]) {
    let wanted = std::env::var("WTPOOL_VERIFY_PANIC").ok();
    if wanted.is_some() && wanted == adapter.event_name(input) {
        panic!("WTPOOL_VERIFY_PANIC");
    }
}

#[cfg(not(feature = "verify"))]
fn verify_panic(_: &Adapters, _: &[u8]) {}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::default(None, Some(Harness::ClaudeCode))]
    #[case::claude_code(Some("claude-code"), Some(Harness::ClaudeCode))]
    #[case::opencode(Some("opencode"), Some(Harness::OpenCode))]
    #[case::unknown(Some("codex"), None)]
    #[case::upper_case(Some("OpenCode"), None)]
    fn named__should_find_only_registered_harnesses(
        #[case] name: Option<&str>,
        #[case] expected: Option<Harness>,
    ) {
        // When
        let found = Harness::named(name);

        // Then
        assert_eq!(found, expected);
    }

    #[test]
    fn names__should_list_every_registered_harness() {
        // When
        let names = Harness::names();

        // Then
        assert_eq!(names, "claude-code|opencode");
    }

    #[test]
    fn registered__should_log_one_line_for_an_unknown_harness() {
        // Given
        let dir = tempfile::tempdir().unwrap();

        // When
        let found = registered(Some("codex"), Some(dir.path()));
        let known = registered(Some("opencode"), Some(dir.path()));

        // Then
        assert!(found.is_none());
        assert!(matches!(known, Some(Adapters::OpenCode(_))));
        let log = std::fs::read_to_string(dir.path().join("hook.log")).unwrap();
        assert_eq!(
            log,
            "wtpool hook: skipped an event: no harness \"codex\" is registered \
             (claude-code|opencode)\n"
        );
    }

    #[test]
    fn install_text__should_refuse_an_unknown_harness_first() {
        // When
        let refused = install_text(Some("codex"), true);

        // Then
        assert_eq!(refused, Err(InstallError::Unknown("codex".to_owned())));
    }
}
