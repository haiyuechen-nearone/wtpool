# Verifier review, phase 0, round 2

- Clone: `~/.cache/wt-verify/clones/phase-0-r2` at the version build fix
- Spec and freeze: docs/design/ at the round 2 freeze; author diff from the round 1 freeze to the round 2 freeze
- Run reviewed: `verify/out/phase0-r2`, verdict PASS
- Canaries: `canaries/` (4 new), results in `canary-run.txt`

All experiments ran in scratch clones of the version build fix. `HOME`, `WT_FOREGROUND` and `WT_SLOT_DIR` were
temporary folders. Only `CARGO_HOME`, `RUSTUP_HOME` and `SCCACHE_DIR` pointed at the real ones.

## Blocking

1. **Canary survived: `canaries/version-stale-commit.patch`.**
   Wrong check: `version__should_list_version_commit_and_features` (`xtask/tests/int.rs:15-60`),
   with the `int` build it reads (`xtask/src/parts.rs:476-490`). The canary changes the
   `rerun-if-changed` line of `crates/wt/build.rs:9` to `build.rs`. The build script then runs
   only when `build.rs` changes. An incremental build after a commit that leaves `crates/wt`
   alone keeps the old commit. The README install command (`cargo build --release`, README.md:12)
   is such a build.
   - Demonstration (`canary-run.txt`, last section): the version build fix as is gives the new commit after an
     empty commit. With the canary, the build after the empty commit `9c1cef2` still prints
     `commit 0d00b0d…`.
   - The check builds once, at the commit under test. The patch itself changes `build.rs`, so the
     canary runner always reruns the script. No run can see the stale commit.
   - Fix: in `int`, clone the repository into the run folder, build `wt` there, add an empty commit
     as wt-verify, build again with the same target folder, and assert the second binary names the
     new commit.

## Should fix

1. **`repo_context__should_be_unchanged` walks `vscode/test/node_modules`.**
   `xtask/src/context.rs:22-29` lists `vscode/node_modules` but not `vscode/test/node_modules`.
   `xtask/src/clone.rs:208` keeps that folder warm, so phase 1 expects it. npm packages that ship
   an `AGENTS.md`, `CLAUDE.md` or `.claude/` then fail a correct build, and the walk reads every
   dependency file. `lint__no_suppressions` skips the folder only because `vscode/test` is an oracle
   path. Fix: add `vscode/test/node_modules` to `OWN_OUTPUT_DIRS`.

## Notes

- The round 1 file `round-1/canaries/suppress-in-target-module.patch` lacks
  `crates/wt/src/target/mod.rs`. The root `.gitignore:4` (`target`) ignores that path, so a plain
  `git diff` leaves the file out. As filed, the patch declares `mod target;` without the module, does
  not build, and is not a valid canary. The adopted copy in `verify/canaries/phase-0/` is complete
  and is caught. My canaries that add files under ignored paths were made with `git add -f`.
- `isolation__left_no_trace` now flags a real pool worktree only when it lies in the run folder or
  sits at a wt-verify commit (`xtask/src/isolation.rs:183-196`). A worktree the run adds to
  `~/code/app` outside the run folder at one of the owner's commits passes. This is the scope
  that `p0-isolation-scope` accepted. Today only an absolute path in the product could do it,
  because every `wt` gets a sandbox `HOME`. It matters again in phase 2, where `wt hook` runs
  under `claude` with the real `HOME`.
- `cover` lists every line of a changed, unmeasured file as "(not measured)" but leaves it out of
  "changed lines" (`xtask/src/cover.rs:98-110`). So such a file never fails `cover`, and the
  verifier must judge it. That matches the spec's "the verifier marks each one".
- The `version__` dirty marker is still the word `dirty` on the commit line, a stricter reading
  than the spec's "version, commit, cargo features". Round 1 asked to report it.
- `crates/wt/build.rs:22` maps `CARGO_FEATURE_X_Y` to `x-y`. No feature has an underscore.

## Earlier findings

| Round 1 finding | Status |
|---|---|
| Blocking 1, nested `target/` skipped | Closed. `context.rs:22-35` and `suppress.rs:285` skip only the root output folders. The adopted canary is caught, and so is my `add-claude-md-in-nested-target.patch`. |
| Blocking 2, canaries ran on a dirty tree | Closed. `clone.rs:300-333` commits the patch unless it says `Tree: uncommitted`. `version-always-dirty.patch` (round 1 file and adopted copy) is caught. |
| Blocking 3, stale binaries in `target/llvm-cov-target` | Closed. Reproduced the round 1 case: a full `int` run with `release-verify-by-default` left `deps/wt-5e80aeeec188d409`. The next clean run of the version build fix removed it (`parts.rs:405-416`) and reported PASS, "42 of 42", only `main.rs:347` and `:353`. |
| Should fix 1, `contains("dirty")` | Closed. `int.rs:43-54` reads the marker on the commit line only. |
| Should fix 2, build scripts not measured | Closed. `--include-build-script` (`parts.rs:355`). `build.rs` lines appear in both lcov files. |
| Should fix 3, isolation too broad | Closed, with the residual gap in Notes. |
| Should fix 4, `must_head` | Closed. `support/mod.rs:33-49` checks the status and the hash form. |
| Should fix 5, trace test name | Closed. `trace.rs:334-373` checks owners, named IDs and phases. |
| Should fix 6, `version_text` table | Closed. `main.rs:396-444` has all 8 cases. |

## Oracle changes since the freeze

None. `git diff` from the round 2 freeze to the version build fix touches only `crates/wt/build.rs` and `crates/wt/src/main.rs`.
The report agrees. No entry is needed.

The freeze itself also changed `xtask/src/oracle.rs` and `xtask/src/report.rs`, outside the
accepted entries' paths. The phase 0 author may change phase 0 files, so this is in order. The
`oracle.rs` change closes a hole: without it, an accepted entry would cover its paths forever.

## Author diff from the round 1 freeze to the round 2 freeze against the spec

- `oracle_unchanged`: entries the freeze commit already held are spent (`oracle.rs:167-180,
  279-290`). This is a stricter reading of "Later an oracle file changes only through an entry",
  and it is correct. Caught: an edit under a spent entry, and my edit under a new entry that reuses
  the spent id `p0-nested-target`.
- `repo_context__should_be_unchanged`, `lint__no_suppressions`, `outside_remotes`: walk every
  folder but the root output folders. Matches. Gap: should fix 1.
- Canary runner: commits the patch by default and cleans ignored files between canaries
  (`clone.rs:192-209`). Matches "as in the lead's clone".
- `cover`: cleans the instrumented workspace binaries before `unit`, measures build scripts, and
  lists unmeasured files. Matches the Test standards' report rule.
- `isolation__left_no_trace`: matches the accepted scope. Residual gap in Notes.
- `version__should_list_version_commit_and_features`: version, full commit and features are right.
  It cannot see a stale commit: blocking 1.
- `trace_file__…` unit test: its `CATALOG_COVERS` equals the catalog's covers plus R3 and U1, and
  `spec_phase` equals the Trace list's phase rule.
- New oracle unit tests use rstest tables, `<sut>__should_<assertion>` names and bare
  Given/When/Then.

## Lines no oracle layer covers

These are in `verify/out/phase0-r2` and in my rerun.

- `crates/wt/src/main.rs:347` (unit only): justified. This is the `unknown` commit branch. It runs
  only when git fails at build time, which no harness build does.
- `crates/wt/src/main.rs:353` (unit only): justified. This is the `none` features branch.
  `release__should_have_no_verify_feature` runs it through the uninstrumented release binary and
  asserts an empty feature list.

## Code against phase 0 and earlier

Phase 0 has no behaviour ID. Only `wt --version` and the harness apply.
- `wt --version` and `-V` print version, full commit, ` (dirty)` for dirty builds, and features.
  They exit 0, write nothing to stderr and read no configuration. The help text and README name it.
- The version build fix only rewrites `git()` in `build.rs` and extends the `version_text` table. Both are
  correct. `version_text` is pure and has no panics.
- Builder tests follow the test standards: the `version_text` table is exhaustive with no overlap,
  and `version__should_print_the_build_without_reading_the_configuration` covers both flags.

## Screenshots

Phase 0 has no `shot` or `e2e` layer, and the run has no screenshots. Nothing to compare.

## Verify run

`verify/out/phase0-r2` says PASS. My own run of the version build fix (quick, int, report) also gives PASS.

## Canaries

27 runs, 26 caught, 1 survived (`version-stale-commit.patch`).
- Round 1 survivors: `version-always-dirty.patch` caught (round 1 file and adopted copy).
  `suppress-in-target-module.patch`: the adopted copy is caught; the round 1 file is incomplete
  (Notes).
- All 19 canaries in `verify/canaries/phase-0/` are caught at the version build fix.
- New: `add-claude-md-in-nested-target.patch`, `edit-oracle-reusing-spent-id.patch`,
  `stop-check-by-renaming-the-binary.patch` caught; `version-stale-commit.patch` survived.
- Phase 0 categories: break `wt --version` (7), stop a frozen check (2), edit an oracle file (6),
  add a `CLAUDE.md` (4), others 6.

verdict: not clean
