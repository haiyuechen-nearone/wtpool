//! `verify --part a,b,...`: several parts of one run, in parallel where they are independent.
//! The shared builds come first, once: the release `wtpool` and the web bundle and .vsix. Then
//! the waves:
//! 1. `int` alone, first, on an idle machine. Its watch checks time file events, and the file
//!    churn of other parts (quick's coverage rebuild, VS Code profiles) backs up the system's
//!    file event service (fseventsd on macOS).
//! 2. `bench` alone, so nothing skews its timings.
//! 3. `quick` and `cc`, in parallel. `cc` reads the int part's `cc_replay:<name>`.
//! 4. `ui` alone, the only part with VS Code windows besides quick's host layer of wave 3.
//!
//! The shared builds, and the jobs of each part but bench, start at a priority that yields the
//! processor to the owner's work ([`crate::priority`]).
//!
//! Each part runs as its own `xtask verify --part` process, the cc part with the run's
//! `--contract` and `--record`. `--only` names a check of one part, so a part list refuses it.
//! When `WTPOOL_VERIFY_WRAPPER` is set, its words, split at white space, come first: a command
//! prefix such as a machine-wide job queue (`WTPOOL_VERIFY_WRAPPER=~/bin/queue` runs
//! `~/bin/queue <xtask> verify --part int ...`). A leading `~/` names the home folder. Unset or
//! empty, nothing wraps the parts. A single `--part` never reads it.

use std::fs;
use std::path::Path;
use std::process::{Child, Command, ExitCode, Stdio};

use crate::parts::{Contract, Ctx};

const WAVES: [&[&str]; 4] = [&["int"], &["bench"], &["quick", "cc"], &["ui"]];

/// The variable that names the command prefix of each part.
pub const WRAPPER: &str = "WTPOOL_VERIFY_WRAPPER";

/// The words of a wrapper `text`, a leading `~/` of each as `home`.
pub fn wrapper_words(text: &str, home: Option<&Path>) -> Vec<String> {
    text.split_whitespace()
        .map(|word| match (word.strip_prefix("~/"), home) {
            (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
            _ => word.to_owned(),
        })
        .collect()
}

fn wrapper() -> Vec<String> {
    let text = std::env::var(WRAPPER).unwrap_or_default();
    wrapper_words(&text, crate::dirs::home().as_deref())
}

fn spawn(
    root: &Path,
    phase: u8,
    run: &str,
    run_dir: &Path,
    part: &str,
    contract: Contract,
    fresh: bool,
) -> Result<Child, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = match wrapper().split_first() {
        Some((program, args)) => {
            let mut command = Command::new(program);
            command.args(args).arg(&exe);
            command
        }
        None => Command::new(&exe),
    };
    let phase = phase.to_string();
    command
        .current_dir(root)
        .args(["verify", "--phase", &phase, "--run", run, "--part", part]);
    if contract.live() && part == "cc" {
        command.arg("--contract");
        if contract.recorded() {
            command.arg("--record");
        }
    }
    if fresh {
        command.arg("--fresh");
    }
    let logs = run_dir.join("logs");
    fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let out = fs::File::create(logs.join(format!("part-{part}.out"))).map_err(|e| e.to_string())?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    command.stdout(Stdio::from(out)).stderr(Stdio::from(err));
    command
        .spawn()
        .map_err(|e| format!("start the {part} part: {e}"))
}

/// The flags of `verify` for a part list, whose parts run whole.
pub struct ListFlags<'a> {
    pub contract: Contract,
    pub fresh: bool,
    /// `--only`, refused by a part list.
    pub only: Option<&'a str>,
}

/// The parts of the comma list `list`, or the refusal of an unknown part or of a flag the list
/// cannot pass on.
pub fn list_parts<'a>(list: &'a str, flags: &ListFlags) -> Result<Vec<&'a str>, String> {
    let wanted: Vec<&str> = list
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    for part in &wanted {
        if !crate::PARTS.contains(part) {
            return Err(format!("unknown part {part}"));
        }
    }
    if let Some(only) = flags.only {
        return Err(format!(
            "--only {only} goes with one part: a part list runs its parts whole"
        ));
    }
    flags.contract.check_parts(&wanted)?;
    Ok(wanted)
}

/// Runs the parts of the comma list `list` in one run: the shared builds, then the waves.
pub fn run_parts(
    root: &Path,
    phase: u8,
    run: &str,
    run_dir: &Path,
    list: &str,
    flags: &ListFlags,
) -> Result<ExitCode, String> {
    let wanted = list_parts(list, flags)?;
    // The shared builds run once, before any part starts, and hold the machine's verify lock
    // shared as a part does. They release it before the waves, whose bench part holds it alone.
    let turn = crate::machine_lock::turn();
    let ctx = Ctx::new(
        root.to_path_buf(),
        phase,
        run_dir.to_path_buf(),
        crate::repo::head(root)?,
        None,
    )?
    .with_low_priority(true);
    if wanted
        .iter()
        .any(|p| ["int", "ui", "bench", "cc"].contains(p))
        && let Err(error) = crate::layers::release_wtpool(&ctx)
    {
        eprintln!("the shared release build failed, each part retries it: {error}");
    }
    if wanted.iter().any(|p| ["quick", "ui", "bench"].contains(p)) {
        for result in crate::layers::web_build_once(&ctx) {
            if result.outcome != crate::results::Outcome::Pass {
                eprintln!(
                    "the shared web build failed, each part retries it: {}",
                    result.message
                );
            }
        }
    }
    drop(turn);
    let mut failed = false;
    for wave in WAVES {
        let mut children = Vec::new();
        let mut refused = None;
        for part in wave.iter().filter(|p| wanted.contains(p)) {
            match spawn(root, phase, run, run_dir, part, flags.contract, flags.fresh) {
                Ok(child) => children.push((*part, child)),
                Err(error) => {
                    refused = Some(error);
                    break;
                }
            }
        }
        // The parts a failed start leaves running still end before the run does.
        for (part, mut child) in children {
            let text = match child.wait() {
                Ok(status) => {
                    failed |= !status.success();
                    status.to_string()
                }
                Err(error) => {
                    failed = true;
                    format!("cannot wait: {error}")
                }
            };
            let out = fs::read_to_string(run_dir.join("logs").join(format!("part-{part}.out")))
                .unwrap_or_default();
            println!("== {part}: {text}");
            print!("{out}");
        }
        if let Some(error) = refused {
            return Err(error);
        }
    }
    println!("run: {}", run_dir.display());
    Ok(if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn flags(contract: Contract, only: Option<&str>) -> ListFlags<'_> {
        ListFlags {
            contract,
            fresh: false,
            only,
        }
    }

    #[rstest]
    #[case::whole_parts("int,quick", Contract::Off, None, Ok(vec!["int", "quick"]))]
    #[case::recorded_with_cc("int, cc", Contract::LiveRecorded, None, Ok(vec!["int", "cc"]))]
    #[case::unknown("int,lint", Contract::Off, None, Err("unknown part lint"))]
    #[case::only(
        "int,quick",
        Contract::Off,
        Some("x"),
        Err("--only x goes with one part")
    )]
    #[case::contract_without_cc("int,ui", Contract::Live, None, Err("--contract goes"))]
    fn list_parts__should_refuse_the_flags_a_list_cannot_pass_on(
        #[case] list: &str,
        #[case] contract: Contract,
        #[case] only: Option<&str>,
        #[case] expected: Result<Vec<&str>, &str>,
    ) {
        // When
        let found = list_parts(list, &flags(contract, only));

        // Then
        match (found, expected) {
            (Ok(parts), Ok(expected)) => assert_eq!(parts, expected),
            (Err(error), Err(start)) => assert!(error.starts_with(start), "{error}"),
            (found, expected) => panic!("{found:?}, expected {expected:?}"),
        }
    }

    #[rstest]
    #[case::unset("", &[])]
    #[case::program("/opt/queue", &["/opt/queue"])]
    #[case::home("~/bin/queue --slots 2", &["/home/u/bin/queue", "--slots", "2"])]
    #[case::tilde_inside("nice -n 5 ~/x", &["nice", "-n", "5", "/home/u/x"])]
    #[case::other_user("~other/queue", &["~other/queue"])]
    fn wrapper_words__should_split_and_expand_the_home(#[case] text: &str, #[case] words: &[&str]) {
        // Given
        let home = Path::new("/home/u");

        // When
        let found = wrapper_words(text, Some(home));

        // Then
        assert_eq!(found, words);
    }
}
