use std::path::PathBuf;
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

#[derive(Default)]
struct Args {
    command: String,
    phase: Option<u8>,
    part: Option<String>,
    only: Option<String>,
    run: Option<String>,
    report: bool,
    contract: bool,
    record: bool,
    fresh: bool,
    dry_run: bool,
    scenario: Option<String>,
    step: Option<usize>,
    dir: Option<PathBuf>,
    patches: Vec<PathBuf>,
}

fn parse(raw: Vec<String>) -> Result<Args, String> {
    let mut args = Args::default();
    let mut iter = raw.into_iter();
    args.command = iter.next().ok_or(USAGE)?;
    while let Some(arg) = iter.next() {
        let mut value = |name: &str| iter.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--phase" => {
                let text = value("--phase")?;
                args.phase = Some(text.parse().map_err(|_| format!("bad phase {text}"))?);
            }
            "--part" => args.part = Some(value("--part")?),
            "--only" => args.only = Some(value("--only")?),
            "--run" => args.run = Some(value("--run")?),
            "--report" => args.report = true,
            "--contract" => args.contract = true,
            "--record" => args.record = true,
            "--fresh" => args.fresh = true,
            "--dry-run" => args.dry_run = true,
            "--scenario" => args.scenario = Some(value("--scenario")?),
            "--step" => {
                let text = value("--step")?;
                args.step = Some(text.parse().map_err(|_| format!("bad step {text}"))?);
            }
            "--dir" => args.dir = Some(PathBuf::from(value("--dir")?)),
            other if other.starts_with("--") => {
                return Err(format!("unknown option {other}\n{USAGE}"));
            }
            other => args.patches.push(PathBuf::from(other)),
        }
    }
    Ok(args)
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
    let args = parse(raw)?;
    if args.command == "pool" {
        return pool(&args);
    }
    if args.command == "payload" {
        return payload(&args);
    }
    if args.command == "scrub" {
        let root = repo::root();
        let recordings = xtask::scrub::recordings_dir().ok_or("HOME is not set")?;
        for rel in xtask::scrub::write_fixtures(&root, &recordings)? {
            println!("scrubbed {rel}");
        }
        return Ok(ExitCode::SUCCESS);
    }
    let phase = args.phase.ok_or(format!("--phase is required\n{USAGE}"))?;
    let root = repo::root();
    match args.command.as_str() {
        "verify" => verify(&args, phase, root),
        "bench" => {
            let mut args = args;
            args.part = Some("bench".to_owned());
            args.report = false;
            verify(&args, phase, root)
        }
        "freeze" => {
            xtask::freeze::freeze(&root, phase, args.dry_run)?;
            Ok(ExitCode::SUCCESS)
        }
        "canary" => {
            let patches: Vec<PathBuf> = args
                .patches
                .iter()
                .map(|p| {
                    if p.is_absolute() {
                        p.clone()
                    } else {
                        root.join(p)
                    }
                })
                .collect();
            if clone::canary(&root, phase, &patches)? {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(1))
            }
        }
        other => Err(format!("unknown command {other}\n{USAGE}")),
    }
}

/// Builds a scenario's sandbox pool for an e2e check and prints its JSON description.
fn pool(args: &Args) -> Result<ExitCode, String> {
    let scenario = args
        .scenario
        .as_deref()
        .ok_or(format!("--scenario is required\n{USAGE}"))?;
    let dir = args
        .dir
        .as_deref()
        .ok_or(format!("--dir is required\n{USAGE}"))?;
    let root = repo::root();
    let built = xtask::layers::build_pool(&root, scenario, dir)?;
    println!("{built}");
    Ok(ExitCode::SUCCESS)
}

/// Prints the payload of a scenario's hook step for the pool `cargo xtask pool` built in `--dir`,
/// for the e2e checks, as they run the release `wtpool hook` themselves.
fn payload(args: &Args) -> Result<ExitCode, String> {
    let name = args
        .scenario
        .as_deref()
        .ok_or(format!("--scenario is required\n{USAGE}"))?;
    let index = args.step.ok_or(format!("--step is required\n{USAGE}"))?;
    let dir = args
        .dir
        .as_deref()
        .ok_or(format!("--dir is required\n{USAGE}"))?;
    let root = repo::root();
    let scenario = xtask::scenario::load(&root, name)?;
    let Some(xtask::scenario::Step::Hook(step)) = scenario.steps.get(index) else {
        return Err(format!("step {index} of {name} is no hook step"));
    };
    let pool = xtask::pool::Pool::at(dir)?;
    let payload = xtask::hooks::payload(&root, step, &pool)?;
    println!("{payload}");
    Ok(ExitCode::SUCCESS)
}

fn verify(args: &Args, phase: u8, root: PathBuf) -> Result<ExitCode, String> {
    let run = match &args.run {
        Some(run) => run.clone(),
        None => repo::run_id(&root)?,
    };
    if run.is_empty() || run.contains('/') || run.starts_with('.') {
        return Err(format!("bad run name {run}"));
    }
    let run_dir = root.join("verify/out").join(&run);
    std::fs::create_dir_all(&run_dir).map_err(|e| format!("{}: {e}", run_dir.display()))?;
    if args.report {
        let verdict = report(&root, phase, &run, &run_dir)?;
        println!("verdict: {}", verdict.text());
        println!("report: {}", run_dir.join("report.md").display());
        return Ok(match verdict {
            Verdict::Pass => ExitCode::SUCCESS,
            Verdict::Fail => ExitCode::from(1),
            Verdict::NeedsYou => ExitCode::from(2),
        });
    }
    let part = args
        .part
        .clone()
        .ok_or(format!("--part or --report is required\n{USAGE}"))?;
    let contract = Contract::from_flags(args.contract, args.record)?;
    if part.contains(',') {
        let flags = xtask::schedule::ListFlags {
            contract,
            fresh: args.fresh,
            only: args.only.as_deref(),
        };
        return xtask::schedule::run_parts(&root, phase, &run, &run_dir, &part, &flags);
    }
    let commit = repo::head(&root)?;
    let ctx = Ctx::new(root, phase, run_dir.clone(), commit, args.only.clone())?
        .with_low_priority(part != "bench");
    let results = run_part(&ctx, &part, contract, args.fresh)?;
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
