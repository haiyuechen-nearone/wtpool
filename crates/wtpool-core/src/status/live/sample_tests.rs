//! The tests of [`Live`]'s own reads of the process table, under each policy.

use rstest::rstest;

use super::*;
use crate::clock::Timestamp;
use crate::jobs::JobPaths;
use crate::status::fixture::{FixedProcesses, Fixture, https_path, must_fixture};

struct FixedClock;

impl crate::clock::ReadClock for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_seconds(42)
    }
}

fn building(live: &Live<'_, FixedClock, FixedProcesses>) -> bool {
    live.document().pools[0]
        .worktrees
        .iter()
        .any(|worktree| worktree.name == "x" && !worktree.builds.is_empty())
}

/// The job paths of a machine config under `fixture` that sets `enforce`, or none at all.
fn job_paths(fixture: &Fixture, enforce: Option<&str>) -> Option<JobPaths> {
    let enforce = enforce?;
    let config = fixture.root.join("config/wtpool/machine.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, format!("[jobs]\nenforce = \"{enforce}\"\n")).unwrap();
    Some(JobPaths {
        config,
        folder: fixture.root.join("state/wtpool/jobs"),
    })
}

/// What a run of the sample shows: the deadlines and whether `x` builds.
#[derive(Debug, PartialEq, Eq)]
struct Sampled {
    first_due: Option<Duration>,
    building_after_first: bool,
    next_due: Option<Duration>,
    building_just_before: bool,
    building_on_time: bool,
}

#[rstest]
#[case::without_machine_config(None, false)]
#[case::off(Some("off"), false)]
#[case::warn(Some("warn"), true)]
#[case::require(Some("require"), true)]
fn live_tick__should_read_the_process_table_each_second_unless_enforce_is_off(
    #[case] enforce: Option<&str>,
    #[case] samples: bool,
) {
    // Given
    let fixture = must_fixture();
    let processes = FixedProcesses::default();
    let sources = Sources {
        clock: FixedClock,
        processes: processes.clone(),
        machine: "machine".to_owned(),
        jobs: job_paths(&fixture, enforce),
        cores: 1,
        forge_repository: https_path,
    };
    let state = PoolState::load(
        fixture.config(),
        fixture.common.clone(),
        &processes,
        sources.host(),
        https_path,
    )
    .unwrap();
    let mut live = Live::new(state, &sources);
    let start = Instant::now();
    let since_start = |at: Option<Instant>| at.map(|at| at - start);

    // When
    let first_due = since_start(live.deadline(start));
    processes.set(vec![fixture.process(9, "make", "x")]);
    live.tick(start);
    let building_after_first = building(&live);
    let next_due = since_start(live.deadline(start));
    processes.set(Vec::new());
    live.tick(start + PROCESS_SAMPLE - Duration::from_millis(1));
    let building_just_before = building(&live);
    live.tick(start + PROCESS_SAMPLE);
    let building_on_time = building(&live);

    // Then
    let expected = Sampled {
        first_due: samples.then_some(Duration::ZERO),
        building_after_first: samples,
        next_due: samples.then_some(PROCESS_SAMPLE),
        building_just_before: samples,
        building_on_time: false,
    };
    let seen = Sampled {
        first_due,
        building_after_first,
        next_due,
        building_just_before,
        building_on_time,
    };
    assert_eq!(seen, expected);
}
