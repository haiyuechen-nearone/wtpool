//! Records, activity folders and pools in temporary folders for the tests of the hook modules.

use std::path::PathBuf;
use std::time::Duration;

use super::record::Record;
use super::store::Activity;
use crate::activity::{Harness, PlacedBy, State};
use crate::clock::Timestamp;

/// 2026-10-20T12:00:00Z.
pub const NOW: Timestamp = Timestamp::from_seconds(1_792_497_600);

/// The time `seconds` after [`NOW`], before it when negative.
pub fn later(seconds: i64) -> Timestamp {
    Timestamp::from_seconds(NOW.seconds() + seconds)
}

/// The record of the working session `session`, placed in `/p/fg` by its folder.
pub fn record(session: &str) -> Record {
    Record {
        version: 1,
        harness: Harness::ClaudeCode,
        session_id: session.to_owned(),
        agent_id: None,
        agent_type: None,
        label: "l".to_owned(),
        worktree: "/p/fg".to_owned(),
        placed_by: PlacedBy::Cwd,
        state: State::Working,
        waiting: None,
        action: None,
        in_flight: Vec::new(),
        done_ids: Vec::new(),
        error: None,
        since: "2026-10-20T12:00:00Z".to_owned(),
        updated: "2026-10-20T12:00:00Z".to_owned(),
        writer_started: 1,
        process: None,
        first_cwd: None,
        ended: None,
        title: None,
        transcript_offset: 0,
        subagents: Vec::new(),
        tasks: Vec::new(),
    }
}

/// `true` once `attempt` succeeds within a second. A process another test spawns holds a copy of
/// every open file until its exec closes them, so a lock just let go may stay held a moment.
pub fn eventually(mut attempt: impl FnMut() -> bool) -> bool {
    const PAUSE: Duration = Duration::from_millis(5);
    (0..200).any(|round| {
        std::thread::sleep(PAUSE * u32::from(round > 0));
        attempt()
    })
}

/// An empty `wtpool-activity/` in a temporary folder.
pub fn must_activity() -> (tempfile::TempDir, Activity) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("wtpool-activity")).unwrap();
    let activity = Activity::open(dir.path()).unwrap();
    (dir, activity)
}

/// A pool without git: `fg` with a `.git` folder, linked worktrees `wt1` and `wt1x` whose
/// `.git` files point at `fg/.git/worktrees/<name>`, and `fake`, whose `.git` file points at
/// wt1's folder without a back link.
pub fn must_pool() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let common = root.join("fg/.git");
    std::fs::create_dir_all(common.join("objects")).unwrap();
    std::fs::write(common.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    for name in ["wt1", "wt1x"] {
        let gitdir = common.join("worktrees").join(name);
        std::fs::create_dir_all(&gitdir).unwrap();
        std::fs::create_dir_all(root.join(name)).unwrap();
        std::fs::write(
            root.join(name).join(".git"),
            format!("gitdir: {}\n", gitdir.display()),
        )
        .unwrap();
        std::fs::write(
            gitdir.join("gitdir"),
            format!("{}\n", root.join(name).join(".git").display()),
        )
        .unwrap();
        std::fs::write(gitdir.join("commondir"), "../..\n").unwrap();
    }
    std::fs::create_dir_all(root.join("fake")).unwrap();
    std::fs::write(
        root.join("fake/.git"),
        format!("gitdir: {}\n", common.join("worktrees/wt1").display()),
    )
    .unwrap();
    (dir, root)
}
