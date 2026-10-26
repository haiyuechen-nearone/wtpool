# Code review, phase 0, round 7 (steps 4 and 5)

- Clone: `~/.cache/wt-verify/clones/phase-0-r7` at the round 7 freeze, clean tree.
- Spec: docs/design/ at the round 7 freeze.
- Run reviewed: `verify/out/phase0-r7`, verdict PASS.
- Product diff: from spec-base to the round 7 freeze under `crates/` and `README.md`. The product commits are
  the `wt --version` commit and the version build fix. No product file changed after the version build fix, so this is the code that rounds 2 to
  6 also reviewed.
- Scope: steps 4 and 5 of prompt B. Canaries (steps 1 and 3) and the oracle review (step 2) are
  reviewed elsewhere.

## Blocking

None.

## Should fix (non blocking)

1. **The build script panics when the environment holds a variable that is not UTF 8**
   (`crates/wt/build.rs:19`). `std::env::vars()` panics while it iterates over such a variable.
   Reproduced in a scratch target folder outside the clone: `env $'WT_REVIEW_BAD=\xff' cargo build
   -p wt` fails with "failed to run custom build command for `wt`" and
   "called `Result::unwrap()` on an `Err` value: "\xFF"". The panic happens at build time only, and
   such variables are rare on macOS, so this does not block. Fix: read
   `CARGO_CFG_FEATURE` (see 2), or use `std::env::vars_os()`.
2. **The feature list also takes `CARGO_FEATURE_*` variables from the caller's environment**
   (`crates/wt/build.rs:19-24`). Cargo sets `CARGO_FEATURE_<name>` for each enabled feature, but it
   does not remove ones the shell already exported. Reproduced: `CARGO_FEATURE_NOT_A_FEATURE=1
   cargo build -p wt` gives a binary whose `wt --version` prints `features: not-a-feature`. The
   mapping also cannot restore a feature name that has an underscore. Fix: read
   `CARGO_CFG_FEATURE`, the comma separated list of enabled features that Cargo 1.85 and later
   give to build scripts (`rust-version` is 1.90). That also fixes item 1.

## Notes (style only)

- `crates/wt/src/main.rs:274-277`: the `--version` arm prints and returns inside the `match` that
  builds `action`. An `Action::Version` variant, handled like `Action::Help` at
  `main.rs:281-284`, would keep one place for output.
- `crates/wt/src/main.rs:336`: `Build<'a>` only ever holds `'static` strings, the test table
  included. `&'static str` fields would drop the lifetime parameter.

## Code against each ID of phase 0 or earlier

Phase 0 has no behaviour ID. All IDs are phase 1 or later (verification.md, Trace list). Phase 0
is `wt --version` (version, commit, cargo features) and the harness.

- `wt --version` and `-V` print three lines: `wt <version>`, `commit <sha>` (with ` (dirty)` for a
  changed tree, `unknown` when git failed) and `features: <list>` (`none` when empty)
  (`main.rs:327-361`). It runs before the configuration is read (`main.rs:274-277`, before
  `Settings::load` at `main.rs:289`). This matches status-panel.md, Phases, row 0.
- `build.rs` reads the commit and the dirty state with git, and runs on every build through a
  `rerun-if-changed` path that never exists (`build.rs:7-9`). `git status` uses
  `--no-optional-locks`, so the build writes no index. `verify/out/` is ignored
  (`verify/.gitignore:1`), so a verify run does not make the build dirty. The run's rebuild steps
  (`verify/out/phase0-r7/rebuild/steps.json`) show all 8 debug and release states with the right
  commit and dirty marker.
- The `verify` feature exists (`crates/wt/Cargo.toml:9-11`) and the release build leaves it out
  (`release__should_have_no_verify_feature` passes).
- README (`README.md:27`, `README.md:92`) and the help text (`main.rs:21`) name `wt --version`.
  `docs/` holds only `docs/design/`, which is an oracle file. No live doc is stale.
- The removed `#[expect(non_snake_case)]` (12 test modules) and `#![allow(...)]` (3 integration
  test files) now rely on the oracle's `non_snake_case = "allow"` workspace lint and the clippy
  `allow-*-in-tests` settings. The `#![cfg(test)]` that replaces them in `crates/wt/tests/cli.rs:3`,
  `crates/wt-core/tests/pool.rs:3` and `crates/wt-core/tests/settings.rs:3` is not a suppression,
  and its comment states the reason. `static:clippy` passes, which shows the comment is true.

## Lines no oracle layer covers

From the coverage section of `verify/out/phase0-r7/report.md`. I also checked both lcov files:
no line of `crates/wt/build.rs` and no changed line of `crates/wt/src/main.rs` other than these
two has a zero count in both layers.

- `crates/wt/src/main.rs:347` (unit only): justified. The `unknown` commit text runs only when git
  fails at build time, and no harness build makes git fail.
- `crates/wt/src/main.rs:353` (unit only): justified. The `none` features text runs only in a build
  without features. The `int` layer builds with `verify`, and the release binary that
  `release__should_have_no_verify_feature` runs has no coverage instrumentation.

## Tests against the test standards

- `version_text__should_list_version_commit_state_and_features` (`main.rs:396-464`): an `rstest`
  table of all 8 cases of three two valued inputs (known or unknown commit, clean or dirty, features
  or none). The table is exhaustive and has no overlap. The name follows `<sut>__should_<assertion>`.
  The body has bare `// Given`, `// When`, `// Then`.
- `version__should_print_the_build_without_reading_the_configuration`
  (`crates/wt/tests/cli.rs:278-301`): an `rstest` table of the two flag spellings, exhaustive, no
  overlap, standard name and sections. It sets an invalid `WT_SLOT_COUNT` to show that the
  configuration is not read.
- New code has no `unwrap`, `expect` or panic outside tests, except `std::env::vars()` in the
  build script (should fix 1). `version_text` is pure, and the I/O stays in `run` and `build.rs`.
  Types are declared before the types they reference (`BUILD` before `Build`). Comments state a
  reason or a contract only.

## Screenshots against the normative mockup parts (step 5)

Phase 0 has no normative mockup part to check: phase 0 has no `shot` or `e2e` layer, and the
visual review starts in phase 1 (verification.md, Canaries). The run has no screenshots. The only
PNG files under `verify/out/phase0-r7` are the mockups copied into `rebuild/repo/docs/`. Nothing to
compare.

verdict: clean
