//! Where `wtpool hook` writes, and the commands that create `wtpool-activity`. The checks of where
//! it places an actor are in [`placement`].

mod placement;

use std::fs;
use std::path::Path;

use rstest::rstest;
use xtask::pool::{PoolSpec, WorktreeSpec, git_in};

use crate::support::hook::{Hooks, RunOptions, catalog_spec, event, every_fixture};
use crate::support::{Sandbox, must_symlink, must_wtpool_bin, tree_state};

pub(super) use placement::bash_records;

/// The process table copy [`Hooks::command`] makes for each run.
const RUN_TABLE_COPY: &str = "procs-run-";

fn no_activity() -> PoolSpec {
    PoolSpec {
        agents: false,
        ..catalog_spec()
    }
}

#[test]
fn hook__should_not_create_wtpool_activity() {
    // Given
    let hooks = Hooks::must_with("hook-no-activity", &no_activity());
    let activity = hooks.activity();
    let payload = event(&hooks.pool, "UserPromptSubmit", "s1", &hooks.pool.fg);

    // When
    let runs: Vec<String> = every_fixture(&hooks.pool)
        .iter()
        .map(|(_, payload)| hooks.run(payload).describe())
        .collect();
    let created = fs::symlink_metadata(&activity).is_ok();
    fs::create_dir(&activity).expect("setup: create wtpool-activity");
    let _ = hooks.run(&payload);

    // Then
    assert!(!created, "wtpool hook created {}", activity.display());
    assert!(!runs.is_empty(), "setup: no fixtures");
    assert!(
        hooks.record("s1").is_some(),
        "with wtpool-activity/ in place the hook wrote no record"
    );
}

#[rstest]
#[case::symlink(true)]
#[case::regular_file(false)]
fn hook__should_refuse_a_symlinked_wtpool_activity(#[case] symlink: bool) {
    // Given
    let hooks = Hooks::must_with("hook-symlinked-activity", &no_activity());
    let activity = hooks.activity();
    let elsewhere = hooks.sandbox.root.join("real-activity");
    fs::create_dir_all(&elsewhere).expect("setup: create the real folder");
    if symlink {
        must_symlink(&elsewhere, &activity);
    } else {
        fs::write(&activity, "not a folder\n").expect("setup: write a file");
    }
    let payload = event(&hooks.pool, "UserPromptSubmit", "s1", &hooks.pool.fg);

    // When
    let run = hooks.run(&payload);
    let written: Vec<String> = fs::read_dir(&elsewhere)
        .map(|e| {
            e.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    let file_text = (!symlink).then(|| fs::read_to_string(&activity).unwrap_or_default());
    fs::remove_file(&activity).expect("setup: remove the forged wtpool-activity");
    fs::create_dir(&activity).expect("setup: create wtpool-activity");
    let _ = hooks.run(&payload);

    // Then
    assert_eq!(run.code(), Some(0), "wtpool hook: {}", run.describe());
    assert!(
        written.is_empty(),
        "wtpool hook wrote {written:?} through a symlink"
    );
    if let Some(text) = file_text {
        assert_eq!(
            text, "not a folder\n",
            "wtpool hook changed a wtpool-activity file"
        );
    }
    assert!(
        hooks.record("s1").is_some(),
        "with a real wtpool-activity/ the hook wrote no record"
    );
}

#[rstest]
#[case::unset(None)]
#[case::zero(Some("0"))]
#[case::word(Some("true"))]
fn hook__should_skip_a_sandbox_pool_without_the_variable(#[case] value: Option<&str>) {
    // Given
    let hooks = Hooks::must_new("hook-sandbox-variable");
    let payload = event(&hooks.pool, "UserPromptSubmit", "s1", &hooks.pool.fg);
    let mut options = RunOptions::default();
    match value {
        None => options.remove.push("WTPOOL_VERIFY_SANDBOX".to_owned()),
        Some(text) => options
            .envs
            .push(("WTPOOL_VERIFY_SANDBOX".to_owned(), text.into())),
    }

    // When
    let run = hooks.run_with(&payload, &options);
    let skipped = hooks.files();
    let _ = hooks.run(&payload);

    // Then
    assert_eq!(run.code(), Some(0), "wtpool hook: {}", run.describe());
    assert!(
        skipped.is_empty(),
        "with WTPOOL_VERIFY_SANDBOX={value:?} the hook wrote {skipped:?} into a sandbox pool"
    );
    assert!(
        hooks.record("s1").is_some(),
        "with WTPOOL_VERIFY_SANDBOX=1 the hook wrote no record"
    );
}

#[test]
fn hook__should_write_only_inside_wtpool_activity() {
    // Given
    let hooks = Hooks::must_new("hook-only-inside");
    let state = hooks.sandbox.root.join("xdg/state/wtpool");
    let activity = hooks.activity();
    let before = tree_state(&hooks.sandbox.root, Some(RUN_TABLE_COPY));

    // When
    for (_, payload) in every_fixture(&hooks.pool) {
        let _ = hooks.run(&payload);
    }
    let after = tree_state(&hooks.sandbox.root, Some(RUN_TABLE_COPY));

    // Then
    let outside: Vec<String> = after
        .iter()
        .filter(|(path, meta)| before.get(*path) != Some(*meta))
        .map(|(path, _)| path)
        .chain(before.keys().filter(|path| !after.contains_key(*path)))
        .filter(|path| !path.starts_with(&activity) && !path.starts_with(&state))
        .filter(|path| **path != state.parent().map(Path::to_path_buf).unwrap_or_default())
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        outside.is_empty(),
        "wtpool hook changed files outside wtpool-activity/ and its log folder:\n{}",
        outside.join("\n")
    );
    assert!(
        hooks.files().iter().any(|name| name.ends_with(".json")),
        "wtpool hook wrote no record for the fixtures"
    );
}

/// Prepares the pool changing command `case` and returns its arguments.
fn pool_command(case: &str, pool: &xtask::pool::Pool) -> Vec<&'static str> {
    let branch = |name: &str| {
        git_in(&pool.fg, &["branch", name]).unwrap_or_else(|e| panic!("setup: git branch: {e}"));
    };
    match case {
        "setup" => vec!["setup"],
        "bg" => {
            branch("feature-z");
            vec!["bg", "feature-z"]
        }
        "fg" => {
            branch("feature-z");
            vec!["fg", "feature-z"]
        }
        "free" => vec!["free", "wt2"],
        "new" => vec!["new", "wt4"],
        "rm" => vec!["rm", "wt3"],
        other => panic!("setup: unknown case {other}"),
    }
}

#[rstest]
#[case::setup("setup")]
#[case::bg("bg")]
#[case::fg("fg")]
#[case::free("free")]
#[case::new("new")]
#[case::rm("rm")]
fn setup__should_create_wtpool_activity(#[case] case: &str) {
    // Given
    let sandbox = Sandbox::must_new("setup-activity");
    let mut spec = no_activity();
    spec.worktrees.insert(
        "wt2".to_owned(),
        WorktreeSpec {
            branch: Some("feature-a".into()),
            ..WorktreeSpec::default()
        },
    );
    let pool = sandbox.must_pool(&spec);
    let args = pool_command(case, &pool);
    let activity = pool.activity();

    // When
    let output = sandbox.run_in(&must_wtpool_bin(), &args, &pool.fg, &[]);
    let meta = fs::symlink_metadata(&activity);

    // Then
    assert!(
        output.status.success(),
        "wtpool {} exited {:?}: {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        meta.as_ref().is_ok_and(fs::Metadata::is_dir),
        "wtpool {} left no real folder {}: {meta:?}",
        args.join(" "),
        activity.display()
    );
}
