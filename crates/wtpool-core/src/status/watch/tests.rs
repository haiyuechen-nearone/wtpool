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
#[case::head("HEAD", Some(Change::GitDir(PathBuf::from(ROOT))))]
#[case::index("index", Some(Change::GitDir(PathBuf::from(ROOT))))]
#[case::merge_head("MERGE_HEAD", Some(Change::GitDir(PathBuf::from(ROOT))))]
#[case::cherry_pick_head("CHERRY_PICK_HEAD", Some(Change::GitDir(PathBuf::from(ROOT))))]
#[case::revert_head("REVERT_HEAD", Some(Change::GitDir(PathBuf::from(ROOT))))]
#[case::bisect_log("BISECT_LOG", Some(Change::GitDir(PathBuf::from(ROOT))))]
#[case::rebase_merge("rebase-merge", Some(Change::GitDir(PathBuf::from(ROOT))))]
#[case::rebase_apply("rebase-apply", Some(Change::GitDir(PathBuf::from(ROOT))))]
#[case::sequencer("sequencer", Some(Change::GitDir(PathBuf::from(ROOT))))]
#[case::packed_refs("packed-refs", Some(Change::Refs))]
#[case::config("config", Some(Change::Refs))]
#[case::worktrees("worktrees", Some(Change::Worktrees))]
#[case::activity("wtpool-activity", Some(Change::Records))]
#[case::branch("refs/heads/main", Some(Change::Refs))]
#[case::nested_branch("refs/heads/deep/nested/keep", Some(Change::Refs))]
#[case::remote("refs/remotes/origin/feature-a", Some(Change::Refs))]
#[case::ref_lock("refs/heads/main.lock", None)]
#[case::ref_temp("refs/tags/.burst", None)]
#[case::ref_tmp_suffix("refs/tags/burst.tmp", None)]
#[case::refs_alone("refs", None)]
#[case::worktree_folder("worktrees/wt1", Some(Change::Worktrees))]
#[case::worktree_head(
    "worktrees/wt1/HEAD",
    Some(Change::GitDir(PathBuf::from("/repo/.git/worktrees/wt1")))
)]
#[case::worktree_index(
    "worktrees/wt1/index",
    Some(Change::GitDir(PathBuf::from("/repo/.git/worktrees/wt1")))
)]
#[case::worktree_head_lock("worktrees/wt1/HEAD.lock", None)]
#[case::worktree_log("worktrees/wt1/logs/HEAD", None)]
#[case::worktree_other("worktrees/wt1/ORIG_HEAD", None)]
#[case::record(
    "wtpool-activity/s-1.json",
    Some(Change::Record(OsString::from("s-1.json")))
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
    #[case] expected: Option<Change>,
) {
    // Given
    let path = Path::new(ROOT).join(relative);

    // When
    let change = classify(&roots(), &path);

    // Then
    assert_eq!(change, expected);
}

#[rstest]
#[case::canonical_root(
    "/real/repo/.git/HEAD",
    Some(Change::GitDir(PathBuf::from("/real/repo/.git")))
)]
#[case::outside_the_root("/elsewhere/HEAD", None)]
#[case::parent_component("/repo/.git/../.git/HEAD", None)]
fn classify__should_match_either_spelling_of_the_root(
    #[case] path: &str,
    #[case] expected: Option<Change>,
) {
    // When
    let change = classify(&roots(), Path::new(path));

    // Then
    assert_eq!(change, expected);
}

#[test]
fn classify__should_drop_a_folder_under_refs() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::create_dir_all(root.join("refs/heads/deep")).unwrap();

    // When
    let folder = classify(std::slice::from_ref(&root), &root.join("refs/heads/deep"));
    let gone = classify(std::slice::from_ref(&root), &root.join("refs/heads/gone"));

    // Then
    assert_eq!((folder, gone), (None, Some(Change::Refs)));
}

#[test]
fn pending_add__should_merge_changes_from_the_first_one_on() {
    // Given
    let mut pending = Pending::default();
    let first = Instant::now();
    let merge = Duration::from_millis(200);
    let later = first + Duration::from_millis(50);

    // When
    pending.add(Change::GitDir(PathBuf::from("/g")), first);
    pending.add(Change::Refs, first);
    pending.add(Change::Record(OsString::from("s.json")), first);
    pending.add(Change::Records, first);
    pending.add(Change::Clock, first);
    pending.add(Change::Worktrees, later);

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
        }
    );
    pending.add(Change::Refs, first + merge);
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
    assert_eq!(steady, beat + period);
    assert_eq!(behind, beat + Duration::from_secs(35));
    assert_eq!(exact, beat + period * 2);
}
