# Verifier review, phase 0, round 5

- Clone: `~/.cache/wt-verify/clones/phase-0-r5` at the round 5 freeze (product unchanged since the version build fix)
- Spec and freeze: docs/design/ at the round 5 freeze. Author diff: from the round 4 freeze to the round 5 freeze
- Run reviewed: `verify/out/phase0-r5`, verdict PASS
- Canaries: `canaries/` (4 new). Runner and helpers: `experiments/`. Results: `canary-run.txt`

All canary runs used 4 scratch clones of the round 5 freeze. Each clone had its own temporary `HOME`, so it
also had its own canary clone. Each target folder was copied with `cp -c -R` from the clone above.
`WT_FOREGROUND` and `WT_SLOT_DIR` were also temporary. Only `CARGO_HOME`, `RUSTUP_HOME` and
`SCCACHE_DIR` were the real ones (`experiments/env.sh`). Each isolation canary ran against a fake
pool that `experiments/mkpool.sh` built under its temporary `HOME`. No run touched the real pool.
No process of these runs is left.

## Blocking

1. **Canary survived: `canaries/isolation-delete-pool-worktree-folder.patch`.**
   Wrong check: `isolation__left_no_trace`, `snapshot` and `traces`
   (`xtask/src/isolation.rs:117-133, 265-273`).
   - The patch makes `wt --version` delete the folder of slot `app-wt1`, as a `wt rm` that
     falls back to deleting files would. That loses any uncommitted work in the slot. The run
     passes: "2 part runs left no trace" (`canary-run.txt`, job2).
   - Cause: git keeps the slot's admin folder, so `git worktree list --porcelain` still lists
     the path, now with `prunable gitdir file points to non-existent location`. `snapshot` keeps
     only the `worktree` line, so the new removal check (oracle change `p0r4-worktree-removed`)
     sees no change. No HEAD moves either.
   - Spec: verification.md:124-125 names `git -C ~/code/app worktree list`, and its output
     changed. `p0r4-worktree-removed` gives the reason "a run that removes ... a real pool
     worktree passes", and this run still does.
   - Fix: record per worktree whether its folder exists, or its `prunable` line. Fail when a
     worktree that had its folder before the part has none after it. Add the case to the
     `traces` table.

2. **Plumbing test helper without `must_`: `pool`** (`xtask/src/isolation.rs:658`).
   - It runs `git init` and `git commit` and panics when they fail, so it can fail when the code
     under test is right. `verify/standards/engineering-standards.md:209-218` asks for the
     `must_` prefix on such helpers. verification.md, "Test standards", makes that file binding.
   - Fix: rename it to `must_pool`. This is oracle code, so it needs an entry. It can share the
     entry for finding 1.

## Should fix (non blocking)

1. **A failed worktree listing passes silently** (`xtask/src/isolation.rs:117`).
   `unwrap_or_default` turns a failed `git worktree list` into an empty list. If it fails before
   a part, a removal during that part goes unseen. If it fails after a part, every worktree is
   reported lost. Record the failure in the snapshot and fail on it, as `traces` already does for
   an unknown fallback identity (`isolation.rs:274-278`).
2. **`fallback_email` reads no system config** (`xtask/src/isolation.rs:168-176`).
   It sets `GIT_CONFIG_NOSYSTEM=1`, but a process with a cleared environment reads the system
   config. If that config ever sets `user.email`, a run's HEAD moves would carry that address
   and pass. Today the system config here (Homebrew git 2.51.1) sets only `credential.helper`, so
   this does not fail now. Drop the variable, so the computed identity is the one such a
   process writes.

## Notes

- The `traces` table has no case where the fallback identity before a part differs from the one
  after it (for example, the host name changed). The code takes both (`isolation.rs:280-287`).
- `p0r4-identity-scope` misses a builder's test that sets its own `user.email`. That gap was
  accepted with the entry, so it is not an open finding.
- The round 3 question about branch moves (`round-3/experiments/isolation-move-pool-branch.patch`)
  is still an open spec question. No oracle change resolves it.
- `p0r3-rebuild-edit` is deferred, so it is not open.

## Earlier findings

| Round 4 finding | Status |
|---|---|
| Blocking 1, `version-release-stale-commit.patch` survived | Closed. The round 4 file and the adopted copy are both caught: "the release rebuild after an empty commit in one target folder ... names another commit". |
| Blocking 2, `isolation-remove-pool-worktree.patch` survived | Closed. The original file was run against its fake pool under `v4/h4` and is caught: "~/code/app lost or moved its worktree .../app-wt1". Blocking 1 above is a new gap in the same check. |
| Should fix 1, the owner's throwaway identity fails a run | Closed by `p0r4-identity-scope`. Experiment (job1 in `canary-run.txt`): during `int`, the owner commits in `app-wt1` as `t <t@t>`, commits in main under their own identity and checks out `owner-base`. Verdict PASS. |
| Should fix 2, `owner_email` reads only the global config | Closed. `p0r4-identity-scope` removed `owner_email`. |

The round 3 survivors `isolation-move-pool-head.patch` and `isolation-add-pool-worktree.patch`
(original files, fake pool under `<scratchpad>/home`) are still caught under `p0r4-identity-scope`: "moved
under you@mac.example, an identity of the run". Findings of rounds 1 to 3 were closed earlier.

## Oracle changes since the freeze

None. The clone is at the round 5 freeze, and the report says "none" for oracle changes and for
`verify/accepted.txt`. The three round 4 entries are accepted at `verify/accepted.txt:15-18`:
`p0r4-release-rebuild`, `p0r4-worktree-removed` and `p0r4-identity-scope`. Each file
the author changed since the round 4 freeze (`isolation.rs`, `parts.rs`, `int.rs`) falls under one of them.
The two adopted canaries are in `verify/oracle.lock`.

## Author diff from the round 4 freeze to the round 5 freeze against the spec

- `version__should_list_version_commit_and_features`: the rebuild steps now build the `verify`
  debug build and the release build (`cargo build --release`, as README.md:12 installs it) at
  each of the 4 tree states. They assert the commit and the dirty marker per step
  (`parts.rs:424-450, 506-509`, `int.rs:21-30, 96-118`). The lead's run shows 8 steps with the
  right states. This matches the spec and closes round 4 blocking 1. My new
  `version-release-misses-an-edit.patch` (release build reruns only on a HEAD reflog change) is
  caught at "release rebuild after an edit".
- `isolation__left_no_trace`:
  - A worktree present before a part and missing after it now fails the part
    (`isolation.rs:265-273`). The removal and my `worktree move` canary are caught. The folder
    deletion is not: blocking 1.
  - HEAD moves now fail only under `verify@wt.invalid` or git's fallback identity
    (`isolation.rs:280-309`), as `p0r4-identity-scope` says. An unknown fallback identity fails the run,
    which is the stricter reading. My `isolation-unit-test-moves-pool-head.patch` (a builder
    unit test moves a slot's HEAD) is caught.
- Tests: the `traces` table gained exhaustive cases, with no overlap, for removal, move, unknown
  identity and each identity class. `fallback_email__` checks the address against what a git with
  a cleared environment really writes. `head_entries__` covers the main worktree and a slot. All
  use `<sut>__should_<assertion>` names and bare Given, When, Then. Gap: blocking 2.

## Lines no oracle layer covers

These lines are the same as in round 4.
- `crates/wt/src/main.rs:347` (unit only): justified. The `unknown` commit text runs only when
  git fails at build time, and no harness build makes git fail.
- `crates/wt/src/main.rs:353` (unit only): justified. The `none` features text.
  `release__should_have_no_verify_feature` and `version__` run it through the release binary,
  which has no coverage instrumentation.

## Code against phase 0 and earlier

Phase 0 has no behaviour ID. Only `wt --version` and the harness apply. `git log` from the version build fix to the round 5 freeze
shows no change to `crates/`, `README.md` or `Cargo.toml`. Rounds 2 to 4 reviewed this code. No
new finding.

## Screenshots

Phase 0 has no `shot` or `e2e` layer, and the run has no screenshots. Nothing to compare.

## Verify run

`verify/out/phase0-r5` says PASS. My owner experiment run of the round 5 freeze in a scratch clone also says
PASS.

## Canaries

37 runs: 36 caught, 1 survived. 1 experiment, not counted.
- Earlier survivors, original files: round 3 `isolation-move-pool-head` and
  `isolation-add-pool-worktree`, round 4 `isolation-remove-pool-worktree`,
  `version-release-stale-commit` and `version-release-never-dirty`. All are caught.
- All 28 canaries in `verify/canaries/phase-0/` are caught at the round 5 freeze.
- New: `isolation-move-pool-worktree`, `isolation-unit-test-moves-pool-head` and
  `version-release-misses-an-edit` are caught. `isolation-delete-pool-worktree-folder`
  survived: blocking 1.
- The set covers all four phase 0 categories: it breaks `wt --version`, stops a frozen check,
  edits an oracle file and adds a `CLAUDE.md`.

verdict: not clean
