use std::path::PathBuf;

use super::*;

fn patterns(texts: &[&str]) -> Vec<Pattern> {
    texts
        .iter()
        .map(|text| Pattern::parse(text).unwrap())
        .collect()
}

fn names(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|text| (*text).to_owned()).collect()
}

fn machine_file(dir: &tempfile::TempDir, text: Option<&str>) -> PathBuf {
    let path = dir.path().join("wtpool/machine.toml");
    if let Some(text) = text {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
    }
    path
}

#[test]
fn set_limit__should_set_then_remove_the_key() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = machine_file(&dir, None);

    // When
    let set = set_limit(&path, Limit::new(6));
    let after_set = std::fs::read_to_string(&path).unwrap();
    let reset = set_limit(&path, None);

    // Then
    assert!(set.is_ok() && reset.is_ok());
    assert_eq!(after_set, "[jobs]\nlimit = 6\n");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[jobs]\n");
}

#[test]
fn set_enforce__should_write_the_policy_as_a_string() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = machine_file(&dir, Some("[jobs]\nenforce = \"strict\" # typo\n"));

    // When
    let set = set_enforce(&path, Some(Enforce::Require));

    // Then
    assert!(set.is_ok());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[jobs]\nenforce = \"require\" # typo\n"
    );
}

#[test]
fn edit_machine_heavy__should_add_and_remove_listed_patterns() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = machine_file(&dir, Some("[jobs]\nheavy = [\"make\"]\n"));

    // When
    let added = edit_machine_heavy(&path, &ListChange::Add(patterns(&["cargo build"])));
    let removed = edit_machine_heavy(&path, &ListChange::Remove(patterns(&["make"])));

    // Then
    assert_eq!(added.unwrap(), names(&["make", "cargo build"]));
    assert_eq!(removed.unwrap(), names(&["cargo build"]));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[jobs]\nheavy = [\"cargo build\"]\n"
    );
}

#[test]
fn edit_machine_heavy__should_refuse_a_duplicate_or_an_unlisted_pattern() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let text = "[jobs]\nheavy = [\"make\"]\n";
    let path = machine_file(&dir, Some(text));

    // When
    let twice = edit_machine_heavy(&path, &ListChange::Add(patterns(&["just", "make"])));
    let unlisted = edit_machine_heavy(&path, &ListChange::Remove(patterns(&["just"])));

    // Then
    assert!(matches!(twice.unwrap_err().fault, EditFault::Listed(name) if name == "make"));
    assert!(matches!(unlisted.unwrap_err().fault, EditFault::NotListed(name) if name == "just"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
}

#[test]
fn edit_repository_heavy__should_add_new_patterns_and_the_suggestions_lacking() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wtpool.toml");
    std::fs::write(&path, "slot_count = 3\nbuild_processes = [\"protoc\"]\n").unwrap();
    let change = RepositoryChange::Add {
        patterns: patterns(&["make"]),
        suggested: patterns(&["make", "protoc", "cargo build"]),
    };

    // When
    let added = edit_repository_heavy(dir.path(), &change);

    // Then
    assert_eq!(
        added.unwrap(),
        RepositoryLists {
            heavy: names(&["make", "cargo build"]),
            build_processes: names(&["protoc"]),
        }
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "slot_count = 3\nbuild_processes = [\"protoc\"]\nheavy = [\"make\", \"cargo build\"]\n"
    );
}

#[test]
fn edit_repository_heavy__should_leave_the_file_when_every_suggestion_is_listed() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wtpool.toml");
    std::fs::write(&path, "heavy = [ \"make\" ]\n").unwrap();
    let change = RepositoryChange::Add {
        patterns: Vec::new(),
        suggested: patterns(&["make"]),
    };

    // When
    let added = edit_repository_heavy(dir.path(), &change);

    // Then
    assert!(added.is_ok());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "heavy = [ \"make\" ]\n"
    );
}

#[test]
fn edit_repository_heavy__should_remove_from_heavy_and_build_processes() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wtpool.toml");
    let text = "heavy = [\"make\", \"protoc\"]  # ours\nbuild_processes = [\"protoc\", \"a/b\"]\n";
    std::fs::write(&path, text).unwrap();

    // When
    let removed = edit_repository_heavy(
        dir.path(),
        &RepositoryChange::Remove(names(&["protoc", "a/b"])),
    );

    // Then
    assert_eq!(
        removed.unwrap(),
        RepositoryLists {
            heavy: names(&["make"]),
            build_processes: Vec::new(),
        }
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "heavy = [\"make\"]  # ours\nbuild_processes = []\n"
    );
}

#[test]
fn edit_repository_heavy__should_refuse_a_name_listed_nowhere() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wtpool.toml"), "heavy = [\"make\"]\n").unwrap();

    // When
    let unlisted = edit_repository_heavy(dir.path(), &RepositoryChange::Remove(names(&["just"])));
    let no_pattern =
        edit_repository_heavy(dir.path(), &RepositoryChange::Remove(names(&["a b c d"])));

    // Then
    assert!(matches!(unlisted.unwrap_err().fault, EditFault::NotListed(name) if name == "just"));
    assert!(matches!(
        no_pattern.unwrap_err().fault,
        EditFault::Pattern(_)
    ));
}

#[test]
fn edit_repository_heavy__should_refuse_a_comment_only_in_the_list_it_rewrites() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wtpool.toml");
    let text = "build_processes = [\n  \"protoc\", # ours\n]\nheavy = []\n";
    std::fs::write(&path, text).unwrap();

    // When
    let added = edit_repository_heavy(
        dir.path(),
        &RepositoryChange::Add {
            patterns: patterns(&["make"]),
            suggested: Vec::new(),
        },
    );
    let removed = edit_repository_heavy(dir.path(), &RepositoryChange::Remove(names(&["protoc"])));

    // Then
    assert!(added.is_ok());
    assert!(matches!(
        removed.unwrap_err().fault,
        EditFault::CommentInArray {
            key: "build_processes",
            line: 2
        }
    ));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "build_processes = [\n  \"protoc\", # ours\n]\nheavy = [\"make\"]\n"
    );
}

#[test]
fn edit_machine_heavy__should_lose_no_pattern_of_parallel_writers() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let path = machine_file(&dir, None);
    let written: Vec<String> = (0..16).map(|n| format!("p{n:02}")).collect();

    // When
    std::thread::scope(|scope| {
        for name in &written {
            let path = &path;
            scope.spawn(move || {
                edit_machine_heavy(path, &ListChange::Add(patterns(&[name.as_str()]))).unwrap()
            });
        }
    });

    // Then
    let text = std::fs::read_to_string(&path).unwrap();
    let mut listed = string_list(&text, JOBS, HEAVY).unwrap().values();
    listed.sort();
    assert_eq!(listed, written);
}
