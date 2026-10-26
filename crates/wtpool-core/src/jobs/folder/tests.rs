use std::path::PathBuf;

use super::*;
use crate::clock::Timestamp;
use crate::jobs::record::JobState;

fn must_folder() -> (tempfile::TempDir, JobFolder) {
    let dir = tempfile::tempdir().unwrap();
    let folder = JobFolder::create(JobFolder::path_in(dir.path())).unwrap();
    (dir, folder)
}

fn record(id: &JobId, ticket: u64) -> JobRecord {
    JobRecord {
        version: RECORD_VERSION,
        id: id.clone(),
        pid: 7,
        started: None,
        summary: "make".to_owned(),
        label: None,
        cwd: PathBuf::from("/w"),
        ticket,
        state: JobState::Waiting,
        queued: Timestamp::from_seconds(0),
        since: Timestamp::from_seconds(0),
    }
}

#[test]
fn job_folder_existing__should_find_none_until_created() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = JobFolder::path_in(dir.path());

    // When
    let before = JobFolder::existing(path.clone()).unwrap();
    let created = JobFolder::create(path.clone()).unwrap();
    let after = JobFolder::existing(path.clone()).unwrap();

    // Then
    assert!(before.is_none());
    assert_eq!(created.dir(), path);
    assert_eq!(after.map(|folder| folder.dir().to_path_buf()), Some(path));
}

#[cfg(unix)]
#[test]
fn job_folder_create__should_refuse_a_symlink_and_a_folder_others_may_write() {
    use std::os::unix::fs::PermissionsExt;

    // Given
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let open = dir.path().join("open");
    std::fs::create_dir(&open).unwrap();
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o770)).unwrap();

    // When
    let linked = JobFolder::create(link).unwrap_err().to_string();
    let shared = JobFolder::create(open).unwrap_err().to_string();

    // Then
    assert!(linked.ends_with("it is a symlink"), "{linked}");
    assert!(
        shared.ends_with("its group or other users may write to it"),
        "{shared}"
    );
}

#[test]
fn job_folder_liveness__should_follow_the_lock() {
    // Given
    let (_dir, folder) = must_folder();
    let id = JobId::new(7);
    let unknown = JobId::new(8);

    // When
    let lock = folder.create_lock(&id).unwrap();
    let held = folder.liveness(&id);
    drop(lock);
    let freed = folder.liveness(&id);

    // Then
    assert_eq!(
        (held, freed, folder.liveness(&unknown)),
        (Liveness::Live, Liveness::Dead, Liveness::Dead)
    );
}

#[test]
fn job_folder_sweep__should_delete_dead_records_and_their_locks() {
    // Given
    let (_dir, folder) = must_folder();
    let (live, dead, orphan) = (JobId::new(1), JobId::new(2), JobId::new(3));
    let held = folder.create_lock(&live).unwrap();
    drop(folder.create_lock(&dead).unwrap());
    drop(folder.create_lock(&orphan).unwrap());
    folder.write_record(&record(&live, 1)).unwrap();
    folder.write_record(&record(&dead, 2)).unwrap();
    std::fs::write(folder.dir().join("notes.json"), "{}").unwrap();

    // When
    let queue = folder.lock_queue().unwrap();
    let swept: Vec<JobId> = folder
        .sweep(&queue, None)
        .into_iter()
        .map(|r| r.id)
        .collect();

    // Then
    let mut left: Vec<String> = std::fs::read_dir(folder.dir())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    let mut expected = vec![
        format!("{live}.json"),
        format!("{live}.lock"),
        "notes.json".to_owned(),
        "queue.lock".to_owned(),
    ];
    expected.sort();
    assert_eq!(swept, vec![live]);
    assert_eq!(left, expected);
    drop(held);
}

#[test]
fn job_folder_take_ticket__should_count_past_every_ticket() {
    // Given
    let (_dir, folder) = must_folder();
    let queue = folder.lock_queue().unwrap();
    let id = JobId::new(1);

    // When
    let first = folder.take_ticket(&queue, &[]).unwrap();
    let second = folder.take_ticket(&queue, &[]).unwrap();
    let past = folder.take_ticket(&queue, &[record(&id, 40)]).unwrap();

    // Then
    assert_eq!((first, second, past), (1, 2, 41));
}

#[test]
fn job_folder_records__should_read_back_what_it_wrote() {
    // Given
    let (_dir, folder) = must_folder();
    let id = JobId::new(9);
    let written = record(&id, 3);

    // When
    folder.write_record(&written).unwrap();
    let read = folder.records();
    folder.delete(&id);

    // Then
    assert_eq!(read, vec![written]);
    assert!(folder.records().is_empty());
}
