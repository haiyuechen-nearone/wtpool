#[cfg(unix)]
use rstest::rstest;

use super::*;
use crate::activity::{ActorId, Harness};
use crate::hook::fixture::{self, eventually, must_activity};

#[cfg(unix)]
#[rstest]
#[case::missing(None, false)]
#[case::folder(Some("folder"), true)]
#[case::symlink(Some("symlink"), false)]
#[case::file(Some("file"), false)]
fn open__should_take_only_a_real_folder(#[case] kind: Option<&str>, #[case] opens: bool) {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wtpool-activity");
    match kind {
        Some("folder") => std::fs::create_dir(&path).unwrap(),
        Some("symlink") => std::os::unix::fs::symlink(dir.path(), &path).unwrap(),
        Some(_) => std::fs::write(&path, "x").unwrap(),
        None => {}
    }

    // When
    let activity = Activity::open(dir.path());

    // Then
    assert_eq!(activity.is_some(), opens);
}

#[test]
fn write__should_round_trip_and_leave_no_temp_file() {
    // Given
    let (_dir, activity) = must_activity();
    let record = fixture::record("s1");

    // When
    let lock = activity.lock("s1").unwrap();
    activity.write("s1", &record).unwrap();
    drop(lock);

    // Then
    assert_eq!(activity.read("s1"), Some(record));
    assert_eq!(activity.stems(), vec!["s1".to_owned()]);
    assert!(activity.orphan_locks().is_empty());
    assert!(!activity.is_sandbox());
}

#[cfg(unix)]
#[rstest]
#[case::symlink("symlink")]
#[case::oversize("oversize")]
#[case::fifo("fifo")]
fn write__should_replace_what_it_cannot_read(#[case] kind: &str) {
    // Given
    let (dir, activity) = must_activity();
    let path = activity.record_path("s1");
    let victim = dir.path().join("victim");
    std::fs::write(&victim, "keep").unwrap();
    match kind {
        "symlink" => std::os::unix::fs::symlink(&victim, &path).unwrap(),
        "oversize" => {
            std::fs::write(&path, vec![b' '; crate::activity::MAX_RECORD_BYTES + 1]).unwrap()
        }
        _ => {
            let made = std::process::Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap();
            assert!(made.success());
        }
    }

    // When
    let before = activity.read("s1");
    activity.write("s1", &fixture::record("s1")).unwrap();

    // Then
    assert_eq!(before, None);
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
    assert!(std::fs::symlink_metadata(&path).unwrap().is_file());
}

#[test]
fn write__should_remove_its_temp_file_when_the_rename_fails() {
    // Given
    let (_dir, activity) = must_activity();
    std::fs::create_dir(activity.record_path("s1")).unwrap();
    std::fs::write(activity.record_path("s1").join("x"), "").unwrap();

    // When
    let written = activity.write("s1", &fixture::record("s1"));

    // Then
    assert!(written.is_err());
    let names: Vec<String> = std::fs::read_dir(activity.record_path("s1").parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["s1.json".to_owned()]);
}

#[test]
fn write__should_leave_the_record_when_the_next_cannot_fit() {
    // Given
    let (_dir, activity) = must_activity();
    let record = fixture::record("s1");
    activity.write("s1", &record).unwrap();
    let mut large = fixture::record("s1");
    large.first_cwd = Some("d".repeat(crate::activity::MAX_RECORD_BYTES));

    // When
    let written = activity.write("s1", &large);

    // Then
    assert_eq!(
        written.map_err(|error| error.kind()),
        Err(io::ErrorKind::FileTooLarge)
    );
    assert_eq!(activity.read("s1"), Some(record));
    assert_eq!(activity.stems(), vec!["s1".to_owned()]);
}

#[test]
fn delete__should_remove_the_record_and_its_lock() {
    // Given
    let (_dir, activity) = must_activity();
    activity.write("s1.a1", &fixture::record("s1")).unwrap();
    std::fs::write(activity.record_path("s1").with_extension("json.lock"), "").unwrap();
    let lock = activity.lock("s1.a1").unwrap();

    // When
    let session = ActorKey {
        harness: Harness::ClaudeCode,
        session: ActorId::try_from("s1").unwrap(),
        agent: None,
    };
    let subagents = activity.subagents_of(&session);
    let in_session = activity.session_stems(&session);
    let orphans = activity.orphan_locks();
    activity.delete("s1.a1", lock);

    // Then
    assert_eq!(subagents, vec!["s1.a1".to_owned()]);
    assert_eq!(in_session, vec!["s1.a1".to_owned()]);
    assert_eq!(orphans, vec!["s1".to_owned()]);
    assert!(activity.stems().is_empty());
}

#[cfg(unix)]
#[test]
fn lock__should_refuse_a_lock_that_is_no_file() {
    // Given
    let (_dir, activity) = must_activity();
    let made = std::process::Command::new("mkfifo")
        .arg(activity.record_path("s1").with_extension("json.lock"))
        .status()
        .unwrap();
    assert!(made.success());

    // When
    let lock = activity.lock("s1");

    // Then
    assert!(lock.is_none());
}

#[cfg(unix)]
fn regular_file_mode(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    let meta = path.symlink_metadata().ok()?;
    meta.is_file().then(|| meta.permissions().mode() & 0o777)
}

#[cfg(unix)]
#[rstest]
#[case::missing(None, Some(""), Some(0o600))]
#[case::file(Some("file"), Some("kept"), Some(0o644))]
#[case::symlink(Some("symlink"), Some(""), Some(0o600))]
#[case::fifo(Some("fifo"), Some(""), Some(0o600))]
#[case::folder(Some("folder"), None, None)]
fn open_file__should_open_a_regular_file_and_replace_the_rest(
    #[case] kind: Option<&str>,
    #[case] expected: Option<&str>,
    #[case] mode: Option<u32>,
) {
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    // Given
    let (dir, activity) = must_activity();
    let path = dir.path().join("wtpool-activity/gc.stamp");
    let outside = dir.path().join("outside");
    std::fs::write(&outside, "outside").unwrap();
    match kind {
        Some("file") => {
            std::fs::write(&path, "kept").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        Some("symlink") => std::os::unix::fs::symlink(&outside, &path).unwrap(),
        Some("fifo") => {
            let made = std::process::Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap();
            assert!(made.success());
        }
        Some(_) => std::fs::create_dir(&path).unwrap(),
        None => {}
    }

    // When
    let opened = activity.open_file("gc.stamp").map(|mut file| {
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        text
    });

    // Then
    assert_eq!(opened.as_deref(), expected);
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "outside");
    assert_eq!(regular_file_mode(&path), mode);
}

#[test]
fn lock_now__should_not_wait_for_a_held_lock() {
    // Given
    let (_dir, activity) = must_activity();
    let held = activity.lock("s1").unwrap();

    // When
    let while_held = activity.lock_now("s1");
    drop(held);
    let after = eventually(|| activity.lock_now("s1").is_some());

    // Then
    assert!(while_held.is_none());
    assert!(after);
}
