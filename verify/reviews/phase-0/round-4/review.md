# Verifier review, phase 0, round 4

- Clone: `~/.cache/wt-verify/clones/phase-0-r4` at the round 4 freeze (product unchanged since the version build fix)
- Spec and freeze: docs/design/ at the round 4 freeze; author diff from the round 3 freeze to the round 4 freeze
- Run reviewed: `verify/out/phase0-r4`, verdict PASS
- Canaries: `canaries/` (3 new), experiment scripts in `experiments/`, results in `canary-run.txt`

All runs used scratch clones of the round 4 freeze, each with its own `target/` copied by APFS clone from the
clone above. `HOME`, `WT_FOREGROUND` and `WT_SLOT_DIR` were temporary folders. Only `CARGO_HOME`,
`RUSTUP_HOME` and `SCCACHE_DIR` pointed at the real ones (`experiments/env.sh`). Each temporary
`HOME` holds a `.gitconfig` with `user.email = h@example.invalid`, in the role of the owner's global
config, and a fake pool built by `experiments/mkpool.sh`: main worktree, slot `app-wt1`, branch
`side`, tag `owner-base`, commits by `h`, no `user.*` in the pool config. The two round 3
isolation canaries name the fake pool under `<scratchpad>/home`, my removal canary the one under
`<scratchpad>/v4/h4`. No run touched the real pool. I read its reflogs read only.

## Blocking

1. **Canary survived: `canaries/version-release-stale-commit.patch`.**
   Wrong check: `version__should_list_version_commit_and_features` (`xtask/tests/int.rs:21-26,
   63-89`), with the release build it gets (`xtask/src/parts.rs:630-657`).
   - The patch makes `build.rs` rerun for a release build only when `build.rs` changes. Debug
     builds keep rerunning on every build.
   - `experiments/release-stale-after-commit.sh` shows the break: after the next commit,
     `cargo build --release` (the README.md:12 install command) still prints the old commit, and
     the debug build prints the new one (`canary-run.txt`, section "release staleness").
   - Cause: the rebuild steps (`parts.rs:470-523`) build and copy only `target/debug/wt`
     (`parts.rs:427`). The release binary is built once, at the run's tree state, right after
     the canary changed `build.rs`, so it names the right commit.
   - The same single state limits the release dirty marker. The lead's clones are clean, so a
     real run only checks that a clean release build has no marker.
     `canaries/version-release-never-dirty.patch` is caught only because it asks for
     `Tree: uncommitted`.
   - Fix: run the four rebuild steps for the release profile too (`--release`, copy
     `target/release/wt`), and assert the commit and the marker per step, as for debug.

2. **Canary survived: `canaries/isolation-remove-pool-worktree.patch`.**
   Wrong check: `isolation__left_no_trace`, `traces` (`xtask/src/isolation.rs:256-268`).
   - The patch makes `wt --version` run `git worktree remove --force` on slot `app-wt1` of the
     pool, as a `wt rm` or `wt free` that reached the real pool would. The run passes: "2 part
     runs left no trace", and the slot folder is gone (`canary-run.txt`).
   - Cause: `traces` walks only `after.worktrees` and never compares it with
     `before.worktrees`. The HEAD reflog part (`isolation.rs:190-215`) cannot see it either:
     `git worktree remove` moves no ref, and the removed worktree's reflog goes with its admin
     folder. By reading, `git worktree move` passes the same way.
   - Spec: verification.md:124-129 names `git -C ~/code/app worktree list`. The exemption
     covers only the owner's ref moves. Oracle change `p0-isolation-scope` narrowed new worktrees
     to the ones the run created, and says nothing about removed ones.
   - Fix: fail when a worktree of `before.worktrees` is missing from `after.worktrees`, and add
     the cases to the `traces` table. A slot the owner removes during a run then fails the
     run, which the spec allows. The owner can rerun.

## Should fix

1. **The HEAD reflog check fails a run on the owner's own work under a throwaway identity.**
   Check I think wrong: `isolation__left_no_trace`, `foreign_head_entries` and its comparison in
   `traces` (`xtask/src/isolation.rs:210, 270-294`).
   - verification.md:128-129: "Your own work on the pool during a run moves refs and must
     not fail the run."
   - The owner's own agents commit in pool slots under throwaway identities. The real pool has
     `t <t@t>` (`~/code/app/.git/worktrees/app-wt1/logs/HEAD:8`, a merge) and `sim <s@s>`
     (`app-wt3/logs/HEAD:3`, a commit). The lead's run lists both as foreign entries
     (`verify/out/phase0-r4/isolation/*.json`).
   - `experiments/owner-throwaway-identity.sh`: during `int`, the owner commits in `app-wt1`
     with `-c user.email=t@t`. Verdict FAIL: "the HEAD of ~/code/app worktree .../app-wt1
     moved under t@t, not the owner's h@example.invalid".
   - My control run shows the owner's normal work passes: during `int`, the owner checks out
     `owner-base` in `app-wt1`, adds `app-wt2`, commits in main and runs `git branch -f`.
     Verdict PASS.
   - Not blocking: accepted change `p0r3-isolation-identity` asks for exactly this rule, the
     check never passes a broken product, and the failure names the identity. Two ways out:
     keep it and rerun on such a failure, or fail only on identities a run's process can name
     (`verify@wt.invalid` and git's passwd fallback, which `git var GIT_COMMITTER_IDENT` gives
     under a cleared environment). The second misses a builder's test that sets its own
     `user.email`.

2. **`owner_email` reads only the global config, outside any repository**
   (`xtask/src/isolation.rs:172-187`, `current_dir(home)`).
   - An address set through `includeIf "gitdir:..."` or in the pool's own config is not found.
     With no address, every reflog entry counts as foreign (`isolation.rs:210`), so any HEAD move
     of the owner's fails the run.
   - Today `~/.gitconfig` sets `user.email` directly, so this does not fail now.
   - Fix: ask git in the pool for the address it would use (`git -C <pool> config user.email`,
     with `HOME` set and `GIT_CONFIG_GLOBAL` removed). That resolves conditional includes.

## Notes

- `owner_email__should_read_the_global_config_of_home` (`isolation.rs:675-715`) has no case with
  `XDG_CONFIG_HOME` set, so the `Some(dir)` branch (`isolation.rs:180`) is untested.
- `foreign_head_entries` keys entries by worktree path. A `git worktree move` by the owner during a
  run puts that worktree's old foreign entries (the `t@t` one above) under a new key, so they
  count as added.
- Round 3 note on branch moves (`experiments/isolation-move-pool-branch.patch` of round 3) is
  still an open spec question. No oracle change resolves it.
- The author added three canaries to `verify/canaries/phase-0/` under the two accepted entries,
  as earlier rounds did. They are in `verify/oracle.lock`.
- `parts.rs:76-77` changed only a comment. `REBUILD_EDITED_FILE` is still `README.md`
  (`parts.rs:20`), as deferred entry `p0r3-rebuild-edit` says.

## Earlier findings

| Round 3 finding | Status |
|---|---|
| Blocking 1, `version-release-drops-commit.patch` survived | Closed. Caught: "no full commit ... in the release build's \"wt 0.1.0\ncommit unknown\nfeatures: none\n\"". Blocking 1 above is a new gap in the same check. |
| Blocking 2, `isolation-move-pool-head.patch` and `isolation-add-pool-worktree.patch` survived | Closed. Both caught, original files, fake pool as their headers say: "moved under you@mac.example, not the owner's h@example.invalid". |
| Should fix 1, rebuild edits `README.md` | Deferred: `p0r3-rebuild-edit`, status deferred. Not open. |

Round 1 and 2 findings were closed in rounds 2 and 3.

## Oracle changes since the freeze

None. The clone is at the round 4 freeze, and the report says "none" for oracle and
`verify/accepted.txt` changes. The two blocking round 3 entries, `p0r3-release-version`
(`xtask/tests/int.rs`) and `p0r3-isolation-identity` (`isolation.rs`, `parts.rs`,
`support/mod.rs`), are accepted at `verify/accepted.txt:13-14`. Every file the author changed
from the round 3 freeze is under one of them. `edit-oracle-under-round-2-entry.patch` and the other oracle
canaries are caught at the round 4 freeze.

## Author diff from the round 3 freeze to the round 4 freeze against the spec

- `version__should_list_version_commit_and_features`: now runs the release binary and asserts
  exit 0, empty stderr, the version, the full commit `head` and the marker. Matches the spec
  and closes round 3 blocking 1, at one tree state only: blocking 1.
- `isolation__left_no_trace`: the HEAD reflog part now fails on any added entry whose identity
  is not the owner's. It sees processes with a cleared environment, which closes round 3
  blocking 2. It also fails on the owner's throwaway identities: should fix 1. Worktree removal
  stays unseen: blocking 2.
- Tests: `traces` gained `earlier_entry_expired` and `expired_and_moved`, which closes the round
  3 note. `owner_email__` and `reflog_email__` are rstest tables, and
  `foreign_head_entries__should_name_only_worktrees_another_identity_moved` covers the cleared
  environment. All use `<sut>__should_<assertion>` names and bare Given/When/Then. Gap: the XDG
  case in Notes.

## Lines no oracle layer covers

From `verify/out/phase0-r4` and my control run, both "42 of 42".
- `crates/wt/src/main.rs:347` (unit only): justified. The `unknown` commit text runs only when
  git fails at build time, which no harness build does.
- `crates/wt/src/main.rs:353` (unit only): justified. The `none` features text.
  `release__should_have_no_verify_feature` and now `version__` run it through the
  uninstrumented release binary.

## Code against phase 0 and earlier

Phase 0 has no behaviour ID. Only `wt --version` and the harness apply. The product is unchanged
since the version build fix (`git log` on `crates/`, `README.md`, `Cargo.toml`), which rounds 2 and 3 reviewed.
`wt --version` and `-V` print version, full commit, ` (dirty)` and features, exit 0 and write
nothing to stderr. The `version_text` table (`main.rs:396-444`) is exhaustive with no overlap.

## Screenshots

Phase 0 has no `shot` or `e2e` layer and the run has no screenshots. Nothing to compare.

## Verify run

`verify/out/phase0-r4` says PASS. My control run of the round 4 freeze (quick, int, report), with the owner
moving refs in the fake pool during `int`, also gives PASS, with the same two coverage lines.

## Canaries

32 runs: 30 caught, 2 survived. 2 experiments, not counted.
- Round 3 survivors: `version-release-drops-commit.patch`, `isolation-move-pool-head.patch` and
  `isolation-add-pool-worktree.patch` are caught (round 3 files).
- All 26 canaries in `verify/canaries/phase-0/` are caught at the round 4 freeze.
- New: `version-release-never-dirty.patch` (`Tree: uncommitted`) is caught by the new release
  assertion. `version-release-stale-commit.patch` and `isolation-remove-pool-worktree.patch`
  survived: blocking 1 and 2.
- The set breaks `wt --version`, stops a frozen check, edits an oracle file and adds a
  `CLAUDE.md`, so all four phase 0 categories hold.

verdict: not clean
