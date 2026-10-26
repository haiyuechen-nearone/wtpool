use rstest::rstest;

use super::*;

pub(super) const ROOT: &str = "/repo/.git";

pub(super) fn watched() -> Watched {
    Watched {
        roots: roots(),
        clock: Some(PathBuf::from(CLOCK)),
    }
}

pub(super) const CLOCK: &str = "/sandbox/clock.txt";

pub(super) fn roots() -> Vec<PathBuf> {
    vec![PathBuf::from("/real/repo/.git"), PathBuf::from(ROOT)]
}

#[rstest]
#[case::head("HEAD", Some(FolderChange::GitDir(PathBuf::from(ROOT))))]
#[case::index("index", Some(FolderChange::GitDir(PathBuf::from(ROOT))))]
#[case::merge_head("MERGE_HEAD", Some(FolderChange::GitDir(PathBuf::from(ROOT))))]
#[case::cherry_pick_head("CHERRY_PICK_HEAD", Some(FolderChange::GitDir(PathBuf::from(ROOT))))]
#[case::revert_head("REVERT_HEAD", Some(FolderChange::GitDir(PathBuf::from(ROOT))))]
#[case::bisect_log("BISECT_LOG", Some(FolderChange::GitDir(PathBuf::from(ROOT))))]
#[case::rebase_merge("rebase-merge", Some(FolderChange::GitDir(PathBuf::from(ROOT))))]
#[case::rebase_apply("rebase-apply", Some(FolderChange::GitDir(PathBuf::from(ROOT))))]
#[case::sequencer("sequencer", Some(FolderChange::GitDir(PathBuf::from(ROOT))))]
#[case::packed_refs("packed-refs", Some(FolderChange::Refs))]
#[case::config("config", Some(FolderChange::Refs))]
#[case::worktrees("worktrees", Some(FolderChange::Worktrees))]
#[case::activity("wtpool-activity", Some(FolderChange::Records))]
#[case::branch("refs/heads/main", Some(FolderChange::Refs))]
#[case::nested_branch("refs/heads/deep/nested/keep", Some(FolderChange::Refs))]
#[case::remote("refs/remotes/origin/feature-a", Some(FolderChange::Refs))]
#[case::ref_lock("refs/heads/main.lock", None)]
#[case::ref_temp("refs/tags/.burst", None)]
#[case::ref_tmp_suffix("refs/tags/burst.tmp", None)]
#[case::refs_alone("refs", None)]
#[case::worktree_folder("worktrees/wt1", Some(FolderChange::Worktrees))]
#[case::worktree_head(
    "worktrees/wt1/HEAD",
    Some(FolderChange::GitDir(PathBuf::from("/repo/.git/worktrees/wt1")))
)]
#[case::worktree_index(
    "worktrees/wt1/index",
    Some(FolderChange::GitDir(PathBuf::from("/repo/.git/worktrees/wt1")))
)]
#[case::worktree_head_lock("worktrees/wt1/HEAD.lock", None)]
#[case::worktree_log("worktrees/wt1/logs/HEAD", None)]
#[case::worktree_other("worktrees/wt1/ORIG_HEAD", None)]
#[case::record(
    "wtpool-activity/s-1.json",
    Some(FolderChange::Record(OsString::from("s-1.json")))
)]
#[case::record_lock("wtpool-activity/s-1.json.lock", None)]
#[case::record_temp("wtpool-activity/.s-1.json.tmp", None)]
#[case::other_file_in_activity("wtpool-activity/notes.txt", None)]
#[case::gc_stamp("wtpool-activity/gc.stamp", None)]
#[case::deeper_in_activity("wtpool-activity/x/s-1.json", None)]
#[case::head_lock("HEAD.lock", None)]
#[case::packed_refs_lock("packed-refs.lock", None)]
#[case::fetch_head("FETCH_HEAD", None)]
#[case::orig_head("ORIG_HEAD", None)]
#[case::objects("objects/ab/cdef", None)]
#[case::logs("logs/HEAD", None)]
#[case::other_name("wtpool-verify-other-name", None)]
#[case::the_root_itself("", None)]
fn classify__should_keep_only_the_watched_names(
    #[case] relative: &str,
    #[case] expected: Option<FolderChange>,
) {
    // Given
    let path = Path::new(ROOT).join(relative);

    // When
    let change = classify(&roots(), &path, |_| false);

    // Then
    assert_eq!(change, expected);
}

#[rstest]
#[case::canonical_root(
    "/real/repo/.git/HEAD",
    Some(FolderChange::GitDir(PathBuf::from("/real/repo/.git")))
)]
#[case::outside_the_root("/elsewhere/HEAD", None)]
#[case::parent_component("/repo/.git/../.git/HEAD", None)]
fn classify__should_match_either_spelling_of_the_root(
    #[case] path: &str,
    #[case] expected: Option<FolderChange>,
) {
    // When
    let change = classify(&roots(), Path::new(path), |_| false);

    // Then
    assert_eq!(change, expected);
}

#[test]
fn classify__should_drop_a_folder_under_refs() {
    // Given
    let deep = Path::new(ROOT).join("refs/heads/deep");
    let is_dir = |path: &Path| path == deep;

    // When
    let folder = classify(&roots(), &deep, is_dir);
    let gone = classify(&roots(), &Path::new(ROOT).join("refs/heads/gone"), is_dir);

    // Then
    assert_eq!((folder, gone), (None, Some(FolderChange::Refs)));
}

#[test]
fn classify__should_ask_for_a_folder_only_below_refs() {
    // Given
    let asked = std::cell::Cell::new(0);
    let is_dir = |_: &Path| {
        asked.set(asked.get() + 1);
        true
    };

    // When
    let head = classify(&roots(), &Path::new(ROOT).join("HEAD"), is_dir);
    let record = classify(
        &roots(),
        &Path::new(ROOT).join("wtpool-activity/s-1.json"),
        is_dir,
    );

    // Then
    assert!(head.is_some() && record.is_some());
    assert_eq!(asked.get(), 0);
}

#[test]
fn pending_add__should_merge_changes_from_the_first_one_on() {
    // Given
    let mut pending = Pending::default();
    let first = Instant::now();
    let merge = Duration::from_millis(200);
    let later = first + Duration::from_millis(50);
    let job = JobId::parse("7-0badc0de").unwrap();

    // When
    pending.add(FolderChange::GitDir(PathBuf::from("/g")), first);
    pending.add(FolderChange::Refs, first);
    pending.add(FolderChange::Record(OsString::from("s.json")), first);
    pending.add(FolderChange::Records, first);
    pending.add(FolderChange::Clock, first);
    pending.add(FolderChange::Job(job.clone()), first);
    pending.add(FolderChange::AllJobs, first);
    pending.add(FolderChange::MachineConfig, first);
    pending.add(FolderChange::RepositoryConfig, first);
    pending.add(FolderChange::Worktrees, later);

    // Then
    assert_eq!(pending.due(merge), Some(later + merge / 2));
    assert_eq!(
        pending,
        Pending {
            since: Some(first),
            last: Some(later),
            all: false,
            worktrees: true,
            git_dirs: BTreeSet::from([PathBuf::from("/g")]),
            refs: true,
            records: BTreeSet::from([OsString::from("s.json")]),
            rescan_records: true,
            clock: true,
            jobs: BTreeSet::from([job]),
            all_jobs: true,
            machine_config: true,
            repository_config: true,
        }
    );
    pending.add(FolderChange::Refs, first + merge);
    assert_eq!(pending.due(merge), Some(first + merge));
    assert_eq!(Pending::default().due(merge), None);
}

#[test]
fn next_beat__should_keep_a_steady_period_unless_a_whole_one_passed() {
    // Given
    let beat = Instant::now();
    let period = Duration::from_secs(10);

    // When
    let steady = next_beat(beat, period, beat + Duration::from_millis(400));
    let behind = next_beat(beat, period, beat + Duration::from_secs(25));
    let exact = next_beat(beat, period, beat + period);

    // Then
    assert_eq!(steady, Some(beat + period));
    assert_eq!(behind, Some(beat + Duration::from_secs(35)));
    assert_eq!(exact, Some(beat + period * 2));
}
