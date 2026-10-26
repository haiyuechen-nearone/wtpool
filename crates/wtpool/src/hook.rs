//! `wtpool hook`, `wtpool hook gc` and `wtpool hook install` (status-panel.md, section 1).

use std::path::{Path, PathBuf};
use std::time::Duration;

use wtpool_claude_code::ClaudeCode;
use wtpool_core::hook::{self, Adapter as _, HookEnv};

use crate::verify_files;

/// How long a hook may run before its watchdog ends it (H4).
const WATCHDOG: Duration = Duration::from_secs(1);

/// The harness adapters of this wtpool. `wtpool hook` reads Claude Code's. `--harness <name>`
/// is reserved for another (status-panel.md, section 1).
#[derive(Debug)]
enum Adapter {
    ClaudeCode(ClaudeCode),
}

impl Adapter {
    /// The adapter `wtpool hook` runs without `--harness`, with this process's environment.
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

/// `wtpool hook`: reads the payload on stdin and writes its record. Prints nothing and always
/// returns, so the process exits 0, panics included (H2, H3, H4).
pub fn run_hook() {
    let env = hook_env();
    let log = hook::log_dir(
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .as_deref(),
        env.home.as_deref(),
    );
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

/// `wtpool hook gc` on the pool whose common git directory is `common`: what it kept and deleted.
#[must_use]
pub fn gc_line(common: &Path) -> String {
    let collected = hook::collect_pool(common, &hook_env());
    format!(
        "wtpool hook gc: kept {}, deleted {} records\n",
        collected.kept, collected.deleted
    )
}

/// `wtpool hook install`: the block to paste into Claude Code's `settings.json`, in
/// `$CLAUDE_CONFIG_DIR` or `~/.claude` (A11), and whether it is there, or why this binary cannot
/// be the one hooks run.
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

/// The hook's clock and process table: the system's, or the [`verify_files`].
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
