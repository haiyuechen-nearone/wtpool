//! `wtpool hook`, `wtpool hook gc` and `wtpool hook install`.

use std::path::Path;
use std::time::Duration;

use wtpool_claude_code::ClaudeCode;
use wtpool_core::hook::{self, Adapter as _, HookEnv};

use crate::verify_files;

/// The time a hook may run before its watchdog ends it.
const WATCHDOG: Duration = Duration::from_secs(1);

/// An adapter `wtpool hook` can run, one variant per harness.
#[derive(Debug)]
enum Adapter {
    ClaudeCode(ClaudeCode),
}

impl Adapter {
    fn from_process() -> Self {
        Self::ClaudeCode(ClaudeCode::from_process())
    }

    fn handle(&self, input: &[u8], env: &HookEnv) {
        match self {
            Self::ClaudeCode(claude_code) => hook::handle(claude_code, input, env),
        }
    }

    fn install(&self, binary: &str) -> hook::Printed {
        match self {
            Self::ClaudeCode(claude_code) => claude_code.install(binary),
        }
    }

    /// The harness's own name for the event of `input`, if it parses.
    #[cfg(feature = "verify")]
    fn event_name(&self, input: &[u8]) -> Option<String> {
        match self {
            Self::ClaudeCode(claude_code) => claude_code.event_name(input),
        }
    }
}

/// `wtpool hook`: reads the payload on stdin and writes its record. Prints nothing and exits 0 on
/// every path, a panic or a hang included.
pub fn run_hook() {
    let env = hook_env();
    let log = env.log.clone();
    let watchdog_log = log.clone();
    std::thread::spawn(move || {
        std::thread::sleep(WATCHDOG);
        if let Some(dir) = &watchdog_log {
            hook::log_line(dir, "wtpool hook: the watchdog ended a run after 1 s");
        }
        std::process::exit(0);
    });
    std::panic::set_hook(Box::new(move |info| {
        let at = info
            .location()
            .map(|l| format!(" at {}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let line = format!("wtpool hook: panicked{at}");
        log.iter().for_each(|dir| hook::log_line(dir, &line));
    }));
    let _ = std::panic::catch_unwind(|| {
        let adapter = Adapter::from_process();
        let input = hook::read_input(std::io::stdin().lock());
        verify_panic(&adapter, &input);
        adapter.handle(&input, &env);
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

/// `wtpool hook install`: the hook block for Claude Code's settings and a note on the hooks they
/// run, or the reason this binary cannot be the one the hooks run.
pub fn install_text() -> Result<hook::Printed, String> {
    let binary = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| format!("cannot tell where this wtpool is: {error}"))?;
    if hook::is_under_target(&binary) {
        return Err(format!(
            "{} is a build under a target folder, which cargo replaces; install wtpool elsewhere \
             (for example ~/.local/bin/wtpool) and run wtpool hook install from there",
            binary.display()
        ));
    }
    Ok(Adapter::from_process().install(&binary.to_string_lossy()))
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
fn verify_panic(adapter: &Adapter, input: &[u8]) {
    let wanted = std::env::var("WTPOOL_VERIFY_PANIC").ok();
    if wanted.is_some() && wanted == adapter.event_name(input) {
        panic!("WTPOOL_VERIFY_PANIC");
    }
}

#[cfg(not(feature = "verify"))]
fn verify_panic(_: &Adapter, _: &[u8]) {}
