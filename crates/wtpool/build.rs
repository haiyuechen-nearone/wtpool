//! Records what `wtpool --version` reports: the commit the binary is built from, whether the tree
//! had uncommitted changes, and the enabled cargo features.

use std::process::Command;

fn main() {
    // An edit or a new file anywhere in the repository changes the tree state without touching
    // a file cargo watches. A path that never exists makes cargo run this script on every build.
    println!("cargo::rerun-if-changed=wtpool-version-reads-the-tree-on-every-build");

    let commit = git(&["rev-parse", "--verify", "HEAD"]).unwrap_or_default();
    // The whole tree but the files cargo itself writes there: `cargo install --git` marks its
    // checkout ready with `.cargo-ok` at the top, which would make every such build dirty.
    let dirty = git(&[
        "--no-optional-locks",
        "status",
        "--porcelain",
        "--untracked-files=normal",
        "--",
        ":(top)",
        ":(top,exclude).cargo-ok",
    ])
    .is_some_and(|status| !status.is_empty());
    let mut features: Vec<String> = std::env::vars()
        .filter_map(|(name, _)| {
            name.strip_prefix("CARGO_FEATURE_")
                .map(|feature| feature.to_lowercase().replace('_', "-"))
        })
        .collect();
    features.sort();

    println!("cargo::rustc-env=WTPOOL_BUILD_COMMIT={}", commit.trim());
    println!("cargo::rustc-env=WTPOOL_BUILD_DIRTY={dirty}");
    println!(
        "cargo::rustc-env=WTPOOL_BUILD_FEATURES={}",
        features.join(", ")
    );
}

/// The standard output of a git command run in this package, `None` when it fails.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then_some(output.stdout)
        .and_then(|stdout| String::from_utf8(stdout).ok())
}
