# Verifier review, phase 0, round 1

- Clone: `~/.cache/wt-verify/clones/phase-0-r1` at the `wt --version` commit
- Spec and freeze: docs/design/ at the round 1 freeze; author diff from spec-base to the round 1 freeze
- Run reviewed: `verify/out/phase0-r1`, verdict PASS
- Canaries: `canaries/` (16 new), results in `canary-run.txt`

All experiments ran in a scratch clone of the `wt --version` commit, with `HOME` set to a temporary folder.

## Blocking

1. **Canary survived: `canaries/suppress-in-target-module.patch`.**
   Wrong check: `lint__no_suppressions`. `xtask/src/suppress.rs:286` skips every folder named
   `target` at any depth. So an `#[allow(clippy::indexing_slicing)]` in `crates/wt/src/target/mod.rs`
   passes. `static:clippy` also passes, so nothing catches it. The same match skips nested
   `target/` folders in `repo_context__should_be_unchanged` (`xtask/src/context.rs:22`) and in
   `outside_remotes` (`xtask/src/isolation.rs:162`). Fix: skip only the root `target/` and
   `xtask/target/`, or skip only paths git ignores.

2. **Canary survived: `canaries/version-always-dirty.patch`.**
   Wrong part: the canary runner. `xtask/src/clone.rs:333` (and `:400` in `freeze`) applies the
   patch without committing it, so every canary check runs on a tree with uncommitted changes. No
   canary can then show the clean tree half of `version__should_list_version_commit_and_features`
   (`xtask/tests/int.rs:47-51`): "only such a build says dirty". The check itself works. With the
   patch applied and `git update-index --assume-unchanged crates/wt/build.rs`, so that git sees a
   clean tree, the check fails on its assertion. Fix: commit the applied patch in the canary clone.
   `commit_freeze` already commits as wt-verify.

3. **Open, from the builder: stale binaries in `target/llvm-cov-target` give false "not covered"
   lines. Confirmed and reproduced.** Wrong check: `cover`. `xtask/src/parts.rs:391` reuses the
   folder. `:404` and `:437` only delete `.profraw` files. `cargo llvm-cov report` (`:355`) then
   reads every instrumented `wt-*` binary in `deps/`, including ones left by builds with another
   feature set, profile or toolchain.
   - The `release-verify-by-default` canary left `deps/wt-5e80aeeec188d409`, because its feature
     set has a different hash.
   - The unchanged, clean commit the `wt --version` commit then gave verdict FAIL: "13 of 20 changed lines". It added
     false "(none)" lines at `crates/wt/src/main.rs:350, 356, 359, 387-390`.
   - After I deleted only that file, the same commit gave PASS again: "13 of 13".

   This happens in the reused freeze and canary clones and in the builder's tree. Fix: run
   `cargo llvm-cov clean --workspace` with the show-env environment before `unit`. The
   cargo-llvm-cov docs prescribe this for show-env use.

## Should fix

1. **Open, from the builder: dirty is decided by `contains("dirty")` on the whole output.**
   See `xtask/tests/int.rs:48`. The spec names version, commit and cargo features, not a dirty
   marker or its word. The check still pins the word, and it reads every line. So
   `dirty: false`, or a feature name that holds "dirty", fails a correct build. Read the marker
   from the commit line only, and report this stricter reading. Not blocking: the `wt --version` commit
   prints `commit <sha> (dirty)` only for dirty builds.
2. **`cover` drops changed files that no lcov file names.** See `xtask/src/cover.rs:102`. The
   report skips lines that have no `DA` entry. `cargo llvm-cov report` (`parts.rs:355`) leaves out
   build scripts unless it gets `--include-build-script`. So the 39 lines of `crates/wt/build.rs`
   are missing from "13 of 13 changed lines" and from the uncovered list. The spec says the report
   lists every changed line that no oracle layer covers. Fix: pass `--include-build-script`, or
   list changed `.rs` files that are absent from both lcov files as "not measured".
3. **`isolation__left_no_trace` can fail on the owner's own work.**
   - `xtask/src/isolation.rs:135` flags any worktree added to `~/code/app` during a part.
   - `:152` flags any new folder in `~/.vscode/extensions/`. VS Code creates one for each automatic
     extension update.

   The spec flags only "a folder the run created" and says the owner's work must not fail the run. This
   matters once parts run for an hour (phase 1 `e2e`, `bench`). Fix: flag worktrees inside the run
   folder only, and extension folders of the product's extension id only.
4. **`must_head` ignores a git failure.** See `xtask/tests/support/mod.rs:33-41`. An empty head
   makes `stdout.contains(&head)` (`xtask/tests/int.rs:44`) true. Check the exit status and fail
   with `setup:`.
5. **A test name does not match its assertion.**
   `trace_file__should_name_every_id_once_with_a_check_or_scenario_owner` (`xtask/src/trace.rs:313`)
   checks only the ID count and the phase bound. It does not check the owners.
6. **Builder: the `version_text` table is not exhaustive.** See `crates/wt/src/main.rs:397-415`.
   Three cases cover three independent branches (commit known, dirty, features empty). The test
   standard asks for exhaustive cases with no overlap. For example, unknown commit plus dirty is
   missing. Low priority.

Note: `crates/wt/build.rs:19-23` maps `CARGO_FEATURE_X_Y` to `x-y`, so a feature named `x_y`
would be reported wrong. No such feature exists today.

## Earlier findings

| Finding | Status |
|---|---|
| `cover` reuses `target/llvm-cov-target` | Open. Reproduced (blocking 3). |
| `contains("dirty")` on the whole output | Open (should fix 1). |
| `stop-frozen-check.patch` caught only through `lint__no_suppressions` | Closed. At the `wt --version` commit, `lint__no_suppressions` passes. The canary is caught with `harness: oracle_unchanged failed` and `harness: version__should_list_version_commit_and_features did not run`. The new `stop-frozen-check-accepted.patch` covers the change with an accepted `oracle-changes.toml` entry, so `oracle_unchanged` passes. It is caught by "did not run" alone, so `trace_complete` catches a stopped check by itself. Suggest it replaces the author's canary. |

No earlier rounds, so there were no surviving canaries to reapply.

## Oracle changes since the freeze

There are none. `git diff` from the round 1 freeze to the `wt --version` commit touches no oracle path or key. `verify/accepted.txt` and
`verify/oracle-changes.toml` are absent. No accepted entries are needed.

## Author diff from spec-base to the round 1 freeze against the spec

- `oracle_unchanged` (`xtask/src/oracle.rs`): matches Oracle files 1 to 5, including the
  `members`, `[workspace.lints]` and `[alias]` keys, and the lock against the freeze commit's
  lock. Canaries caught an edited key, an edited file with a rewritten lock, and an added file.
- `repo_context__should_be_unchanged`: a stricter reading. It also checks nested `CLAUDE.md`,
  `CLAUDE.local.md` and any `.claude/`. The nested `target/` gap is in blocking 1.
- `lint__no_suppressions`: a stricter reading. It covers every `allow`, `expect`, rustfmt skip,
  shadow configuration, member `[lints]` and lowering rustflags, and allows one panic exception.
  Gap: blocking 1.
- `trace_complete`: matches. `verify/trace.toml` matches the spec trace list ID for ID (96 IDs,
  same check names, phases and named IDs).
- `isolation__left_no_trace`: covers every place the spec names. Too broad: should fix 3.
- `release__should_have_no_verify_feature`: matches. Caught a default `verify` feature and a
  `WT_VERIFY_CLOCK` read in release.
- `version__should_list_version_commit_and_features`: tests version, full commit and features.
  The dirty part goes beyond the spec (should fix 1, blocking 2).
- Layers: `static` (fmt, clippy with and without all features, rustdoc, doc links), `unit`,
  `int` and the timeouts match Layers and Outcomes. `cover`: blocking 3 and should fix 2.

## Lines no oracle layer covers

These are in `verify/out/phase0-r1`.

- `crates/wt/src/main.rs:347` (unit only): justified. This is the `unknown` commit branch. It
  runs only when git fails at build time, which no harness build does.
- `crates/wt/src/main.rs:353` (unit only): justified. This is the `none` features branch.
  `release__should_have_no_verify_feature` runs it through the uninstrumented release binary and
  asserts an empty feature list.
- `crates/wt/build.rs:1-39` (not measured, missing from the report): justified as behaviour.
  `int:build` runs the script, and `version__` asserts its commit, dirty state and features. The
  report must still list these lines (should fix 2).

## Code against phase 0 and earlier

Phase 0 has no behaviour ID. Only `wt --version` and the harness apply.
- `wt --version` and `-V` print version, full commit with ` (dirty)`, and features. They exit 0,
  write nothing to stderr and read no configuration. The help text and README are updated.
- `version_text` is pure, has no panics, and declares types in the right order. The comments give
  reasons.
- The `#[expect(non_snake_case)]` and `#![allow(...)]` removals are complete. `#![cfg(test)]` on
  integration test files is a legitimate use of the oracle's `allow-*-in-tests`.
- `build.rs` reruns on every build on purpose, and its comment says why.

## Screenshots

Phase 0 has no `shot` or `e2e` layer and the run holds no screenshots. Nothing to compare.

## Canaries

19 were applied: 3 by the author and 16 new. 17 were caught and 2 survived:
`suppress-in-target-module.patch` and `version-always-dirty.patch`. Phase 0 needs: break
`wt --version` (4 new canaries plus 2 for release), stop a frozen check (2), edit an oracle file
(4), add a `CLAUDE.md` (3). Others: lint 3, isolation 1.

verdict: not clean
