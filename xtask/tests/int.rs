//! The `int` layer: checks that run the `verify` build of `wtpool` through its CLI.

#[cfg(test)]
mod support;

#[cfg(test)]
mod checks;

use std::fs;

use support::{
    Sandbox, contains_bytes, must_build_dirty, must_head, must_package, must_rebuild_steps,
    must_release_bin, must_wtpool_bin,
};
use xtask::version_text::{enabled_features, has_token, listed_features};

#[test]
fn version__should_list_version_commit_and_features() {
    // Given
    let package = must_package();
    let head = must_head();
    let dirty = must_build_dirty();
    let rebuilds = must_rebuild_steps();
    for profile in ["debug ", "release "] {
        assert!(
            rebuilds
                .iter()
                .filter(|s| s.step.starts_with(profile))
                .count()
                >= 2,
            "setup: the harness made fewer than 2 {profile}rebuild steps"
        );
    }
    let release = must_release_bin();
    let sandbox = Sandbox::must_new("version");

    // When
    let output = sandbox.run(&must_wtpool_bin(), &["--version"]);
    let released = sandbox.run(&release, &["--version"]);
    let rebuilt: Vec<_> = rebuilds
        .iter()
        .map(|step| sandbox.run(&step.binary, &["--version"]))
        .collect();

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        package.features.contains_key("verify"),
        "the package that builds wtpool declares no `verify` feature"
    );
    assert!(
        output.status.success(),
        "wtpool --version exited {:?}: {stderr}",
        output.status
    );
    assert!(
        stderr.is_empty(),
        "wtpool --version wrote to stderr: {stderr}"
    );
    assert!(
        has_token(&stdout, &package.version),
        "no version {} in {stdout:?}",
        package.version
    );
    let commit_line = stdout.lines().find(|line| line.contains(&head));
    assert!(commit_line.is_some(), "no full commit {head} in {stdout:?}");
    assert_eq!(
        commit_line.is_some_and(marks_dirty),
        dirty,
        "the commit line marks a build of a tree with uncommitted changes, and only such a build, \
         dirty: {stdout:?}"
    );
    assert_eq!(
        listed_features(&stdout),
        Some(enabled_features(&package.features, &["verify"])),
        "the feature list of {stdout:?}"
    );
    let release_stdout = String::from_utf8_lossy(&released.stdout);
    let release_stderr = String::from_utf8_lossy(&released.stderr);
    assert!(
        released.status.success(),
        "wtpool --version of the release build exited {:?}: {release_stderr}",
        released.status
    );
    assert!(
        release_stderr.is_empty(),
        "wtpool --version of the release build wrote to stderr: {release_stderr}"
    );
    assert!(
        has_token(&release_stdout, &package.version),
        "no version {} in the release build's {release_stdout:?}",
        package.version
    );
    let release_commit_line = release_stdout.lines().find(|line| line.contains(&head));
    assert!(
        release_commit_line.is_some(),
        "no full commit {head} in the release build's {release_stdout:?}"
    );
    assert_eq!(
        release_commit_line.is_some_and(marks_dirty),
        dirty,
        "the release build's commit line marks a build of a tree with uncommitted changes, and \
         only such a build, dirty: {release_stdout:?}"
    );
    for (step, output) in rebuilds.iter().zip(&rebuilt) {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "wtpool --version of the {} exited {:?}",
            step.step,
            output.status
        );
        let commit_line = stdout.lines().find(|line| line.contains(&step.commit));
        assert!(
            commit_line.is_some(),
            "the {} in one target folder, at commit {}, names another commit: {stdout:?}",
            step.step,
            step.commit
        );
        assert_eq!(
            commit_line.is_some_and(marks_dirty),
            step.dirty,
            "the {} in one target folder marks the commit line dirty only for a tree with \
             uncommitted changes, here {}: {stdout:?}",
            step.step,
            if step.dirty { "dirty" } else { "clean" }
        );
    }
}

fn marks_dirty(commit_line: &str) -> bool {
    commit_line
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| word == "dirty")
}

#[test]
fn release__should_have_no_verify_feature() {
    // Given
    let package = must_package();
    let binary = must_release_bin();
    let sandbox = Sandbox::must_new("release");

    // When
    let output = sandbox.run(&binary, &["--version"]);

    // Then
    let stdout = String::from_utf8_lossy(&output.stdout);
    let bytes = fs::read(&binary).expect("setup: read the release binary");
    assert!(
        output.status.success(),
        "wtpool --version exited {:?}",
        output.status
    );
    let listed = listed_features(&stdout);
    assert!(
        listed
            .as_ref()
            .is_some_and(|features| !features.contains("verify")),
        "the release feature list of {stdout:?} must exist and leave out verify"
    );
    assert_eq!(
        listed,
        Some(enabled_features(&package.features, &[])),
        "the release feature list of {stdout:?}"
    );
    for hook in [
        "WTPOOL_VERIFY_CLOCK",
        "WTPOOL_VERIFY_PANIC",
        "WTPOOL_VERIFY_PROCS",
    ] {
        assert!(
            !contains_bytes(&bytes, hook),
            "the release binary reads {hook}"
        );
    }
}
