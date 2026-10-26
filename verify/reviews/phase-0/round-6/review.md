# Verifier review, phase 0, round 6

- Clone: `~/.cache/wt-verify/clones/phase-0-r6` at the round 6 freeze (product unchanged since the version build fix)
- Spec and freeze: docs/design/ at the round 6 freeze. Author diff: from the round 5 freeze to the round 6 freeze
- Run reviewed: `verify/out/phase0-r6`, verdict PASS
- Canaries: `canaries/` (7 new). Runner and helpers: `experiments/`. Results: `canary-run.txt`

All canary runs used 4 scratch clones of the round 6 freeze. Each job had its own temporary `HOME`, so it
also had its own canary clone. Each target folder was copied with `cp -c -R` from the lead clone.
`WT_FOREGROUND` and `WT_SLOT_DIR` were also temporary. Only `CARGO_HOME`, `RUSTUP_HOME` and
`SCCACHE_DIR` were the real ones (`experiments/env.sh`). Each isolation canary ran against a fake
pool that `experiments/mkpool.sh` built under its temporary `HOME`, with slot `app-wt1` holding an
untracked file. No run touched the real pool. No process of these runs is left.

## Blocking

1. **Canary survived: `canaries/isolation-lock-pool-worktree-naming-run.patch`.**
   Wrong check: `isolation__left_no_trace`, `snapshot` and `traces`
   (`xtask/src/isolation.rs:121-144, 262-275`).
   - The patch makes `wt --version`, when an int check runs it, lock slot `app-wt1` with
     `git worktree lock --reason <its sandbox foreground>`. After the run, `git worktree list`
     shows `locked <canary clone>/verify/out/canary-1792793569176/sandbox/release-99228-0/pool/fg`
     (`canary-run.txt`, job8b). The lock stays: `git worktree remove` and `prune` refuse the slot
     until someone unlocks it. The run passes: "2 part runs left no trace".
   - Spec: verification.md:124-125. The check fails when the run left the run folder in
     `git -C ~/code/app worktree list`. Here the run folder is in that listing.
   - Cause: `snapshot` keeps only the `worktree`, `HEAD` and `prunable` fields of the listing.
     `traces` looks for the run folder only in worktree paths (`isolation.rs:262-266`).
   - Fix: record whether the whole porcelain listing names the run folder, with `mentions`
     (`isolation.rs:237`), as the snapshot already does for wt-activity records
     (`isolation.rs:114`). Fail when the listing after a part names it. Add the case to the
     `traces` table. This is oracle code, so it needs an entry.
   - Weight: a product would have to lock a real slot with a reason that names its foreground.
     That is unlikely, but the spec names this place, and the fix is small.

## Should fix (non blocking)

1. **The unit layer gives the builder's tests the real `TMPDIR`** (`xtask/src/parts.rs:68-92,
   582-589`). `Ctx::command` sets the sandbox `WT_FOREGROUND` and `WT_SLOT_DIR`, but no `TMPDIR`.
   The builder's tests make their pools with `tempfile::tempdir()` (`crates/wt/tests/cli.rs:132`)
   and run `wt` there, so `wt` gets a foreground and `HOME` in `<tmp>/`,
   outside the run folder. Seen in batch 2: the first version of the lock canary locked from such a
   call, with reason `<tmp>/.tmp8kbE6f/app`. verification.md:115-116 says
   `wt` gets `TMPDIR` in the run folder. No check depends on it today and `tempfile` cleans up, so
   this does not block. Fix: set `TMPDIR` to `<run>/sandbox/tmp` in `Ctx::command`, or at least
   for the `unit` command.

## Notes

- Experiment `experiments/isolation-empty-pool-worktree-folder.patch`, not a canary: `wt --version`
  deletes every file of slot `app-wt1` except its `.git` link. Uncommitted work is lost, and the run
  passes. No place in verification.md:124-128 changes: the listing, every HEAD and every ref stay
  the same. This is an open spec question, not a finding against the oracle.
- A lock whose reason does not name the run folder also stays unseen. Like the round 3 branch move
  question, whether any other change to the listing is a trace is an open spec question.
- `snapshot__should_record_each_worktree_whose_folder_is_gone` (`isolation.rs:808-855`) takes two
  `bool` columns, where `(true, true)` cannot occur. An enum of the three cases would make the
  table exhaustive by type. Style only.
- The round 5 note still applies: the `traces` table has no case where the fallback identity
  before a part differs from the one after it.
- `p0r3-rebuild-edit` and `p0r5-worktree-list-failure` are deferred, so they are not open.

## Earlier findings

| Round 5 finding | Status |
|---|---|
| Blocking 1, `isolation-delete-pool-worktree-folder.patch` survived | Closed. The round 5 file, on its fake pool under `v5/h2`, is caught: "int: the folder of ~/code/app worktree .../app-wt1 is gone". |
| Blocking 2, plumbing helper `pool` without `must_` | Closed. It is `must_pool` (`isolation.rs:709`), and all three callers use the new name. |
| Should fix 1, a failed worktree listing passes silently | Deferred under `p0r5-worktree-list-failure`. Not open. |
| Should fix 2, `fallback_email` sets `GIT_CONFIG_NOSYSTEM` | Deferred under the same entry. Not open. |

Earlier survivors, original files: round 3 `isolation-move-pool-head` and
`isolation-add-pool-worktree`, round 4 `isolation-remove-pool-worktree` and
`version-release-never-dirty`, round 5 `isolation-move-pool-worktree` and
`isolation-unit-test-moves-pool-head`. All are caught. The other survivors of rounds 1 to 4 are
byte for byte the adopted copies in `verify/canaries/phase-0/`, which are caught. The round 1 file
`suppress-in-target-module.patch` reports SURVIVED, but it lacks its module file and does not build,
as round 2 noted. Its adopted copy is caught.

## Oracle changes since the freeze

None. The clone is at the round 6 freeze with a clean tree, and the report says "none" for oracle
changes and for `verify/accepted.txt`. The author diff from the round 5 freeze to the round 6 freeze changes
`xtask/src/isolation.rs`, under `p0r5-worktree-folder-deleted`, accepted at
`verify/accepted.txt:19`. It also adopts `version-release-misses-an-edit.patch` into
`verify/canaries/phase-0/`, which is in `verify/oracle.lock:42`, as earlier rounds adopted
canaries.

## Author diff from the round 5 freeze to the round 6 freeze against the spec

- `isolation__left_no_trace`: `snapshot` now records each listed worktree whose folder or `.git`
  link is gone, from the `prunable` line or a missing `.git` (`isolation.rs:138-142`). `traces`
  fails on such a worktree when it had its folder before the part (`isolation.rs:285-293`). The
  listing names both cases, so this tests what verification.md:124-125 says. End to end:
  - the round 5 folder deletion is caught;
  - my `isolation-delete-pool-worktree-git-link` (only `.git` deleted) is caught;
  - my `isolation-delete-locked-pool-worktree-folder` (git never marks a locked worktree
    prunable, so only the `.git` test sees it) is caught.
- Tests: three new `traces` cases (`isolation.rs:561-591`) cover a folder lost during the part,
  one gone before it, and a new worktree without its folder. They are exhaustive for the new rule
  and do not overlap. `snapshot__should_record_each_worktree_whose_folder_is_gone` checks the
  snapshot on a real git pool for the folder, the link and an untouched slot. All use
  `<sut>__should_<assertion>` names and bare Given, When, Then. See the style note above.

## Lines no oracle layer covers

These lines are the same as in rounds 4 and 5.
- `crates/wt/src/main.rs:347` (unit only): justified. The `unknown` commit text runs only when
  git fails at build time, and no harness build makes git fail.
- `crates/wt/src/main.rs:353` (unit only): justified. The `none` features text.
  `release__should_have_no_verify_feature` and `version__` run it through the release binary,
  which has no coverage instrumentation.

## Code against phase 0 and earlier

Phase 0 has no behaviour ID. Only `wt --version` and the harness apply. `git log` from the version build fix to the round 6 freeze
shows no change to `crates/`, `README.md`, `Cargo.toml`, `Cargo.lock` or `.cargo/`. Rounds 2 to 5
reviewed this code. No new finding. The lead run's rebuild steps
(`verify/out/phase0-r6/rebuild/steps.json`) show all 8 debug and release states, with the right
commit and dirty marker.

## Screenshots

Phase 0 has no `shot` or `e2e` layer, and the run has no screenshots. Nothing to compare.

## Verify run

`verify/out/phase0-r6` says PASS. Its isolation snapshots show the 4 real pool worktrees before
and after each part, none with a missing folder.

## Canaries

43 counted runs: 42 caught, 1 survived. Not counted: the round 1 original that does not build, the
superseded first version of the lock canary, and 1 experiment.
- All 29 canaries in `verify/canaries/phase-0/` are caught at the round 6 freeze.
- Earlier survivors and round 5 isolation canaries, original files: 7, all caught.
- New: `isolation-delete-pool-worktree-git-link`, `isolation-delete-locked-pool-worktree-folder`,
  `isolation-branch-at-verify-commit`, `isolation-write-pool-wt-toml`,
  `isolation-settings-name-wt-verify` and `isolation-record-in-pool-activity` are caught. The last
  four are the first canaries for the branch, `wt.toml`, settings and wt-activity rules.
  `isolation-lock-pool-worktree-naming-run` survived: blocking 1.
- The set covers all four phase 0 categories: it breaks `wt --version`, stops a frozen check,
  edits an oracle file and adds a `CLAUDE.md`.

verdict: not clean
