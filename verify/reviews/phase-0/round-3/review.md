# Verifier review, phase 0, round 3

- Clone: `~/.cache/wt-verify/clones/phase-0-r3` at the round 3 freeze (product unchanged since the version build fix)
- Spec and freeze: docs/design/ at the round 3 freeze; author diff from the round 2 freeze to the round 3 freeze
- Run reviewed: `verify/out/phase0-r3`, verdict PASS
- Canaries: `canaries/` (5 new), experiments in `experiments/`, results in `canary-run.txt`

All experiments ran in a scratch clone of the round 3 freeze. `HOME`, `WT_FOREGROUND` and `WT_SLOT_DIR` were
temporary folders. Only `CARGO_HOME`, `RUSTUP_HOME` and `SCCACHE_DIR` pointed at the real ones.
`~/code/app` under the temporary `HOME` was a fake pool: main worktree, slot `app-wt1`, branch
`side`, tag `owner-base`, all commits by a non wt-verify identity and no `user.email` in its
config, as in the real pool. The three isolation patches name that fake pool by absolute path, so
they run only on this machine and only against the fake pool.

## Blocking

1. **Canary survived: `canaries/version-release-drops-commit.patch`.**
   Wrong check: `version__should_list_version_commit_and_features` (`xtask/tests/int.rs:15-85`).
   The patch makes a release build print `commit unknown`: `commit` is `env!("WT_BUILD_COMMIT")`
   only under `cfg!(debug_assertions)`. The README install command (`cargo build --release`,
   README.md:12) builds exactly that binary, so the owner's installed `wt --version` names no commit.
   - `version__` runs only debug builds: the `verify` build and the four rebuild steps
     (`xtask/src/parts.rs:470-523`, all without `--release`).
   - `release__should_have_no_verify_feature` (`xtask/tests/int.rs:94-133`) runs the release
     binary but asserts only the feature list and the absent `WT_VERIFY_*` strings.
   - Fix: in `version__`, also run `WT_RELEASE_BIN --version` and assert the full commit `head`
     and the dirty marker, as for the `verify` build.

2. **Canaries survived: `canaries/isolation-move-pool-head.patch` and
   `canaries/isolation-add-pool-worktree.patch`.**
   Wrong check: `isolation__left_no_trace`, its new part `verify_head_moves`
   (`xtask/src/isolation.rs:20-26, 160-186`, the comparison in `traces`) with the `EMAIL` it relies
   on (`xtask/src/parts.rs:76-78`, `xtask/tests/support/mod.rs:155`).
   - The patches make `wt --version` move the `HEAD` of slot `app-wt1` to an owner commit, or add
     worktree `app-wt9` at an owner commit. The second is the gap oracle change
     `p0r2-own-output-dirs` says it closes. Both pass: "2 part runs left no trace".
   - Cause: the first process to run the patched `wt` is the builder's own `unit` layer.
     `crates/wt/tests/cli.rs:179-197` starts `wt` with `env_clear()` and no `EMAIL`, which the
     builder is free to do. Git then names the passwd identity: the reflog entries read
     `Pool Owner <you@mac.example>` (`canary-run.txt`, last sections), never `verify@wt.invalid`.
     Only `wt` started by the oracle's `Sandbox` carries `EMAIL`. The check sees only processes
     that keep an environment variable, so it covers the oracle's own `int` runs and not the
     builder's tests, where a product bug most likely runs first.
   - Fix: invert the test. Record the owner's configured address (their real `user.email`) at
     snapshot time, and fail on any `HEAD` reflog entry added during the part whose identity is
     not that address. A run's process gets the passwd fallback or `verify@wt.invalid`, and the
     owner's own work names their address. Keep the unit test, and add a case for a process with a cleared environment.

## Should fix

1. **The rebuild depends on a product file: `REBUILD_EDITED_FILE = "README.md"`**
   (`xtask/src/parts.rs:20, 512-517`). README.md is product source, not an oracle file. A correct
   product that moves or renames it fails verify: `experiments/move-readme.patch` (only `git mv
   README.md docs/README.md`) gives `int:rebuild` an error, and `version__` fails with "setup:
   WT_VERIFY_REBUILD_STEPS is not set" (`canary-run.txt`). The verdict is FAIL for a product with
   no defect. Fix: edit a tracked oracle file in the rebuild clone, such as `verify/nextest.toml`.
   The rebuild clone is the harness's own copy.

## Notes

- `experiments/isolation-move-pool-branch.patch`: `wt --version` runs `git branch --force side
  owner-base` in the pool. It passes `isolation__left_no_trace`. The spec fails only a branch or
  tag at a wt-verify commit, so this is in scope as written. But it moves one of the owner's
  branches, which is the same kind of trace as the `HEAD` moves above. Open spec question: should
  the check also read `logs/refs/` with the fix of blocking 2? Every ref git updates writes a reflog entry
  that names the identity.
- `verify_head_moves` and the fix proposed in blocking 2 both need git to name an identity. Today
  `~/code/app/.git/config` has no `user.*` and no `include`, which I checked read only.
- `copy_uncommitted` (`xtask/src/parts.rs:444-464`) repeats the loop of `Clone::prepare`
  (`xtask/src/clone.rs:248-265`). One shared function would keep the two copies the same.
- Release rebuilds with sccache name the new commit after an empty commit, also when sccache could
  serve the object from an earlier target folder (scratch test, 5 builds). The debug rebuild steps
  need no release counterpart for staleness. Blocking 1 is about the release code path.
- A new untracked file is dirty for the harness (`xtask/src/repo.rs:19-32`) and for the product
  (`--untracked-files=normal`), but no rebuild step adds one. I wrote no canary for it, as an
  untracked file changes no binary unless a tracked file changes too.
- The round 2 notes on the `dirty` marker reading, unmeasured files in `cover` and `CARGO_FEATURE_`
  names still apply unchanged.

## Earlier findings

| Round 2 finding | Status |
|---|---|
| Blocking 1, `version-stale-commit.patch` survived | Closed. The round 2 file and the adopted copy are caught: "the rebuild after an empty commit in one target folder ... names another commit". |
| Should fix 1, `vscode/test/node_modules` walked | Closed. `xtask/src/context.rs:22-30` lists it, with a unit case. `suppress.rs:285` uses the same list. My `add-claude-md-in-vscode-test.patch` shows the parent folder is still walked. |

## Oracle changes since the freeze

None. The clone is at the round 3 freeze. The report agrees. Both round 2 entries,
`p0r2-stale-commit` and `p0r2-own-output-dirs`, are accepted in `verify/accepted.txt` and spent
by the freeze: `edit-oracle-under-round-2-entry.patch` edits `parts.rs` under the spent entry and
is caught.

## Author diff from the round 2 freeze to the round 3 freeze against the spec

- `version__should_list_version_commit_and_features`: the four rebuild steps (first build, empty
  commit on a branch, edit, revert) in one target folder test the full commit and the dirty marker
  per build. Matches the spec and closes round 2 blocking 1. It covers no release build:
  blocking 1.
- `repo_context__should_be_unchanged`: `vscode/test/node_modules` skipped. Matches.
- `isolation__left_no_trace`: the `HEAD` reflog part is stricter than the spec's list, which is
  allowed. As built it cannot see the builder's test processes: blocking 2.
- `Ctx::command` and `Sandbox::run` now set `EMAIL`. Harmless, but not sufficient (blocking 2).
- New tests: rstest cases for `traces` (three reflog cases) and
  `verify_head_moves__should_name_only_worktrees_a_run_process_added` use bare Given/When/Then
  and `<sut>__should_<assertion>` names. The `traces` table lacks the case "entry before, none
  after" (a reflog expiry), which should give 0.

## Lines no oracle layer covers

From `verify/out/phase0-r3` and my control run, both "42 of 42".
- `crates/wt/src/main.rs:347` (unit only): justified. The `unknown` commit branch runs only when
  git fails at build time, which no harness build does. Blocking 1 shows a release build can reach
  it in practice, so the fix there also covers it from `int`.
- `crates/wt/src/main.rs:353` (unit only): justified. The `none` features branch.
  `release__should_have_no_verify_feature` runs it through the uninstrumented release binary.

## Code against phase 0 and earlier

Phase 0 has no behaviour ID. Only `wt --version` and the harness apply. The product is unchanged
since the version build fix, which round 2 reviewed: `wt --version` and `-V` print version, full commit,
` (dirty)` and features, exit 0, write nothing to stderr and read no configuration. `build.rs`
reruns on every build (`crates/wt/build.rs:9`), so no commit goes stale. The `version_text`
table is exhaustive with no overlap.

## Screenshots

Phase 0 has no `shot` or `e2e` layer and the run has no screenshots. Nothing to compare.

## Verify run

`verify/out/phase0-r3` says PASS. My control run of the round 3 freeze (quick, int, report) also gives PASS,
with the same two lines in the coverage list.

## Canaries

29 runs: 26 caught, 3 survived. 2 experiments, not counted.
- Round 2 survivor `version-stale-commit.patch`: caught (round 2 file and adopted copy).
- All 23 canaries in `verify/canaries/phase-0/` are caught at the round 3 freeze.
- New: `edit-oracle-under-round-2-entry.patch` and `add-claude-md-in-vscode-test.patch` caught.
  `version-release-drops-commit.patch`, `isolation-move-pool-head.patch` and
  `isolation-add-pool-worktree.patch` survived.
- The new set breaks `wt --version`, edits an oracle file, adds a `CLAUDE.md` and tests isolation.
  The two adopted stop canaries cover "stop a frozen check", so all four phase 0 categories hold.

verdict: not clean
