//! `cargo xtask`: one invocation, parsed per command, and the command run.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use xtask::parts::{Contract, Ctx, run_part};
use xtask::report::{Verdict, report};
use xtask::results::Outcome;
use xtask::{clone, repo};

const USAGE: &str = "usage:
  cargo xtask verify --phase N --part quick|int|ui|bench|cc [--only CHECK] [--run NAME] [--contract [--record]] [--fresh]
  cargo xtask verify --phase N --part PART,PART... [--run NAME] [--contract [--record]] [--fresh]
  cargo xtask verify --phase N --report [--run NAME]
  cargo xtask bench --phase N [--only CHECK] [--run NAME]
  cargo xtask freeze --phase N [--dry-run]
  cargo xtask canary --phase N PATCH...
  cargo xtask pool --scenario NAME --dir DIR
  cargo xtask payload --scenario NAME --step N --dir DIR
  cargo xtask scrub";

/// The flags that take the next argument as their value.
const VALUE_FLAGS: [&str; 7] = [
    "--phase",
    "--part",
    "--only",
    "--run",
    "--scenario",
    "--step",
    "--dir",
];

const SWITCHES: [&str; 5] = ["--report", "--contract", "--record", "--fresh", "--dry-run"];

/// One invocation, each command holding only the flags of its usage line.
#[derive(Debug, PartialEq)]
enum Command {
    Verify {
        phase: u8,
        run: Option<String>,
        task: Task,
    },
    Bench {
        phase: u8,
        only: Option<String>,
        run: Option<String>,
    },
    Freeze {
        phase: u8,
        dry_run: bool,
    },
    Canary {
        phase: u8,
        patches: Vec<PathBuf>,
    },
    Pool {
        scenario: String,
        dir: PathBuf,
    },
    Payload {
        scenario: String,
        step: usize,
        dir: PathBuf,
    },
    Scrub,
}

/// The work of `verify`: run a part or a part list, or judge the parts a run holds.
#[derive(Debug, PartialEq)]
enum Task {
    Parts {
        part: String,
        only: Option<String>,
        contract: Contract,
        fresh: bool,
    },
    Report,
}

impl Command {
    fn parse(raw: Vec<String>) -> Result<Self, String> {
        let mut flags = Flags::lex(raw)?;
        let command = match flags.command.as_str() {
            "verify" => {
                let phase = flags.phase()?;
                let run = flags.value("--run");
                if flags.switch("--report") {
                    flags.finish("verify --report")?;
                    return Ok(Self::Verify {
                        phase,
                        run,
                        task: Task::Report,
                    });
                }
                let part = flags
                    .value("--part")
                    .ok_or(format!("--part or --report is required\n{USAGE}"))?;
                let contract =
                    Contract::from_flags(flags.switch("--contract"), flags.switch("--record"))?;
                let task = Task::Parts {
                    part,
                    only: flags.value("--only"),
                    contract,
                    fresh: flags.switch("--fresh"),
                };
                Self::Verify { phase, run, task }
            }
            "bench" => Self::Bench {
                phase: flags.phase()?,
                only: flags.value("--only"),
                run: flags.value("--run"),
            },
            "freeze" => Self::Freeze {
                phase: flags.phase()?,
                dry_run: flags.switch("--dry-run"),
            },
            "canary" => {
                let phase = flags.phase()?;
                let patches: Vec<PathBuf> =
                    flags.positionals.drain(..).map(PathBuf::from).collect();
                if patches.is_empty() {
                    return Err(format!("canary needs a PATCH\n{USAGE}"));
                }
                Self::Canary { phase, patches }
            }
            "pool" => Self::Pool {
                scenario: flags.required("--scenario")?,
                dir: PathBuf::from(flags.required("--dir")?),
            },
            "payload" => {
                let scenario = flags.required("--scenario")?;
                let text = flags.required("--step")?;
                let step = text.parse().map_err(|_| format!("bad step {text}"))?;
                let dir = PathBuf::from(flags.required("--dir")?);
                Self::Payload {
                    scenario,
                    step,
                    dir,
                }
            }
            "scrub" => Self::Scrub,
            other => return Err(format!("unknown command {other}\n{USAGE}")),
        };
        let name = flags.command.clone();
        flags.finish(&name)?;
        Ok(command)
    }
}

/// The flags and positionals of one invocation, taken one by one by its command, so the ones
/// left over are refused.
struct Flags {
    command: String,
    /// Each flag once, in the order first given, holding its last value.
    given: Vec<(String, Option<String>)>,
    positionals: Vec<String>,
}

impl Flags {
    fn lex(raw: Vec<String>) -> Result<Self, String> {
        let mut iter = raw.into_iter();
        let command = iter.next().ok_or(USAGE)?;
        let mut flags = Self {
            command,
            given: Vec::new(),
            positionals: Vec::new(),
        };
        while let Some(arg) = iter.next() {
            let value = if VALUE_FLAGS.contains(&arg.as_str()) {
                Some(iter.next().ok_or(format!("{arg} needs a value"))?)
            } else if SWITCHES.contains(&arg.as_str()) {
                None
            } else if arg.starts_with("--") {
                return Err(format!("unknown option {arg}\n{USAGE}"));
            } else {
                flags.positionals.push(arg);
                continue;
            };
            match flags.given.iter_mut().find(|(flag, _)| *flag == arg) {
                Some(given) => given.1 = value,
                None => flags.given.push((arg, value)),
            }
        }
        Ok(flags)
    }

    fn take(&mut self, flag: &str) -> Option<Option<String>> {
        let index = self.given.iter().position(|(given, _)| given == flag)?;
        Some(self.given.remove(index).1)
    }

    fn value(&mut self, flag: &str) -> Option<String> {
        self.take(flag).flatten()
    }

    fn switch(&mut self, flag: &str) -> bool {
        self.take(flag).is_some()
    }

    fn required(&mut self, flag: &str) -> Result<String, String> {
        self.value(flag)
            .ok_or(format!("{flag} is required\n{USAGE}"))
    }

    fn phase(&mut self) -> Result<u8, String> {
        let text = self.required("--phase")?;
        text.parse().map_err(|_| format!("bad phase {text}"))
    }

    /// Refuses the first flag or positional that `usage` does not take.
    fn finish(self, usage: &str) -> Result<(), String> {
        if let Some((flag, _)) = self.given.first() {
            return Err(format!("{flag} does not go with {usage}\n{USAGE}"));
        }
        if let Some(arg) = self.positionals.first() {
            return Err(format!("unexpected argument {arg} for {usage}\n{USAGE}"));
        }
        Ok(())
    }
}

/// True when `raw` asks for the usage: `--help` or `-h` anywhere, or `help` as the command.
fn wants_help(raw: &[String]) -> bool {
    raw.first().is_some_and(|first| first == "help")
        || raw.iter().any(|arg| arg == "--help" || arg == "-h")
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::from(3)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if wants_help(&raw) {
        println!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    }
    let command = Command::parse(raw)?;
    let root = repo::root();
    match command {
        Command::Verify { phase, run, task } => verify(root, phase, run, task),
        Command::Bench { phase, only, run } => {
            let task = Task::Parts {
                part: "bench".to_owned(),
                only,
                contract: Contract::Off,
                fresh: false,
            };
            verify(root, phase, run, task)
        }
        Command::Freeze { phase, dry_run } => {
            xtask::freeze::freeze(&root, phase, dry_run)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Canary { phase, patches } => {
            let patches: Vec<PathBuf> = patches
                .into_iter()
                .map(|p| if p.is_absolute() { p } else { root.join(p) })
                .collect();
            if clone::canary(&root, phase, &patches)? {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(1))
            }
        }
        Command::Pool { scenario, dir } => {
            let built = xtask::layers::build_pool(&root, &scenario, &dir)?;
            println!("{built}");
            Ok(ExitCode::SUCCESS)
        }
        Command::Payload {
            scenario,
            step,
            dir,
        } => payload(&root, &scenario, step, &dir),
        Command::Scrub => {
            let recordings = xtask::scrub::recordings_dir().ok_or("HOME is not set")?;
            for rel in xtask::scrub::write_fixtures(&root, &recordings)? {
                println!("scrubbed {rel}");
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Prints the payload of a scenario's hook step for the pool `cargo xtask pool` built in `dir`,
/// for the e2e checks, as they run the release `wtpool hook` themselves.
fn payload(root: &Path, name: &str, index: usize, dir: &Path) -> Result<ExitCode, String> {
    let scenario = xtask::scenario::load(root, name)?;
    let Some(xtask::scenario::Step::Hook(step)) = scenario.steps.get(index) else {
        return Err(format!("step {index} of {name} is no hook step"));
    };
    let pool = xtask::pool::Pool::at(dir)?;
    let payload = xtask::hooks::payload(root, step, &pool)?;
    println!("{payload}");
    Ok(ExitCode::SUCCESS)
}

fn verify(root: PathBuf, phase: u8, run: Option<String>, task: Task) -> Result<ExitCode, String> {
    let run = match run {
        Some(run) => run,
        None => repo::run_id(&root)?,
    };
    if run.is_empty() || run.contains('/') || run.starts_with('.') {
        return Err(format!("bad run name {run}"));
    }
    let run_dir = root.join("verify/out").join(&run);
    std::fs::create_dir_all(&run_dir).map_err(|e| format!("{}: {e}", run_dir.display()))?;
    let Task::Parts {
        part,
        only,
        contract,
        fresh,
    } = task
    else {
        let verdict = report(&root, phase, &run, &run_dir)?;
        println!("verdict: {}", verdict.text());
        println!("report: {}", run_dir.join("report.md").display());
        return Ok(match verdict {
            Verdict::Pass => ExitCode::SUCCESS,
            Verdict::Fail => ExitCode::from(1),
            Verdict::NeedsYou => ExitCode::from(2),
        });
    };
    if part.contains(',') {
        let flags = xtask::schedule::ListFlags {
            contract,
            fresh,
            only: only.as_deref(),
        };
        return xtask::schedule::run_parts(&root, phase, &run, &run_dir, &part, &flags);
    }
    let commit = repo::head(&root)?;
    let ctx =
        Ctx::new(root, phase, run_dir.clone(), commit, only)?.with_low_priority(part != "bench");
    let results = run_part(&ctx, &part, contract, fresh)?;
    if let Some(reused) = xtask::results::load_part(&run_dir, &part)?.and_then(|file| file.reused) {
        println!(
            "reused {part} from {} ({}), input hash {}",
            reused.from, reused.commit, reused.hash
        );
    }
    for result in &results {
        let first = result.message.lines().next().unwrap_or_default();
        let flaky = if result.flaky { " (flaky)" } else { "" };
        println!("{:?}{flaky} {} {}", result.outcome, result.check, first);
    }
    println!("run: {}", run_dir.display());
    let failed = results
        .iter()
        .any(|r| matches!(r.outcome, Outcome::Fail | Outcome::NotRun));
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

    fn parse(line: &str) -> Result<Command, String> {
        Command::parse(line.split_whitespace().map(str::to_owned).collect())
    }

    fn parts(part: &str, only: Option<&str>, contract: Contract, fresh: bool) -> Task {
        Task::Parts {
            part: part.to_owned(),
            only: only.map(str::to_owned),
            contract,
            fresh,
        }
    }

    #[rstest]
    #[case::one_part(
        "verify --phase 4 --part quick --only oracle_unchanged",
        Command::Verify { phase: 4, run: None, task: parts("quick", Some("oracle_unchanged"), Contract::Off, false) }
    )]
    #[case::part_list(
        "verify --part int,cc --phase 3 --run mine --contract --record --fresh",
        Command::Verify { phase: 3, run: Some("mine".to_owned()), task: parts("int,cc", None, Contract::LiveRecorded, true) }
    )]
    #[case::report(
        "verify --phase 3 --report --run mine",
        Command::Verify { phase: 3, run: Some("mine".to_owned()), task: Task::Report }
    )]
    #[case::last_value_wins(
        "verify --phase 2 --phase 3 --report",
        Command::Verify { phase: 3, run: None, task: Task::Report }
    )]
    #[case::bench(
        "bench --phase 4 --only budget --run b",
        Command::Bench { phase: 4, only: Some("budget".to_owned()), run: Some("b".to_owned()) }
    )]
    #[case::freeze("freeze --phase 4 --dry-run", Command::Freeze { phase: 4, dry_run: true })]
    #[case::canary(
        "canary --phase 2 a.patch b.patch",
        Command::Canary { phase: 2, patches: vec![PathBuf::from("a.patch"), PathBuf::from("b.patch")] }
    )]
    #[case::pool(
        "pool --scenario idle --dir /tmp/p",
        Command::Pool { scenario: "idle".to_owned(), dir: PathBuf::from("/tmp/p") }
    )]
    #[case::payload(
        "payload --dir /tmp/p --step 2 --scenario idle",
        Command::Payload { scenario: "idle".to_owned(), step: 2, dir: PathBuf::from("/tmp/p") }
    )]
    #[case::scrub("scrub", Command::Scrub)]
    fn parse__should_read_each_usage_line(#[case] line: &str, #[case] expected: Command) {
        // When
        let parsed = parse(line);

        // Then
        assert_eq!(parsed, Ok(expected));
    }

    #[rstest]
    #[case::report_with_part(
        "verify --phase 3 --report --part x",
        "--part does not go with verify --report"
    )]
    #[case::report_with_only(
        "verify --phase 3 --only c --report",
        "--only does not go with verify --report"
    )]
    #[case::bench_with_report("bench --phase 4 --report", "--report does not go with bench")]
    #[case::bench_with_part("bench --phase 4 --part quick", "--part does not go with bench")]
    #[case::freeze_with_run("freeze --phase 4 --run x", "--run does not go with freeze")]
    #[case::scrub_with_phase("scrub --phase 4", "--phase does not go with scrub")]
    #[case::stray_positional(
        "verify --phase 4 --part quick extra",
        "unexpected argument extra for verify"
    )]
    #[case::pool_positional(
        "pool --scenario s --dir d extra",
        "unexpected argument extra for pool"
    )]
    #[case::canary_without_patch("canary --phase 2", "canary needs a PATCH")]
    #[case::no_phase("verify --part quick", "--phase is required")]
    #[case::no_part("verify --phase 4", "--part or --report is required")]
    #[case::record_alone(
        "verify --phase 4 --part cc --record",
        "--record goes only with --contract"
    )]
    #[case::unknown_option("verify --phase 4 --parts quick", "unknown option --parts")]
    #[case::unknown_command("check --phase 4", "unknown command check")]
    #[case::missing_value("verify --phase", "--phase needs a value")]
    #[case::bad_step("payload --scenario s --step x --dir d", "bad step x")]
    fn parse__should_refuse_an_invocation_its_usage_does_not_name(
        #[case] line: &str,
        #[case] message: &str,
    ) {
        // When
        let error = parse(line).expect_err("parsed");

        // Then
        assert!(error.starts_with(message), "{error}");
    }

    #[test]
    fn parse__should_refuse_a_flag_with_the_usage() {
        // When
        let error = parse("bench --phase 4 --fresh").expect_err("parsed");

        // Then
        assert!(error.ends_with(USAGE), "{error}");
    }
}
